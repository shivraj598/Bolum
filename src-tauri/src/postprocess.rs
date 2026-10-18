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
