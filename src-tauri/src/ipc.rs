//! Control channel so `bolum toggle` (or a compositor keybinding) can drive the
//! running app without a second GUI instance: a Unix socket on macOS and
//! Linux, a named pipe on Windows. Each connection carries one command and
//! gets one reply.

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::state::AppState;

/// Turns a command into the reply sent back to the client.
type Handler = Arc<dyn Fn(String) -> String + Send + Sync>;

/// Where the running app listens.
pub fn endpoint() -> String {
    #[cfg(unix)]
    return crate::settings::runtime_dir()
        .join("bolum.sock")
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    return r"\\.\pipe\bolum".to_string();
}

/// Serves control commands for the app until it exits.
pub fn spawn_server(app: AppHandle) {
    let handler: Handler = Arc::new(move |command| handle(&app, &command));
    tauri::async_runtime::spawn(async move {
        let endpoint = endpoint();
        if let Err(err) = serve(&endpoint, handler).await {
            log::error!("control channel at {endpoint}: {err}");
        }
    });
}

/// Answers every connection on `endpoint` with `handler`'s reply.
async fn serve(endpoint: &str, handler: Handler) -> std::io::Result<()> {
    let mut listener = transport::Listener::bind(endpoint)?;
    log::info!("control channel listening at {endpoint}");
    loop {
        let mut stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(err) => {
                log::warn!("control channel accept failed: {err}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        let handler = Arc::clone(&handler);
        tokio::spawn(async move {
            let mut buffer = [0u8; 128];
            let read = stream.read(&mut buffer).await.unwrap_or(0);
            let command = String::from_utf8_lossy(&buffer[..read]).trim().to_string();
            let response = tokio::task::spawn_blocking(move || handler(command))
                .await
                .unwrap_or_else(|err| format!("error: {err}"));
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        });
    }
}

fn handle(app: &AppHandle, command: &str) -> String {
    let result = match command {
        "toggle" => crate::pipeline::toggle(app),
        "start" => crate::pipeline::start(app),
        "stop" => crate::pipeline::stop(app),
        "cancel" => crate::pipeline::cancel(app),
        "status" => Ok(app.state::<AppState>().phase().to_string()),
        "quit" => {
            app.exit(0);
            Ok("bye".into())
        }
        other => Err(anyhow!(
            "unknown command '{other}' (expected toggle, start, stop, cancel, status or quit)"
        )),
    };
    match result {
        Ok(message) => message,
        Err(err) => format!("error: {err}"),
    }
}

/// Client side, used by the `bolum <command>` CLI.
pub fn send(command: &str) -> Result<String> {
    let endpoint = endpoint();
    transport::request(&endpoint, command)
        .map(|response| response.trim().to_string())
        .map_err(|err| anyhow!("Bolum is not running ({endpoint}): {err}"))
}

#[cfg(unix)]
mod transport {
    use std::io::{Read, Write};
    use tokio::net::{UnixListener, UnixStream};

    pub struct Listener(UnixListener);

    impl Listener {
        pub fn bind(endpoint: &str) -> std::io::Result<Self> {
            // A socket file left by a crashed instance would block the bind.
            let _ = std::fs::remove_file(endpoint);
            UnixListener::bind(endpoint).map(Self)
        }

        pub async fn accept(&mut self) -> std::io::Result<UnixStream> {
            self.0.accept().await.map(|(stream, _)| stream)
        }
    }

    pub fn request(endpoint: &str, command: &str) -> std::io::Result<String> {
        let mut stream = std::os::unix::net::UnixStream::connect(endpoint)?;
        stream.write_all(command.as_bytes())?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }
}

#[cfg(windows)]
mod transport {
    use std::io::{Read, Write};
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    /// Keeps one pipe instance waiting for the next client.
    pub struct Listener {
        endpoint: String,
        next: NamedPipeServer,
    }

    impl Listener {
        pub fn bind(endpoint: &str) -> std::io::Result<Self> {
            // Fails while another Bolum owns the pipe, like a second bind would.
            let next = ServerOptions::new().first_pipe_instance(true).create(endpoint)?;
            Ok(Self {
                endpoint: endpoint.to_string(),
                next,
            })
        }

        pub async fn accept(&mut self) -> std::io::Result<NamedPipeServer> {
            self.next.connect().await?;
            let fresh = ServerOptions::new().create(&self.endpoint)?;
            Ok(std::mem::replace(&mut self.next, fresh))
        }
    }

    pub fn request(endpoint: &str, command: &str) -> std::io::Result<String> {
        let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(endpoint)?;
        // A pipe can't be half-closed, so a newline ends the command.
        pipe.write_all(format!("{command}\n").as_bytes())?;
        let mut response = String::new();
        pipe.read_to_string(&mut response)?;
        Ok(response)
    }
}

/// True for arguments that the CLI handles instead of starting a GUI.
pub fn is_control_command(argument: &str) -> bool {
    matches!(
        argument,
        "toggle" | "start" | "stop" | "cancel" | "status" | "quit"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_every_control_command() {
        for command in ["toggle", "start", "stop", "cancel", "status", "quit"] {
            assert!(is_control_command(command), "{command} should be a control command");
        }
    }

    #[test]
    fn other_arguments_start_the_app() {
        for argument in ["", "--help", "Toggle", "open", "toggle "] {
            assert!(!is_control_command(argument), "{argument:?} should not be a control command");
        }
    }

    fn test_endpoint(name: &str) -> String {
        #[cfg(unix)]
        return std::env::temp_dir()
            .join(format!("bolum-{}-{name}.sock", std::process::id()))
            .to_string_lossy()
            .into_owned();
        #[cfg(windows)]
        return format!(r"\\.\pipe\bolum-test-{}-{name}", std::process::id());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commands_round_trip_through_the_control_channel() {
        let endpoint = test_endpoint("round-trip");
        let handler: Handler = Arc::new(|command| format!("got {command}"));
        let server = tokio::spawn({
            let endpoint = endpoint.clone();
            async move { serve(&endpoint, handler).await }
        });

        let replies = tokio::task::spawn_blocking(move || {
            // The server binds asynchronously; retry the first request briefly.
            let first = (0..100)
                .find_map(|_| {
                    transport::request(&endpoint, "status").ok().or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        None
                    })
                })
                .expect("the control channel never answered");
            let second = transport::request(&endpoint, "toggle").expect("a second request");
            (first, second)
        })
        .await
        .unwrap();
        server.abort();

        assert_eq!(replies.0.trim(), "got status");
        assert_eq!(replies.1.trim(), "got toggle");
    }
}
