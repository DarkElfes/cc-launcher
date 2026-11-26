//! Wires a profile's settings.json `hooks` to the app's local hook server
//! (https://code.claude.com/docs/en/hooks, `type: "http"`). Every entry the app adds has a url
//! ending in /ccl-hook, so it can be replaced on the next start (the port changes per run) without
//! touching the user's own hooks.
//!
//! The tab id and the per-run secret come from the claude process env (CCL_TAB_ID, CCL_HOOK_TOKEN)
//! via `allowedEnvVars`. Nothing secret is written to the file.

use super::hook_server::HOOK_PATH;
use super::usage::{read_settings, write_json};
use serde_json::{json, Value};
use std::path::Path;

const EVENTS: &[(&str, Option<&str>, Option<u32>)] = &[
    ("SessionStart", None, None),
    ("UserPromptSubmit", None, None),
    ("PreToolUse", Some("ExitPlanMode"), None),
    // Chat tabs answer permission prompts in the app; terminal tabs get "no decision" right away.
    ("PermissionRequest", None, Some(600)),
    ("PostToolUse", None, None),
    ("Notification", None, None),
    ("Stop", None, None),
    ("StopFailure", None, None),
    ("SubagentStart", None, None),
    ("SubagentStop", None, None),
    ("TaskCreated", None, None),
    ("TaskCompleted", None, None),
    ("SessionEnd", None, None),
];

fn is_ours(h: &Value) -> bool {
    h.get("url").and_then(|u| u.as_str()).map(|u| u.ends_with(HOOK_PATH)).unwrap_or(false)
}

pub fn ensure_hooks(dir: &Path, port: u16) {
    if !dir.exists() || port == 0 {
        return;
    }
    let path = dir.join("settings.json");
    let Some(mut settings) = read_settings(&path) else { return };
    let before = Value::Object(settings.clone());
    let hook = json!({
        "type": "http",
        "url": format!("http://127.0.0.1:{port}{HOOK_PATH}"),
        "timeout": 3,
        "headers": { "X-CCL-Tab": "$CCL_TAB_ID", "Authorization": "Bearer $CCL_HOOK_TOKEN" },
        "allowedEnvVars": ["CCL_TAB_ID", "CCL_HOOK_TOKEN"]
    });
    let mut hooks = match settings.get("hooks") {
        Some(Value::Object(m)) => m.clone(),
        _ => Default::default(),
    };
    for (event, matcher, timeout) in EVENTS {
        // Drop our previous entries for this event, keep everything else.
        let mut groups: Vec<Value> = hooks
            .get(*event)
            .and_then(|g| g.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|mut g| {
                let inner: Vec<Value> = g
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|h| !is_ours(h))
                    .collect();
                if inner.is_empty() {
                    return None;
                }
                g["hooks"] = Value::Array(inner);
                Some(g)
            })
            .collect();
        let mut h = hook.clone();
        if let Some(t) = timeout {
            h["timeout"] = json!(t);
        }
        groups.push(match matcher {
            Some(m) => json!({ "matcher": m, "hooks": [h] }),
            None => json!({ "hooks": [h] }),
        });
        hooks.insert((*event).into(), Value::Array(groups));
    }
    settings.insert("hooks".into(), Value::Object(hooks));
    let after = Value::Object(settings);
    if after != before {
        write_json(&path, &after);
    }
}
