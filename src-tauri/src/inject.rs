//! Text delivery into the focused application.
//!
//! Mirrors Handy's strategy: typing tools are detected on PATH (`wtype`,
//! `kwtype`, `dotool`, `ydotool`, `xdotool`) and the transcript is either typed
//! directly or written to the clipboard first and then pasted. On macOS the
//! built-in `native` tool posts CGEvents instead, which needs the
//! Accessibility permission.

use anyhow::{anyhow, Result};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::settings::SettingsStore;

#[cfg(not(target_os = "macos"))]
const TOOLS: [&str; 5] = ["wtype", "kwtype", "dotool", "ydotool", "xdotool"];
#[cfg(target_os = "macos")]
const TOOLS: [&str; 1] = ["native"];

pub fn which(program: &str) -> Option<std::path::PathBuf> {
    let paths = std::env::var("PATH").unwrap_or_default();
    paths
        .split(':')
        .map(|dir| std::path::Path::new(dir).join(program))
        .find(|candidate| candidate.is_file())
}

pub fn available_tools() -> Vec<&'static str> {
    TOOLS
        .into_iter()
        .filter(|t| *t == "native" || which(t).is_some())
        .collect()
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
        #[cfg(target_os = "macos")]
        "native" => native::type_text(text),
        other => Err(anyhow!("cannot type with '{other}'")),
    }
}

fn write_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    return native::write_clipboard(text);
    #[allow(unreachable_code)]
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
        #[cfg(target_os = "macos")]
        ("native", key) => native::send_key(key),
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

/// Prompts for the Accessibility permission up front so the first dictation
/// is not the moment macOS asks for it.
pub fn request_permissions() -> Result<()> {
    #[cfg(target_os = "macos")]
    native::request_permissions()?;
    Ok(())
}

#[cfg(target_os = "macos")]
mod native {
    use anyhow::{anyhow, Result};
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn connect() -> Result<Enigo> {
        Enigo::new(&Settings::default()).map_err(|err| {
            anyhow!(
                "{err}; allow Bolum under System Settings → Privacy & Security → Accessibility"
            )
        })
    }

    pub fn request_permissions() -> Result<()> {
        connect().map(drop)
    }

    pub fn type_text(text: &str) -> Result<()> {
        connect()?
            .text(text)
            .map_err(|err| anyhow!("typing failed: {err}"))
    }

    pub fn send_key(key: &str) -> Result<()> {
        let mut enigo = connect()?;
        match key {
            "paste" => {
                enigo.key(Key::Meta, Direction::Press)?;
                // Always release Command, even when the V press fails.
                let pressed = enigo.key(Key::Unicode('v'), Direction::Click);
                enigo.key(Key::Meta, Direction::Release)?;
                pressed?;
            }
            "enter" => enigo.key(Key::Return, Direction::Click)?,
            other => return Err(anyhow!("cannot send {other}")),
        }
        Ok(())
    }

    /// `pbcopy` decodes stdin with the locale, which is unset for apps
    /// launched from Finder, so pin UTF-8 or accents arrive garbled.
    pub fn write_clipboard(text: &str) -> Result<()> {
        let mut child = Command::new("/usr/bin/pbcopy")
            .env("LANG", "en_US.UTF-8")
            .stdin(Stdio::piped())
            .spawn()?;
        if let Some(pipe) = child.stdin.as_mut() {
            pipe.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            return Err(anyhow!("pbcopy exited with {status}"));
        }
        Ok(())
    }
}
