//! Optional LLM post-processing through any OpenAI-compatible endpoint
//! (Ollama, llama.cpp server, LM Studio, OpenRouter, OpenAI, ...).

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

/// Sends the transcript to the configured chat completions endpoint and
/// returns the cleaned text.
pub async fn enhance(text: &str, cfg: &Map<String, Value>) -> Result<String> {
    let base_url = cfg
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end_matches('/');
    let model = cfg.get("model").and_then(Value::as_str).unwrap_or_default();
    if base_url.is_empty() || model.is_empty() {
        return Err(anyhow!("post-processing needs a base URL and a model"));
    }
    let prompt = cfg
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or("Clean up this dictation and reply with only the text.");
    let api_key = cfg.get("api_key").and_then(Value::as_str).unwrap_or_default();

    let mut request = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?
        .post(format!("{base_url}/chat/completions"))
        .json(&json!({
            "model": model,
            "temperature": 0.2,
            "messages": [
                { "role": "system", "content": prompt },
                { "role": "user", "content": text },
            ],
        }));
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        let detail: String = response
            .text()
            .await
            .unwrap_or_default()
            .trim()
            .chars()
            .take(200)
            .collect();
        return Err(anyhow!("post-processing endpoint returned {status}: {detail}"));
    }
    let body: Value = response.json().await?;
    let cleaned = body["choices"][0]["message"]["content"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("post-processing response had no text"))?;
    Ok(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    /// Answers one request with `status` and `body`; the handle yields the raw request.
    async fn serve_once(status: &'static str, body: &'static str) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                // Stop once the headers and the declared body have arrived.
                let text = String::from_utf8_lossy(&request).to_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request).into_owned()
        });
        (base_url, server)
    }

    fn config(base_url: &str, api_key: &str) -> Map<String, Value> {
        json!({ "base_url": base_url, "model": "llama3.2", "api_key": api_key, "prompt": "Fix punctuation." })
            .as_object()
            .cloned()
            .unwrap()
    }

    #[tokio::test]
    async fn sends_a_chat_request_and_returns_the_trimmed_answer() {
        let (base_url, server) =
            serve_once("200 OK", r#"{"choices":[{"message":{"content":"  Hello, world.  "}}]}"#).await;
        let cleaned = enhance("hello world", &config(&base_url, "secret")).await.unwrap();
        assert_eq!(cleaned, "Hello, world.");

        let request = server.await.unwrap();
        let lower = request.to_lowercase();
        // The trailing slash on the base URL must not double up.
        assert!(request.starts_with("POST /v1/chat/completions "), "{request}");
        assert!(lower.contains("authorization: bearer secret"));
        assert!(request.contains(r#""model":"llama3.2""#));
        assert!(request.contains(r#""content":"Fix punctuation.""#));
        assert!(request.contains(r#""content":"hello world""#));
    }

    #[tokio::test]
    async fn omits_the_authorization_header_without_a_key() {
        let (base_url, server) = serve_once("200 OK", r#"{"choices":[{"message":{"content":"ok"}}]}"#).await;
        enhance("text", &config(&base_url, "")).await.unwrap();
        assert!(!server.await.unwrap().to_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn reports_the_status_and_detail_of_a_failed_request() {
        let (base_url, server) = serve_once("500 Internal Server Error", "model not loaded").await;
        let error = enhance("text", &config(&base_url, "")).await.unwrap_err().to_string();
        server.await.unwrap();
        assert!(error.contains("500"), "{error}");
        assert!(error.contains("model not loaded"), "{error}");
    }

    #[tokio::test]
    async fn rejects_an_empty_answer() {
        let (base_url, server) = serve_once("200 OK", r#"{"choices":[{"message":{"content":"   "}}]}"#).await;
        let error = enhance("text", &config(&base_url, "")).await.unwrap_err().to_string();
        server.await.unwrap();
        assert!(error.contains("no text"), "{error}");
    }

    #[tokio::test]
    async fn needs_a_base_url_and_a_model() {
        let mut cfg = config("", "");
        assert!(enhance("text", &cfg).await.is_err());
        cfg.insert("base_url".into(), json!("http://127.0.0.1:9/v1"));
        cfg.insert("model".into(), json!(""));
        assert!(enhance("text", &cfg).await.unwrap_err().to_string().contains("base URL and a model"));
    }
}
