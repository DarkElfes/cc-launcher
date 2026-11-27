//! Applies app-level choices to a profile's Claude Code config, so they don't have to be set with
//! /config and /voice in every account:
//!
//! - settings.json `language`: Claude's reply language and the voice-dictation language.
//! - settings.json `voice`: `{ enabled, mode }`.
//! - keybindings.json: binds Alt+K (`meta+k`) to `voice:pushToTalk` in the Chat context, so the
//!   app's mic button / Ctrl+Space can trigger dictation by sending that key.
//!
//! Files that aren't valid JSON are left alone.

use super::types::{Settings, VoiceMode};
use super::usage::{read_settings, write_json};
use serde_json::{json, Value};
use std::path::Path;

pub const DICTATE_KEY: &str = "meta+k";
/// What a terminal sends for Alt+K: ESC followed by "k".
pub const DICTATE_SEQUENCE: &[u8] = b"\x1bk";

pub fn apply_voice_settings(dir: &Path, s: &Settings) {
    if !dir.exists() {
        return;
    }
    let path = dir.join("settings.json");
    if let Some(mut settings) = read_settings(&path) {
        let before = Value::Object(settings.clone());
        if let Some(lang) = &s.claude_language {
            settings.insert("language".into(), json!(lang));
        }
        if let Some(mode) = s.voice_mode {
            let mut voice = match settings.get("voice") {
                Some(Value::Object(m)) => m.clone(),
                _ => Default::default(),
            };
            match mode {
                VoiceMode::Off => {
                    voice.insert("enabled".into(), json!(false));
                }
                VoiceMode::Tap | VoiceMode::Hold => {
                    voice.insert("enabled".into(), json!(true));
                    voice.insert("mode".into(), json!(if mode == VoiceMode::Tap { "tap" } else { "hold" }));
                }
            }
            settings.insert("voice".into(), Value::Object(voice));
        }
        let after = Value::Object(settings);
        if after != before {
            write_json(&path, &after);
        }
    }
    if matches!(s.voice_mode, Some(VoiceMode::Tap) | Some(VoiceMode::Hold)) {
        ensure_dictate_key(dir);
    }
}

/// "Default" chosen in Settings: removes `language` so Claude Code uses its default (replies in the
/// language you write in, dictation in English).
pub fn clear_language(dir: &Path) {
    let path = dir.join("settings.json");
    if let Some(mut settings) = read_settings(&path) {
        if settings.remove("language").is_some() {
            write_json(&path, &Value::Object(settings));
        }
    }
}

pub fn ensure_dictate_key(dir: &Path) {
    let path = dir.join("keybindings.json");
    let Some(mut file) = read_settings(&path) else { return };
    let mut bindings: Vec<Value> = file.get("bindings").and_then(|b| b.as_array()).cloned().unwrap_or_default();
    let already = bindings.iter().any(|b| {
        b.get("context").and_then(|c| c.as_str()) == Some("Chat")
            && b.get("bindings").and_then(|x| x.get(DICTATE_KEY)).and_then(|v| v.as_str()) == Some("voice:pushToTalk")
    });
    if already {
        return;
    }
    if let Some(chat) = bindings.iter_mut().find(|b| b.get("context").and_then(|c| c.as_str()) == Some("Chat")) {
        let mut inner = chat.get("bindings").and_then(|b| b.as_object()).cloned().unwrap_or_default();
        if inner.contains_key(DICTATE_KEY) {
            return; // user bound Alt+K to something else; respect it
        }
        inner.insert(DICTATE_KEY.into(), json!("voice:pushToTalk"));
        chat["bindings"] = Value::Object(inner);
    } else {
        bindings.push(json!({ "context": "Chat", "bindings": { DICTATE_KEY: "voice:pushToTalk" } }));
    }
    file.insert("bindings".into(), Value::Array(bindings));
    write_json(&path, &Value::Object(file));
}

/// Permission modes the chat tabs pass to `--permission-mode`.
pub const PERMISSION_MODES: &[&str] = &["default", "acceptEdits", "plan", "auto"];

/// The mode a chat tab starts in. `picked` is the app's choice (None = Default): then the
/// profile's own `permissions.defaultMode` (user settings.json), else `auto`, which is the
/// interactive CLI's built-in default. (`claude -p` alone would start in Manual.)
pub fn effective_permission_mode(profile: &str, picked: Option<&str>) -> String {
    if let Some(m) = picked.filter(|m| PERMISSION_MODES.contains(m)) {
        return m.into();
    }
    let file = super::profiles::profile_dir(profile).join("settings.json");
    super::types::read_json_file(&file)
        .and_then(|v| v.pointer("/permissions/defaultMode").and_then(|m| m.as_str()).map(String::from))
        .map(|m| if m == "manual" { "default".into() } else { m })
        .filter(|m| PERMISSION_MODES.contains(&m.as_str()))
        .unwrap_or_else(|| "auto".into())
}
