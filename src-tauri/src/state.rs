//! Shared application state.

use crate::models::Downloads;
use crate::settings::Settings;

pub struct AppState {
    pub settings: Settings,
    pub downloads: Downloads,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            settings: Settings::new(),
            downloads: Downloads::default(),
        }
    }
}
