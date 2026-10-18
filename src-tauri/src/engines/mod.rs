//! Engine dispatch: one cached model is kept resident at a time.

pub mod whisper;

use anyhow::Result;

use crate::models::{CatalogModel, Engine};
use crate::settings::SettingsStore;

pub enum Loaded {
    Whisper(whisper::WhisperEngine),
}

impl Loaded {
    pub fn load(model: &CatalogModel) -> Result<Self> {
        match model.engine {
            Engine::Whisper => Ok(Self::Whisper(whisper::WhisperEngine::load(&crate::models::path_of(
                model,
                "model.bin",
            ))?)),
        }
    }

    pub fn transcribe(&mut self, samples: &[f32], settings: &SettingsStore) -> Result<String> {
        let language = settings.str("language");
        let target = settings.str("translate_to");
        match self {
            Self::Whisper(engine) => {
                engine.transcribe(samples, &language, !target.is_empty() && target != "off")
            }
        }
    }
}
