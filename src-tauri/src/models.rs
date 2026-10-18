//! Model catalog and verified downloads.
//!
//! Every artifact is pinned by SHA-256 from its Hugging Face LFS object id and
//! re-verified after download, the same guarantee Handy gives for its hosted
//! blobs. Downloads resume from `.part` files.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

use crate::settings::models_dir;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Whisper,
}

pub struct FileSpec {
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

pub struct CatalogModel {
    pub id: &'static str,
    pub name: &'static str,
    pub engine: Engine,
    pub description: &'static str,
    pub languages: &'static str,
    pub files: &'static [FileSpec],
}

pub const CATALOG: &[CatalogModel] = &[
    CatalogModel {
        id: "whisper-base.en",
        name: "Whisper Base (English)",
        engine: Engine::Whisper,
        description: "Small, fast English model. Good default for dictation.",
        languages: "en",
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin",
            sha256: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
            size: 59_721_011,
        }],
    },
    CatalogModel {
        id: "whisper-small",
        name: "Whisper Small (multilingual)",
        engine: Engine::Whisper,
        description: "Multilingual Whisper with automatic language detection.",
        languages: "99 languages",
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
            sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
            size: 190_085_487,
        }],
    },
    CatalogModel {
        id: "whisper-large-v3-turbo",
        name: "Whisper Large v3 Turbo",
        engine: Engine::Whisper,
        description: "Most accurate Whisper. Slower and a large download.",
        languages: "99 languages",
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
            sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
            size: 574_041_195,
        }],
    },
];

pub fn model_dir(id: &str) -> PathBuf {
    models_dir().join(id)
}

pub fn find(id: &str) -> Result<&'static CatalogModel> {
    CATALOG
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| anyhow!("unknown model '{id}'"))
}

pub fn total_size(model: &CatalogModel) -> u64 {
    model.files.iter().map(|f| f.size).sum()
}

pub fn is_downloaded(model: &CatalogModel) -> bool {
    model.files.iter().all(|f| {
        std::fs::metadata(model_dir(model.id).join(f.name))
            .map(|m| m.len() == f.size)
            .unwrap_or(false)
    })
}

pub fn partial_size(model: &CatalogModel) -> u64 {
    model
        .files
        .iter()
        .map(|f| {
            std::fs::metadata(model_dir(model.id).join(format!("{}.part", f.name)))
                .map(|m| m.len())
                .unwrap_or(0)
        })
        .sum()
}

pub fn path_of(model: &CatalogModel, file: &str) -> PathBuf {
    model_dir(model.id).join(file)
}

#[derive(Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    pub description: String,
    pub languages: String,
    pub size_mb: u64,
    pub downloaded: bool,
    pub downloading: bool,
    pub progress: u32,
}

#[derive(Default)]
pub struct Downloads {
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Downloads {
    pub fn is_downloading(&self, id: &str) -> bool {
        self.cancels.lock().unwrap().contains_key(id)
    }

    /// Registers a download, or returns `None` when one is already running.
    pub fn start(&self, id: &str) -> Option<Arc<AtomicBool>> {
        let mut cancels = self.cancels.lock().unwrap();
        if cancels.contains_key(id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        cancels.insert(id.to_string(), Arc::clone(&flag));
        Some(flag)
    }

    pub fn finish(&self, id: &str) {
        self.cancels.lock().unwrap().remove(id);
    }

    pub fn cancel(&self, id: &str) {
        if let Some(flag) = self.cancels.lock().unwrap().get(id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn status(&self) -> Vec<ModelStatus> {
        CATALOG
            .iter()
            .map(|model| {
                let downloaded = is_downloaded(model);
                let downloading = self.is_downloading(model.id);
                let partial = partial_size(model);
                let total = total_size(model).max(1);
                let progress = if downloaded {
                    100
                } else {
                    ((partial.min(total) * 100) / total) as u32
                };
                ModelStatus {
                    id: model.id.to_string(),
                    name: model.name.to_string(),
                    engine: model.engine,
                    description: model.description.to_string(),
                    languages: model.languages.to_string(),
                    size_mb: total_size(model) / 1_000_000,
                    downloaded,
                    downloading,
                    progress,
                }
            })
            .collect()
    }
}

/// Downloads every missing file of `model`, verifying SHA-256 before renaming.
pub async fn download(
    app: &AppHandle,
    model: &'static CatalogModel,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let dir = model_dir(model.id);
    std::fs::create_dir_all(&dir).context("create model directory")?;
    let total: u64 = total_size(model);
    let client = reqwest::Client::builder()
        .user_agent("bolum/0.1 (+https://github.com/shivraj598/Bolum)")
        .connect_timeout(std::time::Duration::from_secs(20))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()?;

    for file in model.files {
        let final_path = dir.join(file.name);
        if std::fs::metadata(&final_path).map(|m| m.len() == file.size).unwrap_or(false) {
            continue;
        }
        let part_path = dir.join(format!("{}.part", file.name));
        let mut already = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        if already >= file.size {
            // A part file that already has the full size only needs verifying.
            let complete = already == file.size
                && sha256_file(&part_path)
                    .map(|sum| sum.eq_ignore_ascii_case(file.sha256))
                    .unwrap_or(false);
            if complete {
                std::fs::rename(&part_path, &final_path)?;
                continue;
            }
            let _ = std::fs::remove_file(&part_path);
            already = 0;
        }

        let mut request = client.get(file.url);
        if already > 0 {
            request = request.header("Range", format!("bytes={already}-"));
        }
        let response = request.send().await?.error_for_status()?;
        if already > 0 && !resumes_at(&response, already) {
            already = 0; // the server ignored the range or started elsewhere
        }
        let mut stream = response.bytes_stream();
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(already > 0)
            .write(true)
            .truncate(already == 0)
            .open(&part_path)?;

        let done_before: u64 = model
            .files
            .iter()
            .take_while(|f| f.name != file.name)
            .map(|f| f.size)
            .sum();
        let mut received = already;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(anyhow!("download cancelled"));
            }
            let chunk = chunk?;
            out.write_all(&chunk)?;
            received += chunk.len() as u64;
            let _ = app.emit("model-progress", json!({ "id": model.id, "received": done_before + received, "total": total }));
        }
        out.flush()?;
        drop(out);

        let actual = sha256_file(&part_path)?;
        if !actual.eq_ignore_ascii_case(file.sha256) {
            let _ = std::fs::remove_file(&part_path);
            return Err(anyhow!(
                "checksum mismatch for {}: expected {}, got {}",
                file.name,
                file.sha256,
                actual
            ));
        }
        std::fs::rename(&part_path, &final_path)?;
    }
    Ok(())
}

/// True when a 206 response really continues the `.part` file at `already`.
fn resumes_at(response: &reqwest::Response, already: u64) -> bool {
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return false;
    }
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes "))
        .and_then(|value| value.split('-').next())
        .and_then(|value| value.trim().parse::<u64>().ok())
        == Some(already)
}

pub fn sha256_file(path: &PathBuf) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn delete(model: &CatalogModel) -> Result<()> {
    match std::fs::remove_dir_all(model_dir(model.id)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}
