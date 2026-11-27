//! Voice dictation for chat tabs through a hidden, interactive Claude Code.
//!
//! Chat tabs run `claude -p` (stream-json), which has no prompt and so no `/voice`. Claude Code's
//! dictation only exists in its terminal UI, and the app must not call Anthropic's voice service
//! itself (it would need the account's credentials). So a chat tab borrows a hidden terminal
//! Claude Code for the same account: the official binary records the microphone, streams it to
//! Anthropic under its own sign-in and types the words into *its* prompt; the app reads that prompt
//! line off the screen and copies the text into the chat message box. The hidden session never
//! sends a prompt (hold mode, autoSubmit off), so it uses no messages.

use super::terminal::{SpawnOptions, Terminal};
use super::BackendEvent;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Settings for the hidden session only: hold-to-talk with nothing sent on release.
pub const DICTATION_SETTINGS: &str = r#"{"voice":{"enabled":true,"mode":"hold","autoSubmit":false},"spinnerTipsEnabled":false}"#;

/// Tab id prefix of hidden dictation sessions (their terminal events are routed here, not to tabs).
pub const TAB_PREFIX: &str = "dictation-";

pub fn spawn(id: &str, claude: &str, cwd: &str, config_dir: &str, events: async_channel::Sender<BackendEvent>) -> anyhow::Result<Terminal> {
    let mut env = HashMap::new();
    env.insert("CLAUDE_CONFIG_DIR".to_string(), config_dir.to_string());
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("COLORTERM".into(), "truecolor".into());
    // No CCL_TAB_ID / CCL_HOOK_TOKEN: the hook server ignores this session.
    let opts = SpawnOptions {
        program: claude.to_string(),
        args: vec!["--settings".into(), DICTATION_SETTINGS.into()],
        cwd: cwd.to_string(),
        env,
        cols: 160,
        rows: 40,
    };
    Terminal::spawn(id, opts, events)
}

/// The visible screen as plain text lines (trailing spaces trimmed).
pub fn screen_lines(term: &Terminal) -> Vec<String> {
    let t = term.term.lock();
    let grid = t.grid();
    let cols = grid.columns();
    let mut out = Vec::with_capacity(grid.screen_lines());
    for l in 0..grid.screen_lines() {
        let row = &grid[Line(l as i32)];
        let mut s = String::with_capacity(cols);
        for c in 0..cols {
            let ch = row[Column(c)].c;
            s.push(if ch == '\0' { ' ' } else { ch });
        }
        out.push(s.trim_end().to_string());
    }
    out
}

/// Ctrl+U: deletes the prompt text before the cursor (dictated text leaves the cursor at the end).
pub const CLEAR_PROMPT: &[u8] = b"\x15";

/// What Claude Code's prompt box shows, read off the screen.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PromptState {
    /// The prompt text (wrapped lines joined; empty for the "Try …" placeholder).
    pub text: String,
    /// The status line above the box (e.g. "listening…", "No speech detected.", an error).
    pub status: String,
    /// The prompt box is on screen (the session is ready for dictation).
    pub ready: bool,
}

fn is_rule(l: &str) -> bool {
    l.chars().take(20).filter(|c| *c == '─').count() >= 20
}

/// Finds the prompt box (the last two horizontal rules) and reads the text between them.
/// The layout is Claude Code's own and can change between versions; it was checked on 2.1.319:
///
/// ```text
///                                      <status, right-aligned>
/// ──────────────────────────────
/// ❯ the prompt text
///   continuation lines
/// ──────────────────────────────
/// ```
pub fn read_prompt(term: &Terminal) -> PromptState {
    use alacritty_terminal::term::cell::Flags;
    let t = term.term.lock();
    let grid = t.grid();
    let cols = grid.columns();
    let rows = grid.screen_lines();
    let mut lines: Vec<(String, bool)> = Vec::with_capacity(rows);
    for l in 0..rows {
        let row = &grid[Line(l as i32)];
        let mut s = String::with_capacity(cols);
        for c in 0..cols {
            let ch = row[Column(c)].c;
            s.push(if ch == '\0' { ' ' } else { ch });
        }
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        lines.push((s.trim_end().to_string(), wrapped));
    }
    drop(t);
    let rules: Vec<usize> = lines.iter().enumerate().filter(|(_, (l, _))| is_rule(l)).map(|(i, _)| i).collect();
    let (Some(&bottom), Some(&top)) = (rules.last(), rules.iter().rev().nth(1)) else {
        return PromptState::default();
    };
    let mut text = String::new();
    let mut prev_wrapped = false;
    for (i, (l, wrapped)) in lines[top + 1..bottom].iter().enumerate() {
        let body = if i == 0 { l.trim_start_matches(['❯', ' ', '\u{a0}']) } else { l.strip_prefix("  ").unwrap_or(l) };
        if i > 0 && !prev_wrapped {
            text.push('\n');
        }
        text.push_str(body);
        prev_wrapped = *wrapped;
    }
    // While recording, the cursor is a bar that follows the microphone level.
    let text = text.trim_end_matches(['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', ' ', '\u{a0}']).trim().to_string();
    // The empty prompt shows an example as a placeholder.
    let text = if text.starts_with("Try \"") && text.ends_with('"') { String::new() } else { text };
    let status = if top > 0 { lines[top - 1].0.trim().to_string() } else { String::new() };
    let ready = lines.get(top + 1).is_some_and(|(l, _)| l.starts_with('❯'));
    PromptState { text, status, ready }
}

/// Sends the dictation key (Alt+K, bound to `voice:pushToTalk`) repeatedly, like a held key's
/// auto-repeat, while `held` is set. Claude Code's hold mode records while the repeats arrive.
pub fn hold_key(term_input: impl Fn(Vec<u8>) + Send + 'static, held: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while held.load(Ordering::Relaxed) {
            term_input(super::profile_settings::DICTATE_SEQUENCE.to_vec());
            std::thread::sleep(Duration::from_millis(33));
        }
    });
}

/// Debug helper: `cc-launcher --dump-claude-screen <claude.exe> <cwd> <config dir>` starts a hidden
/// session, waits, and prints its screen (to see the prompt layout after a Claude Code update).
pub fn dump_screen(args: &[String]) {
    let (Some(claude), Some(cwd), Some(dir)) = (args.first(), args.get(1), args.get(2)) else {
        eprintln!("usage: --dump-claude-screen <claude.exe> <cwd> <config dir> [seconds] [text-to-type]");
        return;
    };
    let secs: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8);
    let (tx, _rx) = async_channel::unbounded();
    let term = match spawn("dictation-debug", claude, cwd, dir, tx) {
        Ok(t) => t,
        Err(e) => return eprintln!("spawn failed: {e}"),
    };
    std::thread::sleep(Duration::from_secs(secs));
    if let Some(text) = args.get(4) {
        if let Some(n) = text.strip_prefix("hold:").and_then(|n| n.parse::<u64>().ok()) {
            // Hold the dictation key for n seconds, print the screen while held, then after release.
            let held = Arc::new(AtomicBool::new(true));
            let sender = term.input_sender();
            hold_key(move |b| sender(b), held.clone());
            std::thread::sleep(Duration::from_secs(n));
            for l in screen_lines(&term).iter().rev().take(8).collect::<Vec<_>>().into_iter().rev() {
                println!("held|{l}");
            }
            held.store(false, Ordering::Relaxed);
            std::thread::sleep(Duration::from_secs(3));
        } else {
            term.write(text.as_bytes().to_vec());
            std::thread::sleep(Duration::from_secs(1));
            if args.get(5).map(String::as_str) == Some("clear") {
                println!("typed: {:?}", read_prompt(&term));
                term.write(CLEAR_PROMPT.to_vec());
                std::thread::sleep(Duration::from_secs(1));
                println!("after clear: {:?}", read_prompt(&term));
            }
        }
    }
    for (i, l) in screen_lines(&term).iter().enumerate() {
        println!("{i:02}|{l}");
    }
    term.kill();
}
