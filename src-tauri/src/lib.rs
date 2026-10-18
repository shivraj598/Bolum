//! Bolum: push-to-talk dictation with local Whisper and NVIDIA ONNX engines.

mod commands;
mod models;
mod settings;
mod state;

use tauri::Manager;

use crate::state::AppState;

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let first_run = !settings::settings_path().exists();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_setting,
            commands::get_status,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            app.manage(AppState::new());
            let start_hidden = app
                .state::<AppState>()
                .settings
                .0
                .lock()
                .unwrap()
                .bool("start_hidden");
            if first_run || !start_hidden {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bolum");
}
