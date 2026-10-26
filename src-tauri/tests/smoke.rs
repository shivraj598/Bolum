//! End-to-end smoke test against the real model host: download Whisper Tiny
//! (English), verify its SHA-256, and transcribe a known sentence.
//!
//! It needs the network and a 44 MB download, so it is ignored by default:
//!
//! ```sh
//! cargo test --test smoke -- --ignored --nocapture
//! ```
//!
//! Models and settings go to `BOLUM_SMOKE_DIR`, or to Cargo's per-target
//! scratch directory, so repeat runs reuse the download and never touch the
//! real Bolum data folders.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use bolum_lib::engines::Loaded;
use bolum_lib::models;
use bolum_lib::settings::SettingsStore;

const MODEL: &str = "whisper-tiny.en";

fn isolate_data_dirs() {
    let root = std::env::var_os("BOLUM_SMOKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("smoke"));
    std::env::set_var("XDG_DATA_HOME", root.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
}

fn fixture_samples() -> Vec<f32> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/quick-brown-fox.wav");
    let mut reader = hound::WavReader::open(path).expect("open the fixture");
    let spec = reader.spec();
    assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 16_000, 16));
    reader
        .samples::<i16>()
        .map(|sample| sample.expect("read a sample") as f32 / i16::MAX as f32)
        .collect()
}

/// Lowercase words only, so punctuation and casing don't decide the test.
fn words(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "downloads a model; run with --ignored"]
fn downloads_verifies_and_transcribes() {
    isolate_data_dirs();
    let model = models::find(MODEL).expect("the smoke model is in the catalog");

    if models::is_downloaded(model) {
        println!("reusing {}", models::model_dir(MODEL).display());
    } else {
        let runtime = tokio::runtime::Runtime::new().expect("a tokio runtime");
        runtime
            .block_on(models::download(model, Arc::new(AtomicBool::new(false)), |_, _| {}))
            .expect("download and verify the model");
    }
    assert!(models::is_downloaded(model), "every file is present at its pinned size");
    // A download only lands after its checksum matched; re-check the file on disk anyway.
    let file = &model.files[0];
    let actual = models::sha256_file(&models::path_of(model, file.name)).expect("hash the model");
    assert_eq!(actual, file.sha256, "the model on disk matches its pinned SHA-256");

    let mut settings = SettingsStore::load();
    settings.set_quiet("language", "en".into());
    let mut engine = Loaded::load(model).expect("load whisper.cpp");
    let started = std::time::Instant::now();
    let text = engine.transcribe(&fixture_samples(), &settings).expect("transcribe");
    println!("[{} ms] {text}", started.elapsed().as_millis());

    let heard = words(&text);
    for phrase in ["quick brown fox", "lazy dog"] {
        assert!(heard.contains(phrase), "expected {phrase:?} in {text:?}");
    }
}
