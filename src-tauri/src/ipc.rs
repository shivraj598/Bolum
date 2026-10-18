//! Control socket so `bolum toggle` (or a compositor keybinding) can drive the
//! running app without a second GUI instance.

use anyhow::{anyhow, Result};
use std::io::{Read, Write};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::settings::runtime_dir;
use crate::state::AppState;

pub fn socket_path() -> PathBuf {
    runtime_dir().join("bolum.sock")
}

/// Serves one-line commands on the control socket.
pub fn spawn_server(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let path = socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = match tokio::net::UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(err) => {
                log::error!("control socket at {}: {err}", path.display());
                return;
            }
        };
        log::info!("control socket listening at {}", path.display());
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            let app = app.clone();
            tokio::spawn(async move {
                let mut buffer = [0u8; 128];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let command = String::from_utf8_lossy(&buffer[..read]).trim().to_string();
                let response = tokio::task::spawn_blocking(move || handle(&app, &command))
                    .await
                    .unwrap_or_else(|err| format!("error: {err}"));
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
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
    let path = socket_path();
    let mut stream = std::os::unix::net::UnixStream::connect(&path)
        .map_err(|err| anyhow!("Bolum is not running ({}): {err}", path.display()))?;
    stream.write_all(command.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response.trim().to_string())
}

/// True for arguments that the CLI handles instead of starting a GUI.
pub fn is_control_command(argument: &str) -> bool {
    matches!(
        argument,
        "toggle" | "start" | "stop" | "cancel" | "status" | "quit"
    )
}
