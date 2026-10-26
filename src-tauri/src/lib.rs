//! Bolum: push-to-talk dictation with local Whisper and NVIDIA ONNX engines.

pub mod audio;
pub mod engines;
pub mod history;
pub mod ipc;
pub mod models;
pub mod settings;

mod commands;
mod inject;
mod pipeline;
mod postprocess;
mod state;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::state::AppState;

/// Swaps the tray icon between idle, recording and transcribing.
pub fn update_tray(app: &AppHandle, phase: &str) {
    let bytes: &[u8] = match phase {
        "listening" => include_bytes!("../icons/tray-recording.png"),
        "working" => include_bytes!("../icons/tray-transcribing.png"),
        _ => include_bytes!("../icons/tray-idle.png"),
    };
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(icon) = tauri::image::Image::from_bytes(bytes) {
            let _ = tray.set_icon(Some(icon));
        }
        let _ = tray.set_tooltip(Some(&format!("Bolum — {phase}")));
    }
}

/// Registers the configured global shortcut, replacing any previous one.
pub fn apply_hotkey(app: &AppHandle) {
    let accelerator = app.state::<AppState>().settings.0.lock().unwrap().str("hotkey");
    let shortcuts = app.global_shortcut();
    if let Some(previous) = app.state::<AppState>().hotkey.lock().unwrap().take() {
        let _ = shortcuts.unregister(previous.as_str());
    }
    if accelerator.trim().is_empty() {
        return;
    }
    match shortcuts.register(accelerator.as_str()) {
        Ok(()) => {
            log::info!("global shortcut registered: {accelerator}");
            *app.state::<AppState>().hotkey.lock().unwrap() = Some(accelerator);
        }
        Err(err) => {
            log::warn!("global shortcut '{accelerator}' unavailable: {err}");
            let _ = app.emit(
                "notice",
                format!("Shortcut '{accelerator}' unavailable ({err}); use `bolum toggle` from a keybinding"),
            );
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Start / stop dictation", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let copy_last = MenuItem::with_id(app, "copy_last", "Copy last transcript", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &settings_item,
            &copy_last,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/tray-idle.png"
        ))?)
        .tooltip("Bolum")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => {
                if let Err(err) = pipeline::toggle(app) {
                    let _ = app.emit("notice", err.to_string());
                }
            }
            "settings" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "copy_last" => {
                let text = app.state::<AppState>().last_transcript.lock().unwrap().clone();
                match text {
                    Some(text) => {
                        if let Err(err) = inject::copy_to_clipboard(&text) {
                            let _ = app.emit("notice", format!("Copy failed: {err}"));
                        }
                    }
                    None => {
                        let _ = app.emit("notice", "No transcript yet");
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let first_run = !settings::settings_path().exists();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    let push_to_talk = app
                        .state::<AppState>()
                        .settings
                        .0
                        .lock()
                        .unwrap()
                        .bool("push_to_talk");
                    match event.state() {
                        ShortcutState::Pressed => {
                            let result = if push_to_talk {
                                pipeline::start(app)
                            } else {
                                pipeline::toggle(app)
                            };
                            if let Err(err) = result {
                                let _ = app.emit("notice", err.to_string());
                            }
                        }
                        ShortcutState::Released if push_to_talk => {
                            let _ = pipeline::stop(app);
                        }
                        _ => {}
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_setting,
            commands::get_status,
            commands::get_microphones,
            commands::get_typing_tools,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::list_history,
            commands::delete_history,
            commands::copy_history,
            commands::copy_last,
            commands::show_window,
            commands::recording_elapsed_ms,
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
            // A menu-bar app: no Dock icon, like LSUIElement in the bundle.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let level_handle = handle.clone();
            let recorder = audio::AudioRecorder::new(Arc::new(move |level: f32| {
                let _ = level_handle.emit("level", level);
            }));
            let state = AppState::new(recorder);
            app.manage(state);

            if let Err(err) = build_tray(&handle) {
                log::error!("tray icon unavailable: {err}");
            }
            apply_hotkey(&handle);
            ipc::spawn_server(handle.clone());
            if let Err(err) = inject::request_permissions() {
                log::warn!("text insertion unavailable: {err}");
            }

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
