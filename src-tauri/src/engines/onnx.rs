//! NVIDIA NeMo ONNX engines: Parakeet TDT and Canary AED.
//!
//! Both pipelines follow the reference export contracts of the
//! `istupakov/*-onnx` conversions: NeMo log-mel features from `nemo128.onnx`,
//! greedy TDT decoding for Parakeet and greedy transformer decoding with a
//! prompt for Canary.

use anyhow::{anyhow, Context, Result};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::{PrimitiveTensorElementType, TensorRef};
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::Path;
use std::sync::OnceLock;

/// Borrows `data` as an ONNX tensor.
fn tensor<T>(shape: Vec<i64>, data: &[T]) -> Result<TensorRef<'_, T>>
where
    T: PrimitiveTensorElementType + Clone + Debug + 'static,
{
    TensorRef::from_array_view((shape.clone(), data))
        .map_err(|err| anyhow!("tensor {shape:?}: {err}"))
}

/// Runs a session with named inputs, converting `ort` errors to `anyhow`.
macro_rules! run {
    ($session:expr, $context:expr, [$($name:literal => $value:expr),+ $(,)?]) => {
        $session
            .run(ort::inputs![$($name => $value),+])
            .map_err(|err| anyhow!("{}: {}", $context, err))?
    };
}

fn environment() -> Result<&'static Environment> {
    static ENV: OnceLock<Environment> = OnceLock::new();
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init()
        .with_name("bolum")
        .build()
        .map_err(|err| anyhow!("onnx runtime init: {err}"))?;
    Ok(ENV.get_or_init(|| env))
}

fn session(path: &Path) -> Result<Session> {
    let builder = Session::builder(environment()?)
        .map_err(|err| anyhow!("create session builder: {err}"))?;
    let threads = std::thread::available_parallelism()
        .map(|count| count.get().min(8))
        .unwrap_or(4);
    builder
        .with_intra_threads(threads)
        .map_err(|err| anyhow!("set session threads: {err}"))?
        .commit_from_file(path)
        .map_err(|err| anyhow!("load {}: {err}", path.display()))
}

/// Reads a vocabulary file mapping tokens to ids, normalising the
/// SentencePiece space marker (`▁`) to a plain space like the reference
/// implementations do.
fn read_vocab(path: &Path) -> Result<(Vec<String>, HashMap<String, i64>)> {
    let raw = std::fs::read_to_string(path).context("read vocab.txt")?;
    let mut by_id: Vec<(i64, String)> = Vec::new();
    let mut by_token = HashMap::new();
    for line in raw.lines() {
        let Some((token, id)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(id) = id.trim().parse::<i64>() else {
            continue;
        };
        let token = token.replace('▁', " ");
        by_id.push((id, token.clone()));
        by_token.insert(token, id);
    }
    let size = by_id.iter().map(|(id, _)| *id as usize).max().unwrap_or(0) + 1;
    let mut tokens = vec![String::new(); size];
    for (id, token) in by_id {
        if let Some(slot) = tokens.get_mut(id as usize) {
            *slot = token;
        }
    }
    Ok((tokens, by_token))
}

/// Joins decoded ids, dropping NeMo control tokens and tidying the spacing.
fn tokens_to_text(tokens: &[String], ids: &[i64]) -> String {
    let text: String = ids
        .iter()
        .filter_map(|id| tokens.get(*id as usize))
        .filter(|token| !token.starts_with("<|"))
        .cloned()
        .collect();
    // Space runs collapse to a single space before a word and vanish
    // otherwise, so "Hello , world ." reads as "Hello, world." — the cleanup
    // the reference decoder does with `\A\s|\s\B|(\s)\b`.
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            pending_space = false;
            if !out.is_empty() && (c.is_alphanumeric() || c == '_') {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

/// Runs `nemo128.onnx` and returns the log-mel features and their shape.
fn mel_features(pre: &mut Session, samples: &[f32]) -> Result<(Vec<f32>, i64, i64, usize)> {
    let length = [samples.len() as i64];
    let outputs = run!(
        pre,
        "mel features",
        [
            "waveforms" => tensor(vec![1, length[0]], samples)?,
            "waveforms_lens" => tensor(vec![1], &length)?,
        ]
    );
    let (shape, features) = outputs["features"]
        .try_extract_tensor::<f32>()
        .map_err(|err| anyhow!("features: {err}"))?;
    let (_, feature_lengths) = outputs["features_lens"]
        .try_extract_tensor::<i64>()
        .map_err(|err| anyhow!("features_lens: {err}"))?;
    Ok((
        features.to_vec(),
        shape[1],
        shape[2],
        feature_lengths[0] as usize,
    ))
}

pub struct ParakeetEngine {
    pre: Session,
    encoder: Session,
    decoder: Session,
    tokens: Vec<String>,
    blank: i64,
    state_layers: i64,
    state_hidden: i64,
    max_tokens_per_step: usize,
}

impl ParakeetEngine {
    pub fn load(dir: &Path) -> Result<Self> {
        let pre = session(&dir.join("nemo128.onnx"))?;
        let encoder = session(&dir.join("encoder-model.int8.onnx"))?;
        let decoder = session(&dir.join("decoder_joint-model.int8.onnx"))?;
        let (tokens, by_token) = read_vocab(&dir.join("vocab.txt"))?;
        let blank = *by_token
            .get("<blk>")
            .ok_or_else(|| anyhow!("vocab.txt has no <blk> token"))?;
        let (state_layers, state_hidden) = input_shape(&decoder, "input_states_1", (2, 640));
        Ok(Self {
            pre,
            encoder,
            decoder,
            tokens,
            blank,
            state_layers,
            state_hidden,
            // NeMo TDT greedy decoding allows at most 10 tokens per encoder frame.
            max_tokens_per_step: 10,
        })
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let (features, bins, frames, valid) = mel_features(&mut self.pre, samples)?;
        let lengths = [valid as i64];
        let (hidden, time, encoded_len, encoded) = {
            let outputs = run!(
                self.encoder,
                "parakeet encoder",
                [
                    "audio_signal" => tensor(vec![1, bins, frames], &features)?,
                    "length" => tensor(vec![1], &lengths)?,
                ]
            );
            let (shape, data) = outputs["outputs"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("encoder outputs: {err}"))?;
            let (_, lens) = outputs["encoded_lengths"]
                .try_extract_tensor::<i64>()
                .map_err(|err| anyhow!("encoded_lengths: {err}"))?;
            (shape[1], shape[2], lens[0] as usize, data.to_vec())
        };

        let state_size = (self.state_layers * self.state_hidden) as usize;
        let mut state_1 = vec![0.0f32; state_size];
        let mut state_2 = vec![0.0f32; state_size];
        let mut emitted: Vec<i64> = Vec::new();
        let mut frame = 0usize;
        let mut tokens_this_frame = 0usize;

        while frame < encoded_len {
            // The encoder output is [1, hidden, time]; slice one time step to [1, hidden, 1].
            let step: Vec<f32> = (0..hidden as usize)
                .map(|index| encoded[index * time as usize + frame])
                .collect();
            let target = [emitted.last().copied().unwrap_or(self.blank) as i32];
            let target_length = [1i32];
            let outputs = run!(
                self.decoder,
                "parakeet decoder",
                [
                    "encoder_outputs" => tensor(vec![1, hidden, 1], &step)?,
                    "targets" => tensor(vec![1, 1], &target)?,
                    "target_length" => tensor(vec![1], &target_length)?,
                    "input_states_1" => tensor(vec![self.state_layers, 1, self.state_hidden], &state_1)?,
                    "input_states_2" => tensor(vec![self.state_layers, 1, self.state_hidden], &state_2)?,
                ]
            );
            let logits = outputs["outputs"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("decoder outputs: {err}"))?
                .1
                .to_vec();
            let next_state_1 = outputs["output_states_1"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("output_states_1: {err}"))?
                .1
                .to_vec();
            let next_state_2 = outputs["output_states_2"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("output_states_2: {err}"))?
                .1
                .to_vec();

            let vocabulary = self.blank as usize + 1;
            let (token, _) = logits[..vocabulary]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, &0.0));
            // The duration head follows the vocabulary: its index is the number
            // of encoder frames to skip.
            let step_frames = logits[vocabulary..]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(index, _)| index)
                .unwrap_or(0);
            let token = token as i64;

            if token != self.blank {
                state_1 = next_state_1;
                state_2 = next_state_2;
                emitted.push(token);
                tokens_this_frame += 1;
            }
            if step_frames > 0 {
                frame += step_frames;
                tokens_this_frame = 0;
            } else if token == self.blank || tokens_this_frame == self.max_tokens_per_step {
                frame += 1;
                tokens_this_frame = 0;
            }
        }

        Ok(tokens_to_text(&self.tokens, &emitted))
    }
}

pub struct CanaryEngine {
    pre: Session,
    encoder: Session,
    decoder: Session,
    tokens: Vec<String>,
    by_token: HashMap<String, i64>,
    decoder_layers: i64,
    decoder_hidden: i64,
}

impl CanaryEngine {
    pub fn load(dir: &Path) -> Result<Self> {
        let pre = session(&dir.join("nemo128.onnx"))?;
        let encoder = session(&dir.join("encoder-model.int8.onnx"))?;
        let decoder = session(&dir.join("decoder-model.int8.onnx"))?;
        let (tokens, by_token) = read_vocab(&dir.join("vocab.txt"))?;
        let (decoder_layers, decoder_hidden) = input_shape(&decoder, "decoder_mems", (6, 1024));
        Ok(Self {
            pre,
            encoder,
            decoder,
            tokens,
            by_token,
            decoder_layers,
            decoder_hidden,
        })
    }

    fn id(&self, token: &str) -> i64 {
        self.by_token.get(token).copied().unwrap_or(1)
    }

    /// `language` and `target` are NeMo language codes; `target` may equal
    /// `language` to transcribe instead of translate.
    pub fn transcribe(&mut self, samples: &[f32], language: &str, target: &str) -> Result<String> {
        let supported = |lang: &str| matches!(lang, "en" | "de" | "es" | "fr");
        let source = if supported(language) { language } else { "en" };
        let target = if supported(target) { target } else { source };

        let (features, bins, frames, valid) = mel_features(&mut self.pre, samples)?;
        let lengths = [valid as i64];
        let (embeddings, mask, time, hidden) = {
            let outputs = run!(
                self.encoder,
                "canary encoder",
                [
                    "audio_signal" => tensor(vec![1, bins, frames], &features)?,
                    "length" => tensor(vec![1], &lengths)?,
                ]
            );
            let (shape, embeddings) = outputs["encoder_embeddings"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("encoder_embeddings: {err}"))?;
            let (_, mask) = outputs["encoder_mask"]
                .try_extract_tensor::<i64>()
                .map_err(|err| anyhow!("encoder_mask: {err}"))?;
            (embeddings.to_vec(), mask.to_vec(), shape[1], shape[2])
        };

        // NeMo Canary prompt: context, transcript start, emotion, source and
        // target language, punctuation/casing, no ITN, no timestamps, no diarisation.
        let mut prompt = vec![
            self.id(" "),
            self.id("<|startofcontext|>"),
            self.id("<|startoftranscript|>"),
            self.id("<|emo:undefined|>"),
            self.id(&format!("<|{source}|>")),
            self.id(&format!("<|{target}|>")),
            self.id("<|pnc|>"),
            self.id("<|noitn|>"),
            self.id("<|notimestamp|>"),
            self.id("<|nodiarize|>"),
        ];
        let eos = self.id("<|endoftext|>");

        let mut mems_empty = vec![0.0f32; 0];
        let mut mems_length = 0i64;
        let mut generated: Vec<i64> = Vec::new();

        for _ in 0..1024 {
            // The whole prompt is fed once; afterwards only the newest token and
            // the cached decoder state are used.
            let input_ids: Vec<i64> = if mems_length == 0 {
                prompt.clone()
            } else {
                vec![*prompt.last().unwrap()]
            };
            let input_length = input_ids.len() as i64;
            let mems_shape = vec![self.decoder_layers, 1, mems_length, self.decoder_hidden];
            let outputs = run!(
                self.decoder,
                "canary decoder",
                [
                    "input_ids" => tensor(vec![1, input_length], &input_ids)?,
                    "encoder_embeddings" => tensor(vec![1, time, hidden], &embeddings)?,
                    "encoder_mask" => tensor(vec![1, time], &mask)?,
                    "decoder_mems" => tensor(mems_shape, &mems_empty)?,
                ]
            );
            let (logits_shape, logits) = outputs["logits"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("logits: {err}"))?;
            let vocabulary = logits_shape[2] as usize;
            let steps = logits_shape[1] as usize;
            let last = &logits[(steps - 1) * vocabulary..steps * vocabulary];
            let (next, _) = last
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, &0.0));
            let next = next as i64;

            let (mems_shape, mems) = outputs["decoder_hidden_states"]
                .try_extract_tensor::<f32>()
                .map_err(|err| anyhow!("decoder_hidden_states: {err}"))?;
            mems_length = mems_shape[2];
            mems_empty = mems.to_vec();
            if mems_length == 0 || next == eos {
                break;
            }
            generated.push(next);
            prompt.push(next);
        }

        Ok(tokens_to_text(&self.tokens, &generated))
    }
}

/// Reads one dimension of a session input's shape, falling back to NeMo's
/// published defaults when the graph leaves them dynamic.
fn input_shape(session: &Session, name: &str, fallback: (i64, i64)) -> (i64, i64) {
    let shape = session
        .inputs()
        .iter()
        .find(|input| input.name() == name)
        .and_then(|input| match input.dtype() {
            ort::value::ValueType::Tensor { shape, .. } => Some(shape.clone()),
            _ => None,
        });
    match shape {
        Some(shape) => {
            let first = shape.first().copied().filter(|dim| *dim > 0).unwrap_or(fallback.0);
            let last = shape.last().copied().filter(|dim| *dim > 0).unwrap_or(fallback.1);
            (first, last)
        }
        None => fallback,
    }
}
