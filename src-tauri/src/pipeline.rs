//! Dictation pipeline: capture -> transcribe -> inject -> history.

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Emitter, Manager};

use crate::engines::Loaded;
use crate::state::{now_ms, AppState};
use crate::{history, inject, models};

/// Starts listening, unless already active.
pub fn start(app: &AppHandle) -> Result<String> {
    let state = app.state::<AppState>();
    if !state.is_phase("idle") {
        return Ok(format!("already {}", state.phase()));
    }
    let model_id = state.settings.0.lock().unwrap().str("model");
    let model = models::find(&model_id)?;
    if !models::is_downloaded(model) {
        let _ = app.emit(
            "notice",
            format!("Model '{}' is not downloaded yet", model.name),
        );
        return Err(anyhow!("model '{}' is not downloaded", model_id));
    }

    let microphone = state.settings.0.lock().unwrap().opt_str("microphone");
    state.recorder.start(microphone)?;
    state.started_at_ms.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
    state.set_phase("listening");
    crate::update_tray(app, "listening");
    if state.settings.0.lock().unwrap().bool("show_overlay") {
        show_overlay(app, true);
    }
    let _ = app.emit("phase", "listening");
    Ok("listening".into())
}

/// Stops listening and delivers the transcript in the background.
pub fn stop(app: &AppHandle) -> Result<String> {
    let state = app.state::<AppState>();
    if !state.is_phase("listening") {
        return Ok(format!("not listening (currently {})", state.phase()));
    }
    state.set_phase("working");
    crate::update_tray(app, "working");
    let _ = app.emit("phase", "working");
    let samples = match state.recorder.stop() {
        Ok(samples) => samples,
        Err(err) => {
            // Never leave the app wedged in "working" when the mic misbehaves.
            finish(app);
            return Err(err);
        }
    };
    let duration_ms = now_ms().saturating_sub(state.started_at_ms.load(std::sync::atomic::Ordering::Relaxed));

    if samples.len() < crate::audio::TARGET_RATE as usize / 10 {
        finish(app);
        let _ = app.emit("notice", "Recording was too short");
        return Ok("too short".into());
    }
    deliver(app.clone(), samples, duration_ms);
    Ok("transcribing".into())
}

pub fn cancel(app: &AppHandle) -> Result<String> {
    let state = app.state::<AppState>();
    if state.is_phase("listening") {
        state.recorder.cancel();
    }
    finish(app);
    Ok("cancelled".into())
}

pub fn toggle(app: &AppHandle) -> Result<String> {
    if app.state::<AppState>().is_phase("listening") {
        stop(app)
    } else {
        start(app)
    }
}

/// Transcription, text injection and history in one task.
fn deliver(app: AppHandle, samples: Vec<f32>, duration_ms: u64) {
    tauri::async_runtime::spawn(async move {
        let transcript = tauri::async_runtime::spawn_blocking({
            let app = app.clone();
            move || -> Result<String> {
                let state = app.state::<AppState>();
                let model_id = state.settings.0.lock().unwrap().str("model");
                let model = models::find(&model_id)?;
                let stale = {
                    let loaded = state.loaded.lock().unwrap();
                    loaded.as_ref().map(|(id, _)| id != &model_id).unwrap_or(true)
                };
                if stale {
                    log::info!("loading model {model_id}");
                    // Load outside the lock: engines read hundreds of megabytes
                    // and the UI keeps using the state meanwhile.
                    let engine = Loaded::load(model)?;
                    *state.loaded.lock().unwrap() = Some((model_id.clone(), engine));
                }
                // Clone the settings so no guard is held during transcription.
                let settings = state.settings.0.lock().unwrap().clone();
                let mut loaded = state.loaded.lock().unwrap();
                let (_, engine) = loaded.as_mut().unwrap();
                engine.transcribe(&samples, &settings)
            }
        })
        .await;

        let text = match transcript {
            Ok(Ok(text)) => text,
            Ok(Err(err)) => {
                let _ = app.emit("notice", format!("Transcription failed: {err}"));
                finish(&app);
                return;
            }
            Err(err) => {
                let _ = app.emit("notice", format!("Transcription task failed: {err}"));
                finish(&app);
                return;
            }
        };
        if text.trim().is_empty() {
            let _ = app.emit("notice", "Nothing was recognised");
            finish(&app);
            return;
        }

        // Inject into the focused window and record history.
        let inject_result = tauri::async_runtime::spawn_blocking({
            let app = app.clone();
            let text = text.clone();
            move || -> Result<()> {
                let state = app.state::<AppState>();
                let settings = state.settings.0.lock().unwrap().clone();
                inject::inject(&text, &settings)
            }
        })
        .await;
        match inject_result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                let _ = app.emit("notice", format!("Could not type the transcript: {err}"));
            }
            Err(err) => {
                let _ = app.emit("notice", format!("Injection task failed: {err}"));
            }
        }

        let model_id = app.state::<AppState>().settings.0.lock().unwrap().str("model");
        let entry = history::Entry {
            id: now_ms(),
            timestamp_ms: now_ms(),
            text: text.clone(),
            post_processed: None,
            model: model_id,
            duration_ms,
        };
        if let Err(err) = history::append(&entry) {
            log::warn!("could not write history: {err}");
        }
        let limit = app.state::<AppState>().settings.0.lock().unwrap().usize("history_limit");
        let _ = history::trim(limit);
        *app.state::<AppState>().last_transcript.lock().unwrap() = Some(text.clone());

        let _ = app.emit("transcript", text);
        finish(&app);
    });
}

/// Returns to idle: hides the overlay and refreshes the tray icon.
pub fn finish(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.set_phase("idle");
    show_overlay(app, false);
    crate::update_tray(app, "idle");
    let _ = app.emit("phase", "idle");
}

pub fn show_overlay(app: &AppHandle, visible: bool) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = if visible { window.show() } else { window.hide() };
    }
}
