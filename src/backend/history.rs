//! Past Claude Code sessions of a project, per profile. Transcripts live in
//! <profile>/projects/<project>/<session-id>.jsonl, where <project> is the working directory with
//! every non-alphanumeric character replaced by "-".
//!
//! Only file names, times and the first part of each file are read, to find a title. The entry
//! format is internal to Claude Code, so title detection is best-effort.

use super::types::{js, HistorySession};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

pub fn project_dir_name(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn read_head(path: &Path, bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    String::from_utf8_lossy(&buf[..n]).to_string()
}

pub fn text_of(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => parts
            .iter()
            .find(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
            .and_then(|p| p.get("text").and_then(|t| t.as_str()).map(String::from)),
        _ => None,
    }
}

fn title_of(path: &Path) -> Option<String> {
    let mut first_prompt: Option<String> = None;
    for line in read_head(path, 96 * 1024).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(t) = js::str(&e, "customTitle").or_else(|| js::str(&e, "title")) {
            return Some(t.trim().to_string());
        }
        if js::str(&e, "type") == Some("summary") {
            if let Some(s) = js::str(&e, "summary") {
                return Some(s.to_string());
            }
        }
        if first_prompt.is_none() && js::str(&e, "type") == Some("user") && e.get("isMeta").and_then(|v| v.as_bool()) != Some(true) {
            if let Some(t) = text_of(js::obj(js::obj(&e, "message"), "content")) {
                let t = t.trim().to_string();
                if !t.is_empty() && !t.starts_with('<') {
                    first_prompt = Some(t);
                }
            }
        }
    }
    first_prompt
}

pub fn list_history(cwd: &str, profiles: &[(String, std::path::PathBuf)], limit: usize) -> Vec<HistorySession> {
    let name = project_dir_name(cwd);
    if name.len() > 200 {
        return vec![]; // Claude hashes long names; not worth guessing
    }
    let ws = regex::Regex::new(r"\s+").unwrap();
    let mut out = vec![];
    for (pname, dir) in profiles {
        let folder = dir.join("projects").join(&name);
        let Ok(rd) = std::fs::read_dir(&folder) else { continue };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            let Some(id) = fname.strip_suffix(".jsonl") else { continue };
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() || meta.len() == 0 {
                continue;
            }
            let modified = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let title = title_of(&e.path()).map(|t| ws.replace_all(&t, " ").chars().take(120).collect()).unwrap_or_default();
            out.push(HistorySession { id: id.to_string(), profile: pname.clone(), title, modified_at: modified, size_kb: meta.len() / 1024 });
        }
    }
    out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    out.truncate(limit);
    out
}
