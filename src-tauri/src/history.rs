//! Transcript history stored as JSON lines, capped by `history_limit`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::settings::data_dir;

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    pub id: u64,
    pub timestamp_ms: u64,
    pub text: String,
    pub post_processed: Option<String>,
    pub model: String,
    pub duration_ms: u64,
}

fn path() -> std::path::PathBuf {
    data_dir().join("history.jsonl")
}

/// Serialises the read-modify-write cycles below.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn append(entry: &Entry) -> Result<()> {
    use std::io::Write;
    let _guard = lock();
    let mut entry = entry.clone();
    // Ids key the delete/copy lookups, so keep them unique even when two
    // dictations finish within the same millisecond.
    if let Some(last) = read_all().last() {
        if entry.id <= last.id {
            entry.id = last.id + 1;
        }
    }
    std::fs::create_dir_all(data_dir())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path())?;
    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Newest first, limited to `limit` entries; zero keeps none.
pub fn list(limit: usize) -> Vec<Entry> {
    let mut entries = read_all();
    entries.reverse();
    entries.truncate(limit);
    entries
}

pub fn delete(id: u64) -> Result<()> {
    let _guard = lock();
    let mut remaining: Vec<Entry> = read_all().into_iter().filter(|e| e.id != id).collect();
    remaining.reverse();
    write_all(&remaining)
}

/// Keeps the newest `limit` entries; zero empties the log.
pub fn trim(limit: usize) -> Result<()> {
    let _guard = lock();
    let mut entries = read_all();
    entries.reverse();
    entries.truncate(limit);
    write_all(&entries)
}

/// File order, oldest first.
fn read_all() -> Vec<Entry> {
    let Ok(raw) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Replaces the file through a temporary so a crash cannot truncate the log.
fn write_all(entries_newest_first: &[Entry]) -> Result<()> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let mut out = String::new();
    for entry in entries_newest_first.iter().rev() {
        out.push_str(&serde_json::to_string(entry)?);
        out.push('\n');
    }
    let temp = dir.join("history.jsonl.tmp");
    std::fs::write(&temp, out)?;
    std::fs::rename(temp, path())?;
    Ok(())
}
