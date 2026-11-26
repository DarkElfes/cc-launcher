//! Live state per tab, built from:
//! - Claude Code hooks (hook_server.rs): activity, plan, tasks, subagents, files written.
//! - The status line's per-session file (usage.rs writes <profile>/cc-launcher-sessions/<id>.json):
//!   model, context window, cost estimate, lines added/removed, duration.
//! - For chat tabs: the stream-json events themselves.

use super::types::{
    js, now_ms, pretty_model, read_json_file, Activity, ContextUse, OutputOp, SessionAgent, SessionInfo, SessionOutput,
    SessionTask, TaskStatus,
};
use super::usage::SESSIONS_DIR;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

const WAITING_NOTIFICATIONS: &[&str] = &["permission_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"];

#[derive(Default)]
pub struct SessionTracker {
    pub tabs: HashMap<String, SessionInfo>,
    profile_of: HashMap<String, PathBuf>,
    chat_cost_base: HashMap<String, f64>,
    chat_cost_run: HashMap<String, f64>,
}

fn set_activity(s: &mut SessionInfo, a: Activity) {
    if s.activity != a {
        s.activity = a;
        s.activity_at = now_ms();
    }
}

impl SessionTracker {
    pub fn open(&mut self, tab: &str, profile_dir: PathBuf) {
        self.profile_of.insert(tab.into(), profile_dir);
        self.tabs.insert(tab.into(), SessionInfo::new(tab));
    }

    /// The claude process of a tab exited (the tab may stay open to show its last screen).
    pub fn exited(&mut self, tab: &str) {
        if let Some(s) = self.tabs.get_mut(tab) {
            set_activity(s, Activity::Ended);
            for a in &mut s.agents {
                a.running = false;
            }
        }
    }

    pub fn close(&mut self, tab: &str) {
        self.tabs.remove(tab);
        self.profile_of.remove(tab);
        self.chat_cost_base.remove(tab);
        self.chat_cost_run.remove(tab);
    }

    pub fn get(&self, tab: &str) -> Option<&SessionInfo> {
        self.tabs.get(tab)
    }

    /// Applies a hook event. Returns the activity before and after (for notifications).
    pub fn handle(&mut self, tab: &str, e: &Value) -> Option<(Activity, Activity)> {
        let s = self.tabs.get_mut(tab)?;
        let before = s.activity;
        if let Some(id) = js::str(e, "session_id") {
            s.session_id = Some(id.into());
        }
        if let Some(p) = js::str(e, "transcript_path") {
            if e.get("agent_id").map(|v| v.is_null()).unwrap_or(true) {
                s.transcript_path = Some(p.into());
            }
        }
        match js::str(e, "hook_event_name").unwrap_or("") {
            "SessionStart" => {
                let m = js::obj(e, "model");
                s.model = m.as_str().map(String::from).or_else(|| js::str(m, "display_name").map(String::from)).or(s.model.take());
                if let Some(t) = js::str(e, "session_title") {
                    s.session_name = Some(t.into());
                }
                set_activity(s, Activity::Idle);
            }
            "UserPromptSubmit" => {
                if let Some(t) = js::str(e, "session_title") {
                    s.session_name = Some(t.into());
                }
                s.error = None;
                set_activity(s, Activity::Working);
            }
            "PreToolUse" => {
                if js::str(e, "tool_name") == Some("ExitPlanMode") {
                    let input = js::obj(e, "tool_input");
                    let text = js::str(input, "plan").map(String::from).or_else(|| {
                        input.as_object().and_then(|m| {
                            m.values().filter_map(|v| v.as_str()).max_by_key(|s| s.len()).map(String::from)
                        })
                    });
                    if let Some(t) = text {
                        s.plan = Some(t);
                    }
                    set_activity(s, Activity::NeedsYou); // plan approval
                }
            }
            "PostToolUse" => {
                if s.activity != Activity::Working {
                    set_activity(s, Activity::Working);
                }
                on_tool(s, e);
            }
            "TaskCreated" => {
                if let Some(id) = js::str(e, "task_id") {
                    if !s.tasks.iter().any(|t| t.id == id) {
                        s.tasks.push(SessionTask {
                            id: id.into(),
                            subject: js::str(e, "task_subject").unwrap_or(id).into(),
                            status: TaskStatus::Pending,
                        });
                    }
                }
            }
            "TaskCompleted" => {
                let id = js::str(e, "task_id");
                if let Some(t) = s.tasks.iter_mut().find(|t| Some(t.id.as_str()) == id) {
                    t.status = TaskStatus::Completed;
                }
            }
            "SubagentStart" => {
                if let Some(kind) = js::str(e, "agent_type") {
                    s.agents.push(SessionAgent {
                        id: js::str(e, "agent_id").map(String::from).unwrap_or_else(|| now_ms().to_string()),
                        kind: kind.into(),
                        label: None,
                        running: true,
                        started_at: now_ms(),
                        ended_at: None,
                    });
                }
            }
            "SubagentStop" => {
                let id = js::str(e, "agent_id");
                if let Some(a) = s.agents.iter_mut().find(|a| Some(a.id.as_str()) == id) {
                    a.running = false;
                    a.ended_at = Some(now_ms());
                }
            }
            "PermissionRequest" => {
                s.message = Some(format!("Wants to use {}", js::str(e, "tool_name").unwrap_or("a tool")));
                set_activity(s, Activity::NeedsYou);
            }
            "Notification" => {
                if WAITING_NOTIFICATIONS.contains(&js::str(e, "notification_type").unwrap_or("")) {
                    s.message = js::str(e, "message").map(|m| js::short(m, 240));
                    set_activity(s, Activity::NeedsYou);
                }
            }
            "Stop" => {
                s.message = js::str(e, "last_assistant_message").map(|m| js::short(m, 240));
                set_activity(s, Activity::Done);
            }
            "StopFailure" => {
                s.error = js::str(e, "error_details")
                    .or_else(|| js::str(e, "last_assistant_message"))
                    .or_else(|| js::str(e, "error"))
                    .map(|m| js::short(m, 300));
                set_activity(s, Activity::Error);
            }
            "SessionEnd" => set_activity(s, Activity::Ended),
            _ => {}
        }
        Some((before, s.activity))
    }

    /// Chat tabs have no status line (print mode), so model / context / cost come from the
    /// stream-json events: assistant `usage` (current context) and `result` (cost, context window).
    pub fn chat_event(&mut self, tab: &str, e: &Value) {
        let Some(s) = self.tabs.get_mut(tab) else { return };
        let ty = js::str(e, "type").unwrap_or("");
        if ty == "system" && js::str(e, "subtype") == Some("init") {
            if let Some(m) = js::str(e, "model") {
                if s.model.is_none() {
                    s.model = Some(pretty_model(m));
                }
            }
        } else if ty == "assistant" && e.get("parent_tool_use_id").map(|v| v.is_null()).unwrap_or(true) {
            let u = js::obj(js::obj(e, "message"), "usage");
            let n = |k: &str| js::num(u, k).unwrap_or(0.0) as u64;
            let tokens = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
            if tokens > 0 {
                let size = s.context.as_ref().map(|c| c.size).unwrap_or(200_000);
                s.context = Some(ContextUse { used_percent: Some((tokens as f64 / size as f64 * 100.0).round()), tokens, size });
            }
        } else if ty == "result" {
            let run = js::num(e, "total_cost_usd").unwrap_or(0.0);
            self.chat_cost_run.insert(tab.into(), run);
            let window = js::obj(e, "modelUsage")
                .as_object()
                .and_then(|m| m.values().find_map(|v| v.get("contextWindow").and_then(|w| w.as_u64())));
            s.cost_usd = Some(self.chat_cost_base.get(tab).copied().unwrap_or(0.0) + run);
            s.duration_ms = Some(s.duration_ms.unwrap_or(0.0) + js::num(e, "duration_ms").unwrap_or(0.0));
            if let (Some(w), Some(c)) = (window, s.context.as_mut()) {
                c.size = w;
                c.used_percent = Some((c.tokens as f64 / w as f64 * 100.0).round());
            }
            // An interrupted or failed turn gets no Stop hook, so end "working" here.
            if e.get("is_error").and_then(|v| v.as_bool()) == Some(true) && matches!(s.activity, Activity::Working | Activity::NeedsYou) {
                set_activity(s, Activity::Idle);
            }
        }
    }

    /// A chat process ended: its cost total becomes the base for the next (resumed) process.
    pub fn chat_process_ended(&mut self, tab: &str) {
        let run = self.chat_cost_run.remove(tab).unwrap_or(0.0);
        *self.chat_cost_base.entry(tab.into()).or_insert(0.0) += run;
        if let Some(s) = self.tabs.get_mut(tab) {
            if matches!(s.activity, Activity::Working | Activity::NeedsYou) {
                set_activity(s, Activity::Idle);
            }
        }
    }

    /// A message was sent in a chat tab.
    pub fn chat_working(&mut self, tab: &str) {
        if let Some(s) = self.tabs.get_mut(tab) {
            s.error = None;
            set_activity(s, Activity::Working);
        }
    }

    /// Reads the status line's per-session files for open tabs. Returns true if anything changed.
    pub fn poll_status(&mut self) -> bool {
        let mut changed = false;
        for (tab, s) in self.tabs.iter_mut() {
            let Some(dir) = self.profile_of.get(tab) else { continue };
            let folder = dir.join(SESSIONS_DIR);
            let mut file = s.session_id.as_ref().map(|id| folder.join(format!("{id}.json")));
            // Before the first hook arrives, find the file the status line tagged with this tab.
            if file.is_none() {
                if let Ok(rd) = std::fs::read_dir(&folder) {
                    for e in rd.flatten() {
                        if read_json_file(&e.path()).and_then(|d| d.get("tab").and_then(|t| t.as_str()).map(|t| t == tab)) == Some(true) {
                            file = Some(e.path());
                        }
                    }
                }
            }
            let Some(d) = file.and_then(|f| read_json_file(&f)) else { continue };
            // SessionStart only supports command hooks, so a running status line is the first sign
            // that Claude Code is up and waiting for input.
            if s.activity == Activity::Starting {
                set_activity(s, Activity::Idle);
                changed = true;
            }
            let cw = js::obj(&d, "context_window");
            let cost = js::obj(&d, "cost");
            let before = (s.session_id.clone(), s.session_name.clone(), s.model.clone(), s.cost_usd, s.duration_ms, s.lines_added, s.lines_removed, s.context.as_ref().map(|c| (c.tokens, c.size)));
            if let Some(v) = js::str(&d, "session_id") {
                s.session_id = Some(v.into());
            }
            if let Some(v) = js::str(&d, "session_name") {
                s.session_name = Some(v.into());
            }
            if let Some(v) = js::str(js::obj(&d, "model"), "display_name") {
                s.model = Some(v.into());
            }
            if let Some(size) = js::num(cw, "context_window_size") {
                s.context = Some(ContextUse {
                    used_percent: js::num(cw, "used_percentage"),
                    tokens: js::num(cw, "total_input_tokens").unwrap_or(0.0) as u64,
                    size: size as u64,
                });
            }
            if let Some(v) = js::num(cost, "total_cost_usd") {
                s.cost_usd = Some(v);
            }
            if let Some(v) = js::num(cost, "total_duration_ms") {
                s.duration_ms = Some(v);
            }
            if let Some(v) = js::num(cost, "total_lines_added") {
                s.lines_added = Some(v as i64);
            }
            if let Some(v) = js::num(cost, "total_lines_removed") {
                s.lines_removed = Some(v as i64);
            }
            let after = (s.session_id.clone(), s.session_name.clone(), s.model.clone(), s.cost_usd, s.duration_ms, s.lines_added, s.lines_removed, s.context.as_ref().map(|c| (c.tokens, c.size)));
            if before != after {
                changed = true;
            }
        }
        changed
    }
}

fn on_tool(s: &mut SessionInfo, e: &Value) {
    let tool = js::str(e, "tool_name").unwrap_or("");
    let input = js::obj(e, "tool_input");
    match tool {
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            let Some(path) = js::str(input, "file_path").or_else(|| js::str(input, "notebook_path")) else { return };
            if let Some(o) = s.outputs.iter_mut().find(|o| o.path == path) {
                o.count += 1;
                o.at = now_ms();
                if tool == "Write" {
                    o.op = OutputOp::Written;
                }
            } else {
                s.outputs.insert(
                    0,
                    SessionOutput { path: path.into(), op: if tool == "Write" { OutputOp::Written } else { OutputOp::Edited }, count: 1, at: now_ms() },
                );
            }
        }
        "TaskUpdate" => {
            let id = js::str(input, "taskId").or_else(|| js::str(input, "task_id")).or_else(|| js::str(input, "id"));
            if let Some(t) = s.tasks.iter_mut().find(|t| Some(t.id.as_str()) == id) {
                match js::str(input, "status") {
                    Some("pending") => t.status = TaskStatus::Pending,
                    Some("in_progress") => t.status = TaskStatus::InProgress,
                    Some("completed") => t.status = TaskStatus::Completed,
                    _ => {}
                }
                if let Some(sub) = js::str(input, "subject") {
                    t.subject = sub.into();
                }
            }
        }
        "TodoWrite" => {
            if let Some(todos) = input.get("todos").and_then(|t| t.as_array()) {
                s.tasks = todos
                    .iter()
                    .enumerate()
                    .map(|(i, t)| SessionTask {
                        id: js::str(t, "id").map(String::from).unwrap_or_else(|| i.to_string()),
                        subject: js::str(t, "content").or_else(|| js::str(t, "subject")).map(String::from).unwrap_or_else(|| format!("Task {}", i + 1)),
                        status: match js::str(t, "status") {
                            Some("in_progress") => TaskStatus::InProgress,
                            Some("completed") => TaskStatus::Completed,
                            _ => TaskStatus::Pending,
                        },
                    })
                    .collect();
            }
        }
        "Bash" if input.get("run_in_background").and_then(|v| v.as_bool()) == Some(true) => {
            s.agents.push(SessionAgent {
                id: js::str(e, "tool_use_id").map(String::from).unwrap_or_else(|| now_ms().to_string()),
                kind: "bash".into(),
                label: js::str(input, "description").or_else(|| js::str(input, "command")).map(|l| js::short(l, 80)),
                running: true,
                started_at: now_ms(),
                ended_at: None,
            });
        }
        _ => {}
    }
}

/// Deletes status-line session files older than a week.
pub fn prune_session_files(profile_dirs: &[PathBuf]) {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(7 * 86_400);
    for dir in profile_dirs {
        let Ok(rd) = std::fs::read_dir(dir.join(SESSIONS_DIR)) else { continue };
        for e in rd.flatten() {
            if e.metadata().and_then(|m| m.modified()).map(|m| m < cutoff).unwrap_or(false) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}
