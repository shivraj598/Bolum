//! Shared application state.

use crate::settings::Settings;

pub struct AppState {
    pub settings: Settings,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            settings: Settings::new(),
        }
    }
}
