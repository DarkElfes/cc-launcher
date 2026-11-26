//! Chat mode: drives the official Claude Code CLI in print mode with documented stream-json I/O
//! (https://code.claude.com/docs/en/headless):
//!
//!   claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
//!
//! One long-running process per chat tab. User turns go to stdin as JSON lines; events come back
//! on stdout and are drawn by the app.
//!
//! Permissions: Claude Code runs the PermissionRequest hook (our HTTP hook server) before a tool
//! that needs approval. For chat tabs the request is held open until you click Allow/Deny.
//!
//! Stop: sends an `interrupt` control request on stdin, like the Agent SDK does. The turn ends with
//! a `result` event and the partial answer stays in the session. If that fails, the process is
//! ended and the next message resumes the session with --resume <session id>.

use super::types::{js, ChatPermissionDecision, ChatPermissionRequest};
use super::util::{hidden_command, kill_tree};
use super::BackendEvent;
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc;

pub enum StopKind {
    /// Nothing was running.
    None,
    /// An interrupt was sent to the process with this generation; a `result` event will follow.
    Interrupted(u64),
    /// The process was ended.
    Killed,
}

pub struct ChatStartOptions {
    pub claude: String,
    pub cwd: String,
    pub env: Vec<(String, String)>,
    pub resume: Option<String>,
    pub permission_mode: Option<String>,
    pub model: Option<String>,
}

struct ChatProc {
    child: Option<(Child, ChildStdin, u64)>,
    opts: ChatStartOptions,
    session_id: Option<String>,
    pending: HashMap<String, (ChatPermissionRequest, mpsc::Sender<Value>)>,
    /// Open `can_use_tool` control requests from stdout: (request id, tool name).
    control: Vec<(String, String)>,
}

fn send_control_response(c: &mut ChatProc, response: Value) {
    if let Some((_, stdin, _)) = c.child.as_mut() {
        let _ = writeln!(stdin, "{}", json!({ "type": "control_response", "response": response })).and_then(|_| stdin.flush());
    }
}

/// Answers the oldest open control request for this tool with the same decision as the hook.
fn answer_control(c: &mut ChatProc, tool_name: &str, decision: &Value) {
    if let Some(i) = c.control.iter().position(|(_, t)| t == tool_name) {
        let (id, _) = c.control.remove(i);
        send_control_response(c, json!({ "subtype": "success", "request_id": id, "response": decision }));
    }
}

/// Keys of init/result events that are only diagnostics.
const NOISE: &[&str] = &[
    "startup_timing", "time_to_request_phases_ms", "input_attachments_detail", "tool_schema_build_detail",
    "message_normalization_detail", "request_build_detail", "model_usage_since_last_result", "subagent_stats",
];

pub struct ChatManager {
    chats: HashMap<String, ChatProc>,
    events: async_channel::Sender<BackendEvent>,
    next_gen: u64,
}

impl ChatManager {
    pub fn new(events: async_channel::Sender<BackendEvent>) -> Self {
        Self { chats: HashMap::new(), events, next_gen: 1 }
    }

    pub fn has(&self, tab: &str) -> bool {
        self.chats.contains_key(tab)
    }

    pub fn open(&mut self, tab: &str, opts: ChatStartOptions) {
        let session_id = opts.resume.clone();
        self.chats.insert(tab.into(), ChatProc { child: None, opts, session_id, pending: HashMap::new(), control: vec![] });
    }

    /// Remembers the session id from an event (so a restarted process resumes it).
    pub fn note_event(&mut self, tab: &str, generation: u64, e: &Value) -> bool {
        let Some(c) = self.chats.get_mut(tab) else { return false };
        if c.child.as_ref().map(|x| x.2) != Some(generation) {
            return false; // from a process that was already replaced
        }
        if let Some(id) = js::str(e, "session_id") {
            c.session_id = Some(id.into());
        }
        if js::str(e, "type") == Some("control_request") {
            // Claude Code asks for the same permission here and through the PermissionRequest hook;
            // the first answer wins. The hook shows the card, so this request just waits and gets
            // the same answer when the card is answered (reply_permission).
            let id = js::str(e, "request_id").unwrap_or("").to_string();
            let req = e.get("request").cloned().unwrap_or(json!({}));
            if js::str(&req, "subtype") == Some("can_use_tool") {
                c.control.push((id, js::str(&req, "tool_name").unwrap_or("").to_string()));
            } else {
                let sub = js::str(&req, "subtype").unwrap_or("").to_string();
                send_control_response(c, json!({ "subtype": "error", "request_id": id, "error": format!("Unsupported request: {sub}") }));
            }
            return false;
        }
        if js::str(e, "type") == Some("control_cancel_request") {
            let id = js::str(e, "request_id").unwrap_or("");
            c.control.retain(|(r, _)| r != id);
            return false;
        }
        true
    }

    /// The process with this generation exited. Returns true when it was the current one.
    pub fn process_exited(&mut self, tab: &str, generation: u64) -> bool {
        let Some(c) = self.chats.get_mut(tab) else { return false };
        if c.child.as_ref().map(|x| x.2) != Some(generation) {
            return false;
        }
        c.child = None;
        deny_pending(c, "The session stopped.");
        true
    }

    fn start(&mut self, tab: &str) -> Result<(), String> {
        let generation = self.next_gen;
        let events = self.events.clone();
        let c = self.chats.get_mut(tab).ok_or("This chat is closed.")?;
        if c.child.is_some() {
            return Ok(());
        }
        // `--permission-prompt-tool stdio` (as the Agent SDK uses) makes AskUserQuestion and
        // ExitPlanMode available in print mode. Our PermissionRequest hook still decides; requests
        // it leaves open arrive as `control_request` events (see note_event).
        let mut args = vec!["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        if let Some(id) = &c.session_id {
            args.push("--resume".into());
            args.push(id.clone());
        }
        if let Some(m) = &c.opts.model {
            args.push("--model".into());
            args.push(m.clone());
        }
        if let Some(m) = &c.opts.permission_mode {
            args.push("--permission-mode".into());
            args.push(m.clone());
        }
        let mut cmd = hidden_command(&c.opts.claude);
        cmd.args(&args)
            .current_dir(&c.opts.cwd)
            .env_clear()
            .envs(c.opts.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Claude Code: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let mut stderr = child.stderr.take().ok_or("no stderr")?;
        self.next_gen += 1;
        let tab_s = tab.to_string();
        let ev2 = events.clone();
        let stderr_tail = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let tail2 = stderr_tail.clone();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            *tail2.lock().unwrap() = buf.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(mut e) = serde_json::from_str::<Value>(line) else { continue };
                if let Some(m) = e.as_object_mut() {
                    for k in NOISE {
                        m.remove(*k);
                    }
                }
                if ev2.send_blocking(BackendEvent::ChatEvent { tab: tab_s.clone(), generation, event: e }).is_err() {
                    break;
                }
            }
            // stdout closed: wait a moment for stderr to be collected, then report the exit.
            std::thread::sleep(std::time::Duration::from_millis(150));
            let tail = stderr_tail.lock().unwrap().clone();
            let _ = ev2.send_blocking(BackendEvent::ChatExit { tab: tab_s, generation, stderr: tail });
        });
        c.child = Some((child, stdin, generation));
        Ok(())
    }

    /// Sends a user turn (text + image files). Starts or resumes the process if needed.
    pub fn send_message(&mut self, tab: &str, text: &str, images: &[String]) -> Result<(), String> {
        self.start(tab)?;
        let c = self.chats.get_mut(tab).ok_or("This chat is closed.")?;
        let (_, stdin, _) = c.child.as_mut().ok_or("Could not start Claude Code.")?;
        let mut content = vec![];
        for p in images {
            let ext = std::path::Path::new(p).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let media = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => continue,
            };
            let Ok(bytes) = std::fs::read(p) else { continue };
            content.push(json!({ "type": "image", "source": { "type": "base64", "media_type": media, "data": base64::engine::general_purpose::STANDARD.encode(bytes) } }));
        }
        if !text.trim().is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
        if content.is_empty() {
            return Ok(());
        }
        let msg = json!({ "type": "user", "message": { "role": "user", "content": content }, "parent_tool_use_id": null });
        writeln!(stdin, "{msg}").and_then(|_| stdin.flush()).map_err(|e| format!("Could not send: {e}"))
    }

    /// Interrupts the running turn with a stream-json `interrupt` control request (what the Agent
    /// SDK's interrupt() sends). The process stays alive and keeps the partial answer in the
    /// session, and a `result` event ends the turn. Falls back to ending the process.
    pub fn stop(&mut self, tab: &str) -> StopKind {
        let Some(c) = self.chats.get_mut(tab) else { return StopKind::None };
        let Some((_, stdin, generation)) = c.child.as_mut() else { return StopKind::None };
        let generation = *generation;
        let req = json!({ "type": "control_request", "request_id": uuid::Uuid::new_v4().to_string(), "request": { "subtype": "interrupt" } });
        let sent = writeln!(stdin, "{req}").and_then(|_| stdin.flush()).is_ok();
        deny_all(c, "Stopped by the user.", true);
        if sent {
            return StopKind::Interrupted(generation);
        }
        self.kill(tab);
        StopKind::Killed
    }

    /// Ends the process (the next message resumes the session). Used when an interrupt didn't work.
    pub fn kill(&mut self, tab: &str) -> bool {
        let Some(c) = self.chats.get_mut(tab) else { return false };
        let Some((child, _, _)) = c.child.take() else { return false };
        deny_pending(c, "Stopped by the user.");
        kill_tree(child.id());
        true
    }

    /// The generation of the running process, if any.
    pub fn generation(&self, tab: &str) -> Option<u64> {
        self.chats.get(tab)?.child.as_ref().map(|x| x.2)
    }

    /// Restart with another model (keeps the conversation).
    pub fn set_model(&mut self, tab: &str, model: Option<String>) {
        let Some(c) = self.chats.get_mut(tab) else { return };
        c.opts.model = model;
        if let Some((child, _, _)) = c.child.take() {
            kill_tree(child.id());
        }
    }

    /// Restart with another permission mode (keeps the conversation).
    pub fn set_permission_mode(&mut self, tab: &str, mode: Option<String>) {
        let Some(c) = self.chats.get_mut(tab) else { return };
        c.opts.permission_mode = mode;
        if let Some((child, _, _)) = c.child.take() {
            kill_tree(child.id());
        }
    }

    pub fn close(&mut self, tab: &str) {
        if let Some(mut c) = self.chats.remove(tab) {
            deny_pending(&mut c, "The tab was closed.");
            if let Some((child, _, _)) = c.child.take() {
                kill_tree(child.id());
            }
        }
    }

    pub fn close_all(&mut self) {
        for id in self.chats.keys().cloned().collect::<Vec<_>>() {
            self.close(&id);
        }
    }

    /// PermissionRequest hook for a chat tab: remember the responder; the UI shows a card.
    pub fn request_permission(&mut self, tab: &str, e: &Value, reply: mpsc::Sender<Value>) -> Option<ChatPermissionRequest> {
        let c = self.chats.get_mut(tab)?;
        let request = ChatPermissionRequest {
            tab_id: tab.into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            tool_name: js::str(e, "tool_name").unwrap_or("").into(),
            input: e.get("tool_input").cloned().unwrap_or(json!({})),
            suggestions: e.get("permission_suggestions").and_then(|s| s.as_array()).cloned().unwrap_or_default(),
        };
        c.pending.insert(request.request_id.clone(), (request.clone(), reply));
        Some(request)
    }

    /// Sends the decision back to Claude Code. Returns the decision name for the card.
    pub fn reply_permission(&mut self, tab: &str, request_id: &str, d: ChatPermissionDecision) -> Option<&'static str> {
        let c = self.chats.get_mut(tab)?;
        let (req, tx) = c.pending.remove(request_id)?;
        let (name, decision) = decision_json(&req, d);
        answer_control(c, &req.tool_name, &decision);
        let _ = tx.send(json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }));
        Some(name)
    }
}

fn decision_json(req: &ChatPermissionRequest, d: ChatPermissionDecision) -> (&'static str, Value) {
    match d {
        ChatPermissionDecision::Allow { updated_input, updated_permissions } => {
            let mut v = json!({ "behavior": "allow", "updatedInput": updated_input.unwrap_or_else(|| req.input.clone()) });
            if let Some(p) = updated_permissions {
                v["updatedPermissions"] = Value::Array(p);
            }
            ("allow", v)
        }
        ChatPermissionDecision::Deny { message, interrupt } => {
            let mut v = json!({ "behavior": "deny", "message": message.unwrap_or_else(|| "The user denied this.".into()) });
            if interrupt {
                v["interrupt"] = json!(true);
            }
            ("deny", v)
        }
    }
}

fn deny_pending(c: &mut ChatProc, message: &str) {
    deny_all(c, message, false);
}

/// Denies every open permission request (hook and control requests).
fn deny_all(c: &mut ChatProc, message: &str, interrupt: bool) {
    let pending: Vec<_> = c.pending.drain().collect();
    for (_, (req, tx)) in pending {
        let (_, decision) = decision_json(&req, ChatPermissionDecision::Deny { message: Some(message.into()), interrupt });
        let _ = tx.send(json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }));
    }
    let deny = json!({ "behavior": "deny", "message": message, "interrupt": interrupt });
    for (id, _) in std::mem::take(&mut c.control) {
        send_control_response(c, json!({ "subtype": "success", "request_id": id, "response": deny }));
    }
}

/// Earlier messages of a resumed session, from its transcript file (best effort; internal format).
pub fn read_transcript_messages(path: &str, limit: usize) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else { return vec![] };
    let mut out = vec![];
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        let ty = js::str(&e, "type").unwrap_or("");
        let flag = |k: &str| e.get(k).and_then(|v| v.as_bool()) == Some(true);
        if (ty == "user" || ty == "assistant") && e.get("message").is_some() && !flag("isMeta") && !flag("isSidechain") {
            out.push(json!({ "type": ty, "message": e["message"], "parent_tool_use_id": null, "uuid": e.get("uuid"), "history": true }));
        }
    }
    let skip = out.len().saturating_sub(limit);
    out.split_off(skip)
}
