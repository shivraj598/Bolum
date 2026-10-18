//! Whisper engine backed by whisper.cpp through `whisper-rs`.

use anyhow::Result;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    pub fn load(model_path: &std::path::Path) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_string_lossy().as_ref(),
            WhisperContextParameters::default(),
        )?;
        Ok(Self { ctx })
    }

    /// `language` is a Whisper code or "auto" to auto-detect.
    pub fn transcribe(&self, samples: &[f32], language: &str, translate: bool) -> Result<String> {
        let mut state = self.ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_translate(translate);
        params.set_language(if language == "auto" { None } else { Some(language) });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_n_threads(
            std::thread::available_parallelism()
                .map(|n| n.get().min(8) as i32)
                .unwrap_or(4),
        );

        state.full(params, samples)?;

        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_str_lossy()?);
        }
        Ok(text.trim().to_string())
    }
}
