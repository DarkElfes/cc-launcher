//! Plan usage (5-hour and weekly limits) per profile.
//!
//! The only documented source is Claude Code's status line: its stdin JSON carries
//! `rate_limits.five_hour` / `rate_limits.seven_day` with `used_percentage` and `resets_at`
//! (https://code.claude.com/docs/en/statusline.md).
//!
//! For each profile the app installs a small PowerShell status-line script into the profile folder
//! and points that profile's settings.json `statusLine` at it. The script saves `rate_limits` to
//! cc-launcher-usage.json and per-session info to cc-launcher-sessions/<id>.json. No credentials
//! are touched.

use super::types::{now_ms, read_json_file, write_atomic, ProfileUsage, UsageSource, UsageWindow};
use serde_json::{json, Value};
use std::path::Path;

const SCRIPT_NAME: &str = "cc-launcher-statusline.ps1";
const USAGE_FILE: &str = "cc-launcher-usage.json";
/// Per-session status files (model, context, cost) read by sessions.rs.
pub const SESSIONS_DIR: &str = "cc-launcher-sessions";

const SCRIPT: &str = r#"# Written by cc-launcher. Saves plan usage (rate_limits) and per-session info (model, context,
# cost) for the app, and prints a short status line.
$ErrorActionPreference = 'SilentlyContinue'
# Claude Code sends UTF-8; the console default would garble non-ASCII text (e.g. Cyrillic titles).
[Console]::InputEncoding = [Text.Encoding]::UTF8
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$j = [Console]::In.ReadToEnd() | ConvertFrom-Json
$now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
if ($j.rate_limits) {
  $out = Join-Path $PSScriptRoot 'cc-launcher-usage.json'
  $data = @{ updatedAt = $now; rate_limits = $j.rate_limits }
  [IO.File]::WriteAllText("$out.tmp", ($data | ConvertTo-Json -Depth 6 -Compress))
  Move-Item -LiteralPath "$out.tmp" -Destination $out -Force
}
if ($j.session_id -match '^[A-Za-z0-9-]+$') {
  $dir = Join-Path $PSScriptRoot 'cc-launcher-sessions'
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
  $out = Join-Path $dir ($j.session_id + '.json')
  $data = @{ updatedAt = $now; tab = $env:CCL_TAB_ID; session_id = $j.session_id; session_name = $j.session_name; model = $j.model; cost = $j.cost; context_window = $j.context_window }
  [IO.File]::WriteAllText("$out.tmp", ($data | ConvertTo-Json -Depth 6 -Compress))
  Move-Item -LiteralPath "$out.tmp" -Destination $out -Force
}
$parts = @()
if ($j.model.display_name) { $parts += $j.model.display_name }
if ($null -ne $j.context_window.used_percentage) { $parts += ('ctx {0:N0}%' -f [double]$j.context_window.used_percentage) }
if ($j.rate_limits.five_hour) { $parts += ('5h {0:N0}%' -f [double]$j.rate_limits.five_hour.used_percentage) }
if ($j.rate_limits.seven_day) { $parts += ('week {0:N0}%' -f [double]$j.rate_limits.seven_day.used_percentage) }
[Console]::Out.Write(($parts -join ' | '))
"#;

fn status_line_command(dir: &Path) -> String {
    // Forward slashes and quotes: Claude Code may run this through Git Bash or PowerShell.
    let script = dir.join(SCRIPT_NAME).to_string_lossy().replace('\\', "/");
    format!("powershell -NoProfile -ExecutionPolicy Bypass -File \"{script}\"")
}

/// Reads settings.json: Some(map) when absent or valid, None when it can't be parsed (never clobber it).
pub fn read_settings(path: &Path) -> Option<serde_json::Map<String, Value>> {
    if !path.exists() {
        return Some(Default::default());
    }
    match read_json_file(path) {
        Some(Value::Object(m)) => Some(m),
        _ => None,
    }
}

pub fn write_json(path: &Path, v: &Value) {
    if let Ok(text) = serde_json::to_string_pretty(v) {
        let _ = write_atomic(path, &text);
    }
}

/// Installs/refreshes the script and wires it into <profile>/settings.json.
/// Leaves settings.json alone if it isn't valid JSON or already has someone else's statusLine.
pub fn ensure_status_line(dir: &Path) {
    if !dir.exists() {
        return;
    }
    let script_path = dir.join(SCRIPT_NAME);
    if std::fs::read_to_string(&script_path).ok().as_deref() != Some(SCRIPT) {
        let _ = std::fs::write(&script_path, SCRIPT);
    }
    let settings_path = dir.join("settings.json");
    let Some(mut settings) = read_settings(&settings_path) else { return };
    let current_cmd = settings
        .get("statusLine")
        .map(|s| s.get("command").and_then(|c| c.as_str()).unwrap_or("").to_string());
    if let Some(cmd) = &current_cmd {
        if !cmd.contains(SCRIPT_NAME) {
            return; // user's own status line
        }
    }
    let command = status_line_command(dir);
    if current_cmd.as_deref() == Some(command.as_str()) {
        return;
    }
    settings.insert("statusLine".into(), json!({ "type": "command", "command": command }));
    write_json(&settings_path, &Value::Object(settings));
}

fn to_window(raw: &Value, now_sec: f64) -> Option<UsageWindow> {
    let used = raw.get("used_percentage")?.as_f64()?;
    let resets = raw.get("resets_at")?.as_f64()?;
    // After the reset time the saved number is stale: the window has started over.
    if resets <= now_sec {
        return Some(UsageWindow { used_percent: 0.0, resets_at: None, resets_label: None, stale: true });
    }
    Some(UsageWindow {
        used_percent: used.clamp(0.0, 100.0),
        resets_at: Some((resets * 1000.0) as i64),
        resets_label: None,
        stale: false,
    })
}

pub fn read_usage(dir: &Path) -> Option<ProfileUsage> {
    let data = read_json_file(&dir.join(USAGE_FILE))?;
    let now = now_ms() as f64 / 1000.0;
    let rl = data.get("rate_limits").cloned().unwrap_or(Value::Null);
    Some(ProfileUsage {
        updated_at: (data.get("updatedAt").and_then(|v| v.as_f64()).unwrap_or(0.0) * 1000.0) as i64,
        five_hour: rl.get("five_hour").and_then(|w| to_window(w, now)),
        week: rl.get("seven_day").and_then(|w| to_window(w, now)),
        source: UsageSource::StatusLine,
    })
}
