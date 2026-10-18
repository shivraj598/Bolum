//! Shared application state.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::audio::AudioRecorder;
use crate::engines::Loaded;
use crate::models::Downloads;
use crate::settings::Settings;

pub struct AppState {
    pub settings: Settings,
    pub recorder: AudioRecorder,
    /// One resident model: `(catalog id, engine)`.
    pub loaded: Mutex<Option<(String, Loaded)>>,
    pub downloads: Downloads,
    pub started_at_ms: AtomicU64,
    pub last_transcript: Mutex<Option<String>>,
    /// Accelerator currently registered with the global shortcut plugin.
    pub hotkey: Mutex<Option<String>>,
    phase: AtomicU8,
}

impl AppState {
    pub fn new(recorder: AudioRecorder) -> Self {
        Self {
            settings: Settings::new(),
            recorder,
            loaded: Mutex::new(None),
            downloads: Downloads::default(),
            started_at_ms: AtomicU64::new(0),
            last_transcript: Mutex::new(None),
            hotkey: Mutex::new(None),
            phase: AtomicU8::new(0),
        }
    }

    pub fn phase(&self) -> &'static str {
        match self.phase.load(Ordering::Relaxed) {
            1 => "listening",
            2 => "working",
            _ => "idle",
        }
    }

    pub fn is_phase(&self, phase: &str) -> bool {
        self.phase() == phase
    }

    pub fn set_phase(&self, phase: &str) {
        let value = match phase {
            "listening" => 1,
            "working" => 2,
            _ => 0,
        };
        self.phase.store(value, Ordering::Relaxed);
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
