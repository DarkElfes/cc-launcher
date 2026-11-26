//! Receives Claude Code HTTP hooks (https://code.claude.com/docs/en/hooks) from sessions this app
//! started. Listens on 127.0.0.1 only, on a random free port. Each app run makes a new secret,
//! passed to its claude processes as CCL_HOOK_TOKEN and sent back in the Authorization header,
//! so other local programs can't inject events. The tab id comes from the X-CCL-Tab header.
//!
//! Answers 200 with an empty body ("success, no decision") unless the app replies with a decision
//! (PermissionRequest in chat tabs), which may take up to the hook timeout.

use super::BackendEvent;
use serde_json::Value;
use std::io::Read;
use std::time::Duration;

pub const HOOK_PATH: &str = "/ccl-hook";

pub struct HookServer {
    pub token: String,
    pub port: u16,
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("random");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time comparison so the token can't be guessed byte by byte.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl HookServer {
    pub fn start(events: async_channel::Sender<BackendEvent>) -> anyhow::Result<Self> {
        let token = random_hex(24);
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| anyhow::anyhow!("{e}"))?;
        let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
        let expected = format!("Bearer {token}");
        std::thread::Builder::new().name("hook-server".into()).spawn(move || {
            for mut req in server.incoming_requests() {
                let header = |name: &str| {
                    req.headers()
                        .iter()
                        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
                        .map(|h| h.value.as_str().to_string())
                        .unwrap_or_default()
                };
                if req.method() != &tiny_http::Method::Post || req.url() != HOOK_PATH {
                    let _ = req.respond(tiny_http::Response::empty(404));
                    continue;
                }
                if !same(header("authorization").as_bytes(), expected.as_bytes()) {
                    if std::env::var_os("CCL_DEBUG_HOOKS").is_some() { eprintln!("hook 401 auth={:?}", header("authorization").len()); }
                    let _ = req.respond(tiny_http::Response::empty(401));
                    continue;
                }
                let tab = header("x-ccl-tab");
                let mut body = Vec::new();
                let _ = req.as_reader().take(2 * 1024 * 1024).read_to_end(&mut body);
                let Ok(event) = serde_json::from_slice::<Value>(&body) else {
                    let _ = req.respond(tiny_http::Response::empty(200));
                    continue;
                };
                if tab.is_empty() {
                    let _ = req.respond(tiny_http::Response::empty(200));
                    continue;
                }
                if std::env::var_os("CCL_DEBUG_HOOKS").is_some() {
                    eprintln!("hook tab={tab} event={:?}", event.get("hook_event_name"));
                }
                let wants_reply =event.get("hook_event_name").and_then(|v| v.as_str()) == Some("PermissionRequest");
                if !wants_reply {
                    let _ = events.send_blocking(BackendEvent::Hook { tab, event, reply: None });
                    let _ = req.respond(tiny_http::Response::empty(200));
                    continue;
                }
                // Held open until the app answers (chat tabs) or drops the sender (terminal tabs).
                let (tx, rx) = std::sync::mpsc::channel::<Value>();
                let _ = events.send_blocking(BackendEvent::Hook { tab, event, reply: Some(tx) });
                std::thread::spawn(move || match rx.recv_timeout(Duration::from_secs(595)) {
                    Ok(body) => {
                        let resp = tiny_http::Response::from_string(body.to_string()).with_header(
                            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                        );
                        let _ = req.respond(resp);
                    }
                    Err(_) => {
                        let _ = req.respond(tiny_http::Response::empty(200));
                    }
                });
            }
        })?;
        Ok(Self { token, port })
    }
}
