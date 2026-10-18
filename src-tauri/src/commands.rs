//! Tauri commands exposed to the settings window.

use serde_json::Value;
use tauri::State;

use crate::models::{self, ModelStatus};
use crate::state::AppState;

fn message(err: impl std::fmt::Display) -> String {
    err.to_string()
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Value {
    state.settings.0.lock().unwrap().json()
}

#[tauri::command]
pub fn set_setting(state: State<AppState>, key: String, value: Value) -> Value {
    state.settings.0.lock().unwrap().set(&key, value);
    state.settings.0.lock().unwrap().json()
}

#[tauri::command]
pub fn get_status() -> String {
    "idle".into()
}

#[tauri::command]
pub fn get_microphones() -> Vec<String> {
    crate::audio::input_devices()
}

#[tauri::command]
pub fn list_models(state: State<AppState>) -> Vec<ModelStatus> {
    state.downloads.status()
}

#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
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
    // Stop a running download before its directory disappears underneath it.
    state.downloads.cancel(&id);
    models::delete(model).map_err(message)
}
