//! Persistent settings store.
//!
//! Settings are kept as a JSON object so `set_setting(key, value)` can update
//! one field without a command per option, mirroring how Handy exposes its
//! `change_*_setting` commands. Typed accessors read values with defaults.

use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var(var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(fallback))
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("bolum")
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("bolum")
}

pub fn runtime_dir() -> PathBuf {
    // macOS has no XDG_RUNTIME_DIR; TMPDIR is its private per-user equivalent.
    std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("TMPDIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Default settings, aligned with the options Handy exposes.
pub fn defaults() -> &'static Value {
    static DEFAULTS: OnceLock<Value> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        // Pasting is instant everywhere on macOS, where synthetic typing is
        // slow in some editors; Linux typing tools handle Unicode directly.
        let injection = if cfg!(target_os = "macos") { "clipboard" } else { "type" };
        json!({
            "hotkey": "ctrl+alt+space",
            "push_to_talk": false,
            "microphone": null,
            "model": "whisper-base.en",
            "language": "auto",
            "translate_to": "",
            "injection": injection,
            "typing_tool": "auto",
            "append_trailing_space": true,
            "auto_submit": false,
            "show_overlay": true,
            "start_hidden": true,
            "history_limit": 200,
            "post_process": {
                "enabled": false,
                "base_url": "http://localhost:11434/v1",
                "model": "llama3.2",
                "api_key": "",
                "prompt": "Fix punctuation and casing in this dictation. Reply with only the cleaned text."
            }
        })
    })
}

/// Reads a possibly dotted key from [`defaults`].
fn default_for(key: &str) -> Value {
    defaults()
        .pointer(&format!("/{}", key.replace('.', "/")))
        .cloned()
        .unwrap_or(Value::Null)
}

#[derive(Clone)]
pub struct SettingsStore {
    value: Map<String, Value>,
}

impl SettingsStore {
    pub fn load() -> Self {
        let path = settings_path();
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|value| value.as_object().cloned());
        match stored {
            Some(value) => Self { value },
            None => {
                let store = Self {
                    value: defaults().as_object().cloned().unwrap_or_default(),
                };
                // Materialise the file so first-run detection and hand edits
                // work from the defaults rather than an absent file.
                store.save();
                store
            }
        }
    }

    pub fn json(&self) -> Value {
        Value::Object(self.value.clone())
    }

    /// Sets `key`; a dotted key such as `post_process.enabled` updates the
    /// nested object.
    pub fn set(&mut self, key: &str, value: Value) {
        self.set_quiet(key, value);
        self.save();
    }

    /// Like [`set`](Self::set) without persisting to disk.
    pub fn set_quiet(&mut self, key: &str, value: Value) {
        let path: Vec<&str> = key.split('.').collect();
        set_in(&mut self.value, &path, value);
    }

    fn save(&self) {
        if let Err(err) = self.try_save() {
            log::warn!("could not save settings: {err}");
        }
    }

    fn try_save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir())?;
        std::fs::write(
            settings_path(),
            serde_json::to_string_pretty(&self.json()).unwrap_or_default(),
        )
    }

    /// Reads a possibly dotted key from the stored object.
    fn stored(&self, key: &str) -> Option<&Value> {
        let mut parts = key.split('.');
        let mut current = self.value.get(parts.next()?)?;
        for part in parts {
            current = current.get(part)?;
        }
        Some(current)
    }

    pub fn str(&self, key: &str) -> String {
        self.stored(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| default_for(key).as_str().map(str::to_string))
            .unwrap_or_default()
    }

    pub fn bool(&self, key: &str) -> bool {
        self.stored(key)
            .and_then(Value::as_bool)
            .or_else(|| default_for(key).as_bool())
            .unwrap_or(false)
    }

    pub fn usize(&self, key: &str) -> usize {
        self.stored(key)
            .and_then(Value::as_u64)
            .or_else(|| default_for(key).as_u64())
            .unwrap_or(0) as usize
    }

    pub fn opt_str(&self, key: &str) -> Option<String> {
        self.stored(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    /// Returns `key`'s stored object with defaults for missing fields.
    pub fn sub(&self, key: &str) -> Map<String, Value> {
        let mut merged = default_for(key).as_object().cloned().unwrap_or_default();
        if let Some(stored) = self.value.get(key).and_then(Value::as_object) {
            merged.extend(stored.clone());
        }
        merged
    }
}

/// Writes `value` at the dotted `path`, creating or replacing intermediate
/// objects so a bad hand-edited file cannot swallow the write.
fn set_in(map: &mut Map<String, Value>, path: &[&str], value: Value) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        map.insert(head.to_string(), value);
        return;
    }
    let entry = map
        .entry(head.to_string())
        .or_insert_with(|| Value::Object(Map::default()));
    if !entry.is_object() {
        *entry = Value::Object(Map::default());
    }
    set_in(
        entry.as_object_mut().expect("just ensured an object"),
        rest,
        value,
    );
}

pub struct Settings(pub Mutex<SettingsStore>);

impl Settings {
    pub fn new() -> Self {
        Self(Mutex::new(SettingsStore::load()))
    }
}
