//! Shared application state.

use crate::audio::AudioRecorder;
use crate::models::Downloads;
use crate::settings::Settings;

pub struct AppState {
    pub settings: Settings,
    pub recorder: AudioRecorder,
    pub downloads: Downloads,
}

impl AppState {
    pub fn new(recorder: AudioRecorder) -> Self {
        Self {
            settings: Settings::new(),
            recorder,
            downloads: Downloads::default(),
        }
    }
}
