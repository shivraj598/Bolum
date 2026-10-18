//! Tauri commands exposed to the settings window.

use serde_json::Value;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

use crate::models::{self, ModelStatus};
use crate::state::AppState;
use crate::{inject, pipeline};

fn message(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Value {
    state.settings.0.lock().unwrap().json()
}

#[tauri::command]
pub fn set_setting(app: AppHandle, state: State<AppState>, key: String, value: Value) -> Value {
    state.settings.0.lock().unwrap().set(&key, value);
    match key.as_str() {
        "hotkey" => crate::apply_hotkey(&app),
        "model" => {
            // Drop the resident engine so the new model loads on next use.
            *state.loaded.lock().unwrap() = None;
        }
        _ => {}
    }
    state.settings.0.lock().unwrap().json()
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> String {
    state.phase().to_string()
}

#[tauri::command]
pub fn get_microphones() -> Vec<String> {
    crate::audio::input_devices()
}

#[tauri::command]
pub fn get_typing_tools() -> Vec<String> {
    inject::available_tools().into_iter().map(str::to_string).collect()
}

#[tauri::command]
pub fn list_models(state: State<AppState>) -> Vec<ModelStatus> {
    state.downloads.status()
}

#[tauri::command]
pub async fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let model = models::find(&id).map_err(message)?;
    let cancel = state
        .downloads
        .start(&id)
        .ok_or_else(|| format!("'{id}' is already downloading"))?;
    let result = models::download(&app, model, cancel).await;
    state.downloads.finish(&id);
    result.map_err(message)
}

#[tauri::command]
pub fn cancel_download(state: State<AppState>, id: String) {
    state.downloads.cancel(&id);
}

#[tauri::command]
pub fn delete_model(state: State<AppState>, id: String) -> Result<(), String> {
    let model = models::find(&id).map_err(message)?;
    if state.settings.0.lock().unwrap().str("model") == id {
        *state.loaded.lock().unwrap() = None;
    }
    // Stop a running download before its directory disappears underneath it.
    state.downloads.cancel(&id);
    models::delete(model).map_err(message)
}

#[tauri::command]
pub fn start_recording(app: AppHandle) -> Result<String, String> {
    pipeline::start(&app).map_err(message)
}

#[tauri::command]
pub fn stop_recording(app: AppHandle) -> Result<String, String> {
    pipeline::stop(&app).map_err(message)
}

#[tauri::command]
pub fn cancel_recording(app: AppHandle) -> Result<String, String> {
    pipeline::cancel(&app).map_err(message)
}

#[tauri::command]
pub fn copy_last(state: State<AppState>) -> Result<(), String> {
    let text = state.last_transcript.lock().unwrap().clone();
    match text {
        Some(text) => inject::copy_to_clipboard(&text).map_err(message),
        None => Err("no transcript yet".into()),
    }
}

#[tauri::command]
pub fn show_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn recording_elapsed_ms(state: State<AppState>) -> u64 {
    if state.phase() != "listening" {
        return 0;
    }
    crate::state::now_ms().saturating_sub(state.started_at_ms.load(Ordering::Relaxed))
}
