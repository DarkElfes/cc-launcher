//! Claude's thinking text, kept by the app for chat history.
//!
//! Claude Code's session transcripts store thinking blocks without their text (only a signature),
//! so a reopened chat would lose them. The app saves the thinking it streamed live, keyed by the API
//! message id + the block's position in that message, in %APPDATA%\cc-launcher\thinking.jsonl.

use super::store::data_dir;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 20 * 1024 * 1024;

fn file() -> PathBuf {
    data_dir().join("thinking.jsonl")
}

pub fn key(msg_id: &str, nth: usize) -> String {
    format!("{msg_id}#{nth}")
}

/// Appends entries on a background thread; drops the older half once the file gets big.
pub fn save(entries: Vec<(String, String)>) {
    if entries.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        let path = file();
        let _ = std::fs::create_dir_all(data_dir());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            for (k, t) in &entries {
                let _ = writeln!(f, "{}", json!({ "id": k, "text": t }));
            }
        }
        if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let lines: Vec<&str> = text.lines().collect();
                let keep = lines[lines.len() / 2..].join("\n") + "\n";
                let _ = super::types::write_atomic(&path, &keep);
            }
        }
    });
}

/// Saved thinking for the given keys (unknown keys are simply missing).
pub fn load(keys: &HashSet<String>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if keys.is_empty() {
        return out;
    }
    let Ok(text) = std::fs::read_to_string(file()) else { return out };
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let (Some(k), Some(t)) = (v.get("id").and_then(|x| x.as_str()), v.get("text").and_then(|x| x.as_str())) {
            if keys.contains(k) {
                out.insert(k.to_string(), t.to_string());
            }
        }
    }
    out
}

/// Keys of the thinking blocks in transcript events (as read by `chat::read_transcript_messages`).
pub fn keys_in(events: &[Value]) -> HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut keys = HashSet::new();
    for e in events {
        let msg = e.get("message");
        let Some(id) = msg.and_then(|m| m.get("id")).and_then(|v| v.as_str()) else { continue };
        let Some(content) = msg.and_then(|m| m.get("content")).and_then(|c| c.as_array()) else { continue };
        for b in content {
            if b.get("type").and_then(|t| t.as_str()) == Some("thinking") {
                let n = counts.entry(id.to_string()).or_default();
                keys.insert(key(id, *n));
                *n += 1;
            }
        }
    }
    keys
}
