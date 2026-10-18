//! Text delivery into the focused application.
//!
//! Mirrors Handy's strategy: typing tools are detected on PATH (`wtype`,
//! `kwtype`, `dotool`, `ydotool`, `xdotool`) and the transcript is either typed
//! directly or written to the clipboard first and then pasted.

use anyhow::{anyhow, Result};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::settings::SettingsStore;

const TOOLS: [&str; 5] = ["wtype", "kwtype", "dotool", "ydotool", "xdotool"];

pub fn which(program: &str) -> Option<std::path::PathBuf> {
    let paths = std::env::var("PATH").unwrap_or_default();
    paths
        .split(':')
        .map(|dir| std::path::Path::new(dir).join(program))
        .find(|candidate| candidate.is_file())
}

pub fn available_tools() -> Vec<&'static str> {
    TOOLS.into_iter().filter(|t| which(t).is_some()).collect()
}

fn resolve_tool(settings: &SettingsStore) -> Result<&'static str> {
    let requested = settings.str("typing_tool");
    if requested != "auto" {
        let tool = TOOLS
            .into_iter()
            .find(|t| *t == requested.as_str())
            .ok_or_else(|| anyhow!("unknown typing tool '{requested}'"))?;
        return Ok(tool);
    }
    available_tools()
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no typing tool found (install wtype, dotool or xdotool)"))
}

fn run_with_stdin(program: &str, args: &[&str], stdin: &str) -> Result<()> {
    let binary = which(program).ok_or_else(|| anyhow!("{program} not found"))?;
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(pipe) = child.stdin.as_mut() {
        pipe.write_all(stdin.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("{program} exited with {status}"));
    }
    Ok(())
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let binary = which(program).ok_or_else(|| anyhow!("{program} not found"))?;
    let status = Command::new(binary)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(anyhow!("{program} exited with {status}"));
    }
    Ok(())
}

fn type_text(tool: &str, text: &str) -> Result<()> {
    match tool {
        "wtype" => run_with_stdin("wtype", &["-"], text),
        "kwtype" => run_with_stdin("kwtype", &["-"], text),
        "ydotool" => run("ydotool", &["type", "--", text]),
        // dotool treats a newline in its command stream as Enter.
        "dotool" => run_with_stdin("dotool", &[], &format!("type {}\n", text.replace('\n', " "))),
        "xdotool" => run("xdotool", &["type", "--clearmodifiers", "--delay", "2", "--", text]),
        other => Err(anyhow!("cannot type with '{other}'")),
    }
}

fn write_clipboard(text: &str) -> Result<()> {
    if which("wl-copy").is_some() && std::env::var("WAYLAND_DISPLAY").is_ok() {
        return run_with_stdin("wl-copy", &[], text);
    }
    if which("xclip").is_some() {
        return run_with_stdin("xclip", &["-selection", "clipboard"], text);
    }
    if which("xsel").is_some() {
        return run_with_stdin("xsel", &["--clipboard", "--input"], text);
    }
    Err(anyhow!("no clipboard tool found (wl-copy, xclip or xsel)"))
}

fn send_key(tool: &str, key: &str) -> Result<()> {
    match (tool, key) {
        ("wtype", "paste") => run("wtype", &["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"]),
        ("wtype", "enter") => run("wtype", &["-P", "Return", "-p", "Return"]),
        ("kwtype", "paste") => run("kwtype", &["-M", "ctrl", "-P", "v", "-p", "v", "-m", "ctrl"]),
        ("kwtype", "enter") => run("kwtype", &["-P", "Return", "-p", "Return"]),
        ("xdotool", "paste") => run("xdotool", &["key", "--clearmodifiers", "ctrl+v"]),
        ("xdotool", "enter") => run("xdotool", &["key", "--clearmodifiers", "Return"]),
        ("ydotool", "paste") => run("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]),
        ("ydotool", "enter") => run("ydotool", &["key", "28:1", "28:0"]),
        ("dotool", "paste") => run_with_stdin("dotool", &[], "key ctrl+v\n"),
        ("dotool", "enter") => run_with_stdin("dotool", &[], "key enter\n"),
        (tool, key) => Err(anyhow!("{tool} cannot send {key}")),
    }
}

/// Copies `text` to the system clipboard (tray "copy last transcript", history).
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    write_clipboard(text)
}

/// Sends `text` to the focused window using the configured strategy.
pub fn inject(text: &str, settings: &SettingsStore) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    let mut payload = text.to_string();
    if settings.bool("append_trailing_space") {
        payload.push(' ');
    }
    let mode = settings.str("injection");
    if mode == "none" {
        return Ok(());
    }
    let tool = resolve_tool(settings)?;
    match mode.as_str() {
        "clipboard" => {
            write_clipboard(&payload)?;
            std::thread::sleep(std::time::Duration::from_millis(80));
            send_key(tool, "paste")?;
            if settings.bool("auto_submit") {
                send_key(tool, "enter")?;
            }
            Ok(())
        }
        _ => {
            type_text(tool, &payload)?;
            if settings.bool("auto_submit") {
                std::thread::sleep(std::time::Duration::from_millis(60));
                send_key(tool, "enter")?;
            }
            Ok(())
        }
    }
}
