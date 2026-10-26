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
    Parakeet,
    Canary,
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
    /// Relative 0..1 guides for the model cards, not benchmark results.
    pub accuracy: f32,
    pub speed: f32,
    /// Shown first, and suggested to new users.
    pub recommended: bool,
    pub files: &'static [FileSpec],
}

const NEMO_PRE: FileSpec = FileSpec {
    name: "nemo128.onnx",
    url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/nemo128.onnx",
    sha256: "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f",
    size: 139_764,
};

pub const CATALOG: &[CatalogModel] = &[
    CatalogModel {
        id: "whisper-base.en",
        name: "Whisper Base (English)",
        engine: Engine::Whisper,
        description: "Small and quick English model. A good default for dictation.",
        languages: "en",
        accuracy: 0.55,
        speed: 0.92,
        recommended: true,
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin",
            sha256: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
            size: 59_721_011,
        }],
    },
    CatalogModel {
        id: "parakeet-tdt-0.6b-v2",
        name: "NVIDIA Parakeet TDT 0.6B v2",
        engine: Engine::Parakeet,
        description: "Excellent English accuracy and still fast on CPU. ONNX int8.",
        languages: "en",
        accuracy: 0.88,
        speed: 0.80,
        recommended: true,
        files: &[
            FileSpec {
                name: "encoder-model.int8.onnx",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/encoder-model.int8.onnx",
                sha256: "3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155",
                size: 652_184_014,
            },
            FileSpec {
                name: "decoder_joint-model.int8.onnx",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/decoder_joint-model.int8.onnx",
                sha256: "a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3",
                size: 8_998_286,
            },
            NEMO_PRE,
            FileSpec {
                name: "vocab.txt",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main/vocab.txt",
                sha256: "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d",
                size: 9_384,
            },
        ],
    },
    CatalogModel {
        id: "parakeet-tdt-0.6b-v3",
        name: "NVIDIA Parakeet TDT 0.6B v3",
        engine: Engine::Parakeet,
        description: "Fast and accurate across 25 European languages. ONNX int8.",
        languages: "25 languages",
        accuracy: 0.86,
        speed: 0.80,
        recommended: true,
        files: &[
            FileSpec {
                name: "encoder-model.int8.onnx",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/encoder-model.int8.onnx",
                sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
                size: 652_183_999,
            },
            FileSpec {
                name: "decoder_joint-model.int8.onnx",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/decoder_joint-model.int8.onnx",
                sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
                size: 18_202_004,
            },
            NEMO_PRE,
            FileSpec {
                name: "vocab.txt",
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/vocab.txt",
                sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
                size: 93_939,
            },
        ],
    },
    CatalogModel {
        id: "whisper-tiny.en",
        name: "Whisper Tiny (English)",
        engine: Engine::Whisper,
        description: "Tiny English model, virtually instant even on older machines.",
        languages: "en",
        accuracy: 0.40,
        speed: 1.0,
        recommended: false,
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q8_0.bin",
            sha256: "5bc2b3860aa151a4c6e7bb095e1fcce7cf12c7b020ca08dcec0c6d018bb7dd94",
            size: 43_550_795,
        }],
    },
    CatalogModel {
        id: "whisper-base",
        name: "Whisper Base (multilingual)",
        engine: Engine::Whisper,
        description: "Small and quick, with automatic language detection.",
        languages: "99 languages",
        accuracy: 0.48,
        speed: 0.92,
        recommended: false,
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
            sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
            size: 59_707_625,
        }],
    },
    CatalogModel {
        id: "canary-180m-flash",
        name: "NVIDIA Canary 180M Flash",
        engine: Engine::Canary,
        description: "Tiny encoder-decoder with English, German, Spanish and French, plus translation between them. ONNX int8.",
        languages: "en, de, es, fr",
        accuracy: 0.72,
        speed: 0.85,
        recommended: false,
        files: &[
            FileSpec {
                name: "encoder-model.int8.onnx",
                url: "https://huggingface.co/istupakov/canary-180m-flash-onnx/resolve/main/encoder-model.int8.onnx",
                sha256: "996d1c89e6cbc891a7c88bf410884c178ffa474f7b13084522ac74a5e144cc81",
                size: 133_710_896,
            },
            FileSpec {
                name: "decoder-model.int8.onnx",
                url: "https://huggingface.co/istupakov/canary-180m-flash-onnx/resolve/main/decoder-model.int8.onnx",
                sha256: "9dd9c447872088c912e916d73751f9621a54085d5bc46788454fe904db51a914",
                size: 79_520_211,
            },
            NEMO_PRE,
            FileSpec {
                name: "vocab.txt",
                url: "https://huggingface.co/istupakov/canary-180m-flash-onnx/resolve/main/vocab.txt",
                sha256: "2dae6fc7815f9640645e0c765522b278ee0cef49b482d91f6913e334628d3e77",
                size: 53_555,
            },
        ],
    },
    CatalogModel {
        id: "whisper-small.en",
        name: "Whisper Small (English)",
        engine: Engine::Whisper,
        description: "More accurate English Whisper that is still quick on Apple Silicon.",
        languages: "en",
        accuracy: 0.68,
        speed: 0.75,
        recommended: false,
        files: &[FileSpec {
            name: "model.bin",
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-q5_1.bin",
            sha256: "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
            size: 190_098_681,
        }],
    },
    CatalogModel {
        id: "whisper-small",
        name: "Whisper Small (multilingual)",
        engine: Engine::Whisper,
        description: "Multilingual Whisper with automatic language detection.",
        languages: "99 languages",
        accuracy: 0.65,
        speed: 0.75,
        recommended: false,
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
        accuracy: 0.85,
        speed: 0.40,
        recommended: false,
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
    pub accuracy: f32,
    pub speed: f32,
    pub recommended: bool,
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
                    accuracy: model.accuracy,
                    speed: model.speed,
                    recommended: model.recommended,
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
