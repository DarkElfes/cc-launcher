//! Conversation state per chat tab, built from Claude Code's stream-json events
//! (https://code.claude.com/docs/en/headless).

use crate::backend::types::{js, ChatPermissionRequest};
use crate::backend::thinking_cache;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ToolResult {
    pub text: String,
    pub is_error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermState {
    Pending,
    Allow,
    Deny,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // ids keep items distinguishable when debugging
pub enum ChatItem {
    User { id: u64, text: String, images: Vec<String>, image_count: usize },
    Text { id: u64, msg_id: Option<String>, text: String, streaming: bool },
    Thinking { id: u64, msg_id: Option<String>, text: String, streaming: bool, open: bool },
    Tool { id: u64, tool_id: String, name: String, input: Value, result: Option<ToolResult>, steps: u32, open: bool },
    Permission { id: u64, request: ChatPermissionRequest, state: PermState, answers: Vec<(String, Vec<String>)> },
    Notice { id: u64, text: String, tone: Tone },
    Result { id: u64, duration_ms: Option<f64>, cost_usd: Option<f64>, is_error: bool, subtype: Option<String>, denials: Vec<String> },
}


#[derive(Default)]
pub struct ChatState {
    pub items: Vec<ChatItem>,
    pub busy: bool,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    /// Stop was clicked; the turn's `result` shows as "Stopped" instead of an error.
    pub stop_requested: bool,
    /// Finished live thinking blocks (cache key, text) for the app to save; see thinking_cache.
    pub thinking_done: Vec<(String, String)>,
    /// Saved thinking used while loading history (transcripts don't keep the text).
    thinking_saved: HashMap<String, String>,
    current_msg: Option<String>,
    seq: u64,
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|b| match js::str(b, "type") {
                Some("text") => Some(b.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string()),
                Some("image") => Some("[image]".into()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

impl ChatState {
    fn nid(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Last streaming text/thinking item for an API message id.
    fn find_streaming(&self, thinking: bool, msg_id: Option<&str>) -> Option<usize> {
        self.items.iter().rposition(|it| match it {
            ChatItem::Text { msg_id: m, streaming: true, .. } if !thinking => msg_id.is_none() || m.as_deref() == msg_id,
            ChatItem::Thinking { msg_id: m, streaming: true, .. } if thinking => msg_id.is_none() || m.as_deref() == msg_id,
            _ => false,
        })
    }

    fn end_streaming(&mut self) {
        for it in &mut self.items {
            match it {
                ChatItem::Text { streaming, .. } | ChatItem::Thinking { streaming, .. } => *streaming = false,
                _ => {}
            }
        }
    }

    pub fn apply(&mut self, e: &Value) {
        let ty = js::str(e, "type").unwrap_or("");
        let parent = js::str(e, "parent_tool_use_id").map(String::from);
        match ty {
            "system" => match js::str(e, "subtype") {
                Some("init") => {
                    if let Some(m) = js::str(e, "model") {
                        self.model = Some(m.into());
                    }
                    if let Some(m) = js::str(e, "permissionMode") {
                        self.permission_mode = Some(m.into());
                    }
                }
                Some("api_retry") => {
                    let text = format!(
                        "Retrying ({}), attempt {}/{}…",
                        js::str(e, "error").unwrap_or("error"),
                        e.get("attempt").map(|v| v.to_string()).unwrap_or_default(),
                        e.get("max_retries").map(|v| v.to_string()).unwrap_or_default()
                    );
                    self.notice(text, Tone::Warn);
                }
                Some("compact_boundary") => self.notice("Conversation compacted to free up context.".into(), Tone::Info),
                Some("status") => {
                    if let Some(m) = js::str(e, "permissionMode") {
                        self.permission_mode = Some(m.into());
                    }
                }
                _ => {}
            },
            "stream_event" if parent.is_none() => {
                let ev = js::obj(e, "event");
                match js::str(ev, "type") {
                    Some("message_start") => self.current_msg = js::str(js::obj(ev, "message"), "id").map(String::from),
                    Some("content_block_start") => {
                        let block = js::obj(ev, "content_block");
                        let msg_id = self.current_msg.clone();
                        match js::str(block, "type") {
                            Some("text") => {
                                let id = self.nid();
                                self.items.push(ChatItem::Text { id, msg_id, text: String::new(), streaming: true });
                            }
                            Some("thinking") => {
                                let id = self.nid();
                                self.items.push(ChatItem::Thinking { id, msg_id, text: String::new(), streaming: true, open: false });
                            }
                            _ => {}
                        }
                    }
                    Some("content_block_delta") => {
                        let d = js::obj(ev, "delta");
                        let thinking = match js::str(d, "type") {
                            Some("text_delta") => false,
                            Some("thinking_delta") => true,
                            _ => return,
                        };
                        let piece = d.get("text").or_else(|| d.get("thinking")).and_then(|v| v.as_str()).unwrap_or("");
                        if piece.is_empty() {
                            return;
                        }
                        let cur = self.current_msg.clone();
                        if let Some(i) = self.find_streaming(thinking, cur.as_deref()) {
                            match &mut self.items[i] {
                                ChatItem::Text { text, .. } | ChatItem::Thinking { text, .. } => text.push_str(piece),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            "assistant" => {
                let msg = js::obj(e, "message");
                let msg_id = js::str(msg, "id").map(String::from);
                let content = msg.get("content").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                for b in content {
                    let bt = js::str(&b, "type").unwrap_or("");
                    if let Some(p) = &parent {
                        // Subagent work: count its tool calls on the parent tool card.
                        if bt == "tool_use" {
                            if let Some(ChatItem::Tool { steps, .. }) =
                                self.items.iter_mut().find(|x| matches!(x, ChatItem::Tool { tool_id, .. } if tool_id == p))
                            {
                                *steps += 1;
                            }
                        }
                        continue;
                    }
                    if bt == "text" || bt == "thinking" {
                        let thinking = bt == "thinking";
                        let history = e.get("history").and_then(|v| v.as_bool()) == Some(true);
                        let mut text = b.get("text").or_else(|| b.get("thinking")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        // Position of this thinking block within its message (the cache key).
                        let nth = |items: &[ChatItem], upto: usize| {
                            items[..upto].iter().filter(|x| matches!(x, ChatItem::Thinking { msg_id: m, .. } if *m == msg_id)).count()
                        };
                        if let Some(i) = self.find_streaming(thinking, msg_id.as_deref()) {
                            let n = nth(&self.items, i);
                            match &mut self.items[i] {
                                ChatItem::Text { text: t, streaming, .. } | ChatItem::Thinking { text: t, streaming, .. } => {
                                    if !text.is_empty() {
                                        *t = text;
                                    }
                                    *streaming = false;
                                    if let (true, Some(m)) = (thinking && !t.is_empty(), &msg_id) {
                                        self.thinking_done.push((thinking_cache::key(m, n), t.clone()));
                                    }
                                }
                                _ => {}
                            }
                        } else if thinking && (history || !text.is_empty()) {
                            let n = nth(&self.items, self.items.len());
                            if let Some(m) = &msg_id {
                                if text.is_empty() {
                                    text = self.thinking_saved.get(&thinking_cache::key(m, n)).cloned().unwrap_or_default();
                                } else if !history {
                                    self.thinking_done.push((thinking_cache::key(m, n), text.clone()));
                                }
                            }
                            let id = self.nid();
                            self.items.push(ChatItem::Thinking { id, msg_id: msg_id.clone(), text, streaming: false, open: false });
                        } else if !text.is_empty() {
                            let id = self.nid();
                            self.items.push(if thinking {
                                ChatItem::Thinking { id, msg_id: msg_id.clone(), text, streaming: false, open: false }
                            } else {
                                ChatItem::Text { id, msg_id: msg_id.clone(), text, streaming: false }
                            });
                        }
                    } else if bt == "tool_use" {
                        let tool_id = js::str(&b, "id").map(String::from).unwrap_or_else(|| self.seq.to_string());
                        if !self.items.iter().any(|x| matches!(x, ChatItem::Tool { tool_id: t, .. } if *t == tool_id)) {
                            let id = self.nid();
                            self.items.push(ChatItem::Tool {
                                id,
                                tool_id,
                                name: js::str(&b, "name").unwrap_or("tool").into(),
                                input: b.get("input").cloned().unwrap_or(Value::Null),
                                result: None,
                                steps: 0,
                                open: false,
                            });
                        }
                    }
                }
            }
            "user" => {
                if e.get("isReplay").and_then(|v| v.as_bool()) == Some(true) {
                    return;
                }
                let history = e.get("history").and_then(|v| v.as_bool()) == Some(true);
                let content = js::obj(js::obj(e, "message"), "content").clone();
                match content {
                    Value::String(s) => {
                        if history && !s.starts_with('<') {
                            let id = self.nid();
                            self.items.push(ChatItem::User { id, text: s, images: vec![], image_count: 0 });
                        }
                    }
                    Value::Array(parts) => {
                        let mut text = String::new();
                        let mut image_count = 0;
                        for b in parts {
                            match js::str(&b, "type") {
                                Some("tool_result") => {
                                    let tid = js::str(&b, "tool_use_id").unwrap_or("");
                                    if let Some(ChatItem::Tool { result, .. }) =
                                        self.items.iter_mut().find(|x| matches!(x, ChatItem::Tool { tool_id, .. } if tool_id == tid))
                                    {
                                        *result = Some(ToolResult {
                                            text: result_text(b.get("content").unwrap_or(&Value::Null)),
                                            is_error: b.get("is_error").and_then(|v| v.as_bool()) == Some(true),
                                        });
                                    }
                                }
                                Some("text") if history && parent.is_none() => text.push_str(b.get("text").and_then(|t| t.as_str()).unwrap_or("")),
                                Some("image") if history && parent.is_none() => image_count += 1,
                                _ => {}
                            }
                        }
                        if (!text.is_empty() && !text.starts_with('<')) || image_count > 0 {
                            let id = self.nid();
                            self.items.push(ChatItem::User { id, text, images: vec![], image_count });
                        }
                    }
                    _ => {}
                }
            }
            "result" => {
                if std::mem::take(&mut self.stop_requested) {
                    self.exited(true, Some(0), "");
                    return;
                }
                let denials = e
                    .get("permission_denials")
                    .and_then(|d| d.as_array())
                    .map(|a| a.iter().map(|d| js::str(d, "tool_name").unwrap_or("tool").to_string()).collect())
                    .unwrap_or_default();
                let id = self.nid();
                self.items.push(ChatItem::Result {
                    id,
                    duration_ms: js::num(e, "duration_ms"),
                    cost_usd: js::num(e, "total_cost_usd"),
                    is_error: e.get("is_error").and_then(|v| v.as_bool()) == Some(true),
                    subtype: js::str(e, "subtype").map(String::from),
                    denials,
                });
                self.end_streaming();
                self.busy = false;
            }
            _ => {}
        }
    }

    pub fn load_history(&mut self, events: &[Value]) {
        self.thinking_saved = thinking_cache::load(&thinking_cache::keys_in(events));
        for e in events {
            self.apply(e);
        }
        self.thinking_saved.clear();
        if !events.is_empty() {
            self.notice("Earlier messages of this session. New messages continue it.".into(), Tone::Info);
        }
    }

    pub fn add_user(&mut self, text: &str, images: &[String]) {
        let id = self.nid();
        self.busy = true;
        self.items.push(ChatItem::User { id, text: text.into(), images: images.to_vec(), image_count: 0 });
    }

    pub fn notice(&mut self, text: String, tone: Tone) {
        let id = self.nid();
        self.items.push(ChatItem::Notice { id, text, tone });
    }

    pub fn add_permission(&mut self, request: ChatPermissionRequest) {
        let id = self.nid();
        self.items.push(ChatItem::Permission { id, request, state: PermState::Pending, answers: vec![] });
    }

    pub fn permission_done(&mut self, request_id: &str, decision: &str) {
        for it in &mut self.items {
            if let ChatItem::Permission { request, state, .. } = it {
                if request.request_id == request_id {
                    *state = match decision {
                        "allow" => PermState::Allow,
                        "deny" => PermState::Deny,
                        _ => PermState::Expired,
                    };
                }
            }
        }
    }

    /// Pending permission cards that will never get an answer (process ended).
    pub fn expire_pending(&mut self) {
        for it in &mut self.items {
            if let ChatItem::Permission { state, .. } = it {
                if *state == PermState::Pending {
                    *state = PermState::Expired;
                }
            }
        }
    }

    pub fn exited(&mut self, stopped: bool, code: Option<i32>, stderr: &str) {
        self.busy = false;
        self.stop_requested = false;
        self.end_streaming();
        self.expire_pending();
        if stopped {
            self.notice("Stopped. Your next message continues the conversation.".into(), Tone::Warn);
        } else if let Some(c) = code.filter(|c| *c != 0) {
            let extra = if stderr.is_empty() { String::new() } else { format!(" {stderr}") };
            self.notice(format!("Claude Code exited (code {c}).{extra}"), Tone::Error);
        }
    }
}
