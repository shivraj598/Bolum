//! Engine dispatch: one cached model is kept resident at a time.

pub mod onnx;
pub mod whisper;

use anyhow::Result;

use crate::models::{CatalogModel, Engine};
use crate::settings::SettingsStore;

pub enum Loaded {
    Whisper(whisper::WhisperEngine),
    Parakeet(onnx::ParakeetEngine),
    Canary(onnx::CanaryEngine),
}

impl Loaded {
    pub fn load(model: &CatalogModel) -> Result<Self> {
        let dir = crate::models::model_dir(model.id);
        match model.engine {
            Engine::Whisper => Ok(Self::Whisper(whisper::WhisperEngine::load(
                &crate::models::path_of(model, "model.bin"),
            )?)),
            Engine::Parakeet => Ok(Self::Parakeet(onnx::ParakeetEngine::load(&dir)?)),
            Engine::Canary => Ok(Self::Canary(onnx::CanaryEngine::load(&dir)?)),
        }
    }

    pub fn transcribe(&mut self, samples: &[f32], settings: &SettingsStore) -> Result<String> {
        let language = settings.str("language");
        let target = settings.str("translate_to");
        match self {
            Self::Whisper(engine) => {
                engine.transcribe(samples, &language, !target.is_empty() && target != "off")
            }
            Self::Parakeet(engine) => engine.transcribe(samples),
            Self::Canary(engine) => engine.transcribe(samples, &language, &target),
        }
    }
}
