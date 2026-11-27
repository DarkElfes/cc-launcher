#![allow(dead_code)] // some fields are kept for completeness of the data model
//! Data types shared by the backend and the UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<i64>,
    /// Profile used the last time Claude Code was opened for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_profile: Option<String>,
}

impl Project {
    pub fn exists(&self) -> bool {
        std::path::Path::new(&self.path).is_dir()
    }
}

/// A Claude tab to reopen after the app restarts (its conversation continues via --resume).
/// Same shape as the Electron 0.9.1 `openTabs` entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTab {
    pub project_id: String,
    pub profile: String,
    pub mode: TabMode,
    pub session_id: String,
    #[serde(default)]
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceMode {
    Tap,
    Hold,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TabMode {
    #[default]
    Chat,
    Terminal,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Full path to claude.exe. Empty means auto-detect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_path: Option<String>,
    /// Command used for "Open in editor". Defaults to "zed".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_command: Option<String>,
    /// Claude's reply + voice dictation language for all profiles. None = leave as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_language: Option<String>,
    /// Voice dictation for all profiles. None = leave as is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_mode: Option<VoiceMode>,
    /// How new Claude tabs open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_mode: Option<TabMode>,
    /// Account buttons continue the project's latest session (None = yes) or start a new one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continue_last: Option<bool>,
    /// Widths of the sidebar and the session panel (px), when changed by dragging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_width: Option<f32>,
    /// Model for new Claude tabs (alias like "opus" / "sonnet"); None = account default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Permission mode picked for chat tabs (`--permission-mode` value); None = auto, like the
    /// interactive CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Session side panel visible (was localStorage in the Electron build).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_open: Option<bool>,
    /// Window position and size when the app last closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPlacement>,
}

/// Window position/size in logical pixels; `maximized` keeps the size to restore to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AccountInfo {
    pub logged_in: bool,
    pub email: Option<String>,
    /// e.g. "pro", "max"
    pub plan: Option<String>,
    pub org_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UsageWindow {
    /// 0–100. 0 with stale=true means the window has reset since the last report.
    pub used_percent: f64,
    /// Epoch ms when the window resets, or None when unknown / already reset.
    pub resets_at: Option<i64>,
    /// Reset time as Claude Code printed it (e.g. "Nov 10, 7pm").
    pub resets_label: Option<String>,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageSource {
    UsageCommand,
    StatusLine,
}

#[derive(Clone, Debug)]
pub struct ProfileUsage {
    pub updated_at: i64,
    pub five_hour: Option<UsageWindow>,
    pub week: Option<UsageWindow>,
    pub source: UsageSource,
}

#[derive(Clone, Debug)]
pub struct ProfileInfo {
    pub name: String,
    /// Config folder passed as CLAUDE_CONFIG_DIR: ~/.claude-profiles/<name>.
    pub dir: String,
    pub signed_in: bool,
    pub account: Option<AccountInfo>,
    pub usage: Option<ProfileUsage>,
    pub error: Option<String>,
}

/// What Claude is doing in a tab, from its hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Starting,
    Idle,
    Working,
    NeedsYou,
    Done,
    Error,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Clone, Debug)]
pub struct SessionTask {
    pub id: String,
    pub subject: String,
    pub status: TaskStatus,
}

#[derive(Clone, Debug)]
pub struct SessionAgent {
    pub id: String,
    /// Subagent type, or "bash" for a background command.
    pub kind: String,
    pub label: Option<String>,
    pub running: bool,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputOp {
    Written,
    Edited,
}

#[derive(Clone, Debug)]
pub struct SessionOutput {
    pub path: String,
    pub op: OutputOp,
    pub count: u32,
    pub at: i64,
}

#[derive(Clone, Debug)]
pub struct ContextUse {
    pub used_percent: Option<f64>,
    pub tokens: u64,
    pub size: u64,
}

/// Live info about the Claude Code session in one tab.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub tab_id: String,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    /// The conversation file (for continuing it in another account).
    pub transcript_path: Option<String>,
    pub model: Option<String>,
    pub activity: Activity,
    pub activity_at: i64,
    pub message: Option<String>,
    pub error: Option<String>,
    pub context: Option<ContextUse>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<f64>,
    pub lines_added: Option<i64>,
    pub lines_removed: Option<i64>,
    pub plan: Option<String>,
    pub tasks: Vec<SessionTask>,
    pub agents: Vec<SessionAgent>,
    pub outputs: Vec<SessionOutput>,
}

impl SessionInfo {
    pub fn new(tab_id: &str) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            session_id: None,
            session_name: None,
            transcript_path: None,
            model: None,
            activity: Activity::Starting,
            activity_at: now_ms(),
            message: None,
            error: None,
            context: None,
            cost_usd: None,
            duration_ms: None,
            lines_added: None,
            lines_removed: None,
            plan: None,
            tasks: vec![],
            agents: vec![],
            outputs: vec![],
        }
    }
}

/// A tool call waiting for approval in a chat tab (from the PermissionRequest hook).
#[derive(Clone, Debug)]
pub struct ChatPermissionRequest {
    pub tab_id: String,
    pub request_id: String,
    pub tool_name: String,
    pub input: Value,
    pub suggestions: Vec<Value>,
}

#[derive(Clone, Debug)]
pub enum ChatPermissionDecision {
    Allow { updated_input: Option<Value>, updated_permissions: Option<Vec<Value>> },
    Deny { message: Option<String>, interrupt: bool },
}

#[derive(Clone, Debug)]
pub struct HistorySession {
    pub id: String,
    pub profile: String,
    pub title: String,
    pub modified_at: i64,
    pub size_kb: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpStatus {
    Connected,
    NeedsAuth,
    Failed,
    Pending,
    Disabled,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct McpServer {
    pub name: String,
    pub detail: String,
    pub status: McpStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflict,
}

#[derive(Clone, Debug)]
pub struct GitChange {
    pub path: String,
    pub status: GitStatus,
    pub staged: bool,
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct GitInfo {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<GitChange>,
    pub added: u64,
    pub removed: u64,
    /// (hash, subject, epoch ms)
    pub last_commit: Option<(String, String, i64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateState {
    Idle,
    Disabled,
    Checking,
    None,
    Downloading { version: String, percent: u8 },
    Ready { version: String },
    Error(String),
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Small helpers for reading loosely-typed JSON (hook payloads, stream-json events).
pub mod js {
    use serde_json::Value;

    pub fn str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
        v.get(key).and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty())
    }
    pub fn num(v: &Value, key: &str) -> Option<f64> {
        v.get(key).and_then(|x| x.as_f64())
    }
    pub fn obj<'a>(v: &'a Value, key: &str) -> &'a Value {
        static NULL: Value = Value::Null;
        v.get(key).unwrap_or(&NULL)
    }
    pub fn short(s: &str, n: usize) -> String {
        if s.chars().count() > n {
            let mut out: String = s.chars().take(n - 1).collect();
            out.push('…');
            out
        } else {
            s.to_string()
        }
    }
}

/// "claude-opus-5-5" → "Opus 5.5"
pub fn pretty_model(id: &str) -> String {
    let re = regex::Regex::new(r"claude-([a-z]+)-(\d+)-(\d+)").unwrap();
    match re.captures(id) {
        Some(c) => {
            let fam = &c[1];
            let mut chars = fam.chars();
            let first = chars.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            format!("{}{} {}.{}", first, chars.as_str(), &c[2], &c[3])
        }
        None => id.to_string(),
    }
}

/// Reads a JSON file, tolerating a UTF-8 BOM.
pub fn read_json_file(path: &std::path::Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

/// Writes a file through a temp file and rename, so readers never see half a file.
pub fn write_atomic(path: &std::path::Path, data: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod settings_tests {
    use super::Settings;

    #[test]
    fn new_settings_round_trip_in_app_json() {
        let s: Settings = serde_json::from_str(r#"{"permissionMode":"plan","continueLast":false}"#).unwrap();
        assert_eq!(s.permission_mode.as_deref(), Some("plan"));
        assert_eq!(s.continue_last, Some(false));
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["permissionMode"], "plan");
        assert_eq!(v["continueLast"], false);
        // Older app.json files without the keys still load.
        let old: Settings = serde_json::from_str(r#"{"tabMode":"chat"}"#).unwrap();
        assert!(old.permission_mode.is_none() && old.continue_last.is_none());
    }
}
