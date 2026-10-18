//! Tauri commands exposed to the settings window.

use serde_json::Value;
use tauri::State;

use crate::state::AppState;

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
