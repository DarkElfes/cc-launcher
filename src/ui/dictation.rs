//! Chat-tab voice dictation, driven through a hidden interactive Claude Code per account
//! (see backend/dictation.rs for why). Press the mic (or Ctrl+Space) to start, again to stop;
//! the words appear live in the message box and stay there to edit and send.

use super::app::LauncherApp;
use crate::backend::dictation::{self, PromptState, CLEAR_PROMPT, TAB_PREFIX};
use crate::backend::profiles::profile_dir;
use crate::backend::terminal::Terminal;
use crate::backend::types::now_ms;
use gpui_kit::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A hidden Claude Code kept warm for one account.
pub struct DictationSession {
    pub term: Terminal,
    pub last_used: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DictPhase {
    /// Waiting for the hidden session's prompt to appear.
    Starting,
    /// The dictation key is held; words stream into the hidden prompt.
    Listening,
    /// Key released; waiting for the final transcript.
    Finishing,
}

pub struct DictationRun {
    pub tab: String,
    pub profile: String,
    /// The message box text before dictation started (dictated words are appended).
    pub base: String,
    /// Hidden-prompt text that isn't part of this run's words (leftovers the clear didn't remove,
    /// or words already handed over before the user edited the message box).
    pub prefix: String,
    /// What the run last put into the message box; anything else there means the user edited it.
    pub shown: String,
    pub held: Arc<AtomicBool>,
    pub phase: DictPhase,
    pub phase_at: i64,
    pub last: PromptState,
    pub stable_at: i64,
    /// When the app answered Claude Code's folder-trust question for its own dictation folder (0 = not yet).
    pub trust_answered_at: i64,
}

const START_TIMEOUT_MS: i64 = 20_000;
const MAX_LISTEN_MS: i64 = 10 * 60_000;
const FINISH_STABLE_MS: i64 = 800;
const FINISH_TIMEOUT_MS: i64 = 8_000;
const IDLE_KILL_MS: i64 = 5 * 60_000;

/// Status-line messages from Claude Code worth showing (errors and hints, not the usual footer).
fn is_problem(status: &str) -> bool {
    let s = status.to_lowercase();
    ["no speech", "voice", "microphone", "dictation", "error", "failed", "needs", "denied", "not available", "unavailable", "/login", "connection"]
        .iter()
        .any(|k| s.contains(k))
        && !s.contains("listening")
}

/// The hidden prompt's words minus `prefix`. Claude Code may revise the last interim words, so the
/// prefix is matched up to where the two texts start to differ.
fn new_words<'a>(prefix: &str, hidden: &'a str) -> &'a str {
    if prefix.is_empty() {
        return hidden;
    }
    let common: usize = prefix.chars().zip(hidden.chars()).take_while(|(a, b)| a == b).map(|(_, b)| b.len_utf8()).sum();
    hidden[common..].trim_start()
}

fn joined(base: &str, words: &str) -> String {
    if words.is_empty() {
        base.to_string()
    } else if base.is_empty() || base.ends_with(char::is_whitespace) {
        format!("{base}{words}")
    } else {
        format!("{base} {words}")
    }
}

impl LauncherApp {
    /// Mic / Ctrl+Space in a chat tab: start dictation, or stop it when it's running.
    pub fn chat_dictate(&mut self, tab: &str, profile: &str, cwd: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(run) = &self.dict_run {
            if run.tab == tab && run.phase == DictPhase::Listening {
                return self.dictation_release(cx);
            }
            if run.tab == tab && run.phase == DictPhase::Starting {
                return self.dictation_end(None, window, cx); // pressed again before it started: cancel
            }
            return; // another tab is dictating or finishing
        }
        let Some(claude) = self.claude_path() else {
            return self.show_error("Claude Code was not found. Install it or set its path in Settings.", window, cx);
        };
        if !self.dict_sessions.contains_key(profile) {
            let id = format!("{TAB_PREFIX}{profile}");
            let dir = profile_dir(profile).to_string_lossy().to_string();
            crate::backend::profile_settings::ensure_dictate_key(&profile_dir(profile));
            match dictation::spawn(&id, &claude, cwd, &dir, self.events.clone()) {
                Ok(term) => {
                    self.dict_sessions.insert(profile.to_string(), DictationSession { term, last_used: now_ms() });
                }
                Err(e) => return self.show_error(format!("Could not start Claude Code for dictation: {e}"), window, cx),
            }
        }
        let base = self.composer.read(cx).text(cx);
        self.dict_run = Some(DictationRun {
            tab: tab.to_string(),
            profile: profile.to_string(),
            shown: base.clone(),
            base,
            prefix: String::new(),
            held: Arc::new(AtomicBool::new(false)),
            phase: DictPhase::Starting,
            phase_at: now_ms(),
            last: PromptState::default(),
            stable_at: now_ms(),
            trust_answered_at: 0,
        });
        self.set_dictation_status(Some("Starting voice… (Claude Code)".into()), cx);

        // Poll the hidden screen until the run ends.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(120)).await;
                let go_on = this.update_in(cx, |this, window, cx| this.dictation_tick(window, cx)).unwrap_or(false);
                if !go_on {
                    break;
                }
            }
        })
        .detach();
    }

    fn set_dictation_status(&mut self, status: Option<String>, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| {
            c.dictation = status;
            cx.notify();
        });
    }

    /// Stops holding the key; the transcript is finalized and picked up by `dictation_tick`.
    fn dictation_release(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.dict_run.as_mut() {
            run.held.store(false, Ordering::Relaxed);
            run.phase = DictPhase::Finishing;
            run.phase_at = now_ms();
            run.stable_at = now_ms();
        }
        self.set_dictation_status(Some("Finishing…".into()), cx);
    }

    /// Ends the run: puts the final words (if any) into the message box, clears the hidden prompt.
    fn dictation_end(&mut self, error: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dictation_stop(cx) {
            return;
        }
        if let Some(e) = error {
            self.show_error(e, window, cx);
        }
        self.composer.update(cx, |c, cx| c.focus_input(window, cx));
        cx.notify();
    }

    /// The message was sent from `tab` while dictating there: turn the mic off and drop the hidden
    /// prompt's words, so the next dictation starts from an empty box.
    pub fn dictation_on_send(&mut self, tab: &str, cx: &mut Context<Self>) {
        if self.dict_run.as_ref().is_some_and(|r| r.tab == tab) {
            self.dictation_stop(cx);
            cx.notify();
        }
    }

    /// Stops the run (if any) and clears the hidden prompt, now and again a bit later: after the key
    /// is released Claude Code may still type the final transcript. Returns false if nothing ran.
    fn dictation_stop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(run) = self.dict_run.take() else { return false };
        run.held.store(false, Ordering::Relaxed);
        if let Some(s) = self.dict_sessions.get_mut(&run.profile) {
            s.term.write(CLEAR_PROMPT.to_vec());
            s.last_used = now_ms();
        }
        self.set_dictation_status(None, cx);
        let profile = run.profile;
        cx.spawn(async move |this, cx| {
            for _ in 0..3 {
                cx.background_executor().timer(Duration::from_millis(1000)).await;
                let _ = this.update(cx, |this, _| {
                    if this.dict_run.is_none() {
                        if let Some(s) = this.dict_sessions.get(&profile) {
                            s.term.write(CLEAR_PROMPT.to_vec());
                        }
                    }
                });
            }
        })
        .detach();
        true
    }

    /// The user edited the message box during the run: keep their text and count only words
    /// dictated from now on.
    fn dictation_rebase(&mut self, cx: &mut Context<Self>) {
        let current = self.composer.read(cx).text(cx);
        if let Some(run) = self.dict_run.as_mut() {
            if current != run.shown {
                run.base = current.clone();
                run.shown = current;
                run.prefix = run.last.text.clone();
            }
        }
    }

    /// Shows base + this run's words in the message box.
    fn dictation_show(&mut self, hidden: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.dict_run.as_mut() else { return };
        let text = joined(&run.base, new_words(&run.prefix, hidden));
        if text != run.shown {
            run.shown = text.clone();
            self.composer.update(cx, |c, cx| c.set_text(&text, window, cx));
        }
    }

    /// One poll step; returns false when the run is over.
    fn dictation_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(run) = self.dict_run.as_ref() else { return false };
        let now = now_ms();
        let Some(session) = self.dict_sessions.get(&run.profile) else {
            self.dictation_end(Some("The dictation session ended unexpectedly.".into()), window, cx);
            return false;
        };
        let state = dictation::read_prompt(&session.term);
        let phase = run.phase;
        let age = now - run.phase_at;

        match phase {
            DictPhase::Starting => {
                if state.ready {
                    // Leftover words from an earlier run: clear (Ctrl+U takes a line at a time) and
                    // look again; whatever still stays after a while is ignored as a prefix.
                    if !state.text.is_empty() && age < 2_000 {
                        session.term.write(CLEAR_PROMPT.to_vec());
                        return true;
                    }
                    let held = run.held.clone();
                    held.store(true, Ordering::Relaxed);
                    dictation::hold_key(session.term.input_sender(), held);
                    let run = self.dict_run.as_mut().unwrap();
                    run.prefix = state.text.clone();
                    run.last = state;
                    run.phase = DictPhase::Listening;
                    run.phase_at = now;
                    self.set_dictation_status(Some("Listening… press the mic or Ctrl+Space again to stop".into()), cx);
                    return true;
                }
                let screen = dictation::screen_lines(&session.term).join("\n").to_lowercase();
                if screen.contains("trust") && screen.contains("folder") {
                    // The hidden session runs in the app's own empty folder (see `dictate`), so trusting it is
                    // safe. The menu starts on "No, exit": move down until the ❯ line says "yes", then
                    // Enter (one key per tick). Still asking 4 s after that: give up.
                    if run.trust_answered_at == 0 {
                        let on_yes = screen.lines().any(|l| l.contains('❯') && l.contains("yes"));
                        if on_yes {
                            session.term.write(vec![13u8]); // Enter
                            self.dict_run.as_mut().unwrap().trust_answered_at = now;
                        } else if age < 3_000 {
                            session.term.write(b"\x1b[B".to_vec()); // Down
                        } else {
                            self.dict_run.as_mut().unwrap().trust_answered_at = now - 4_000; // no "yes" found
                        }
                        return true;
                    }
                    if now - run.trust_answered_at < 4_000 {
                        return true;
                    }
                    let profile = run.profile.clone();
                    self.dict_sessions.remove(&profile);
                    self.dictation_end(
                        Some("Claude Code keeps asking whether to trust the dictation folder. Try dictation again.".into()),
                        window,
                        cx,
                    );
                    return false;
                }
                if screen.contains("/login") || screen.contains("select login method") {
                    let profile = run.profile.clone();
                    self.dict_sessions.remove(&profile);
                    self.dictation_end(Some("This account isn't signed in to Claude Code. Sign in under Accounts first.".into()), window, cx);
                    return false;
                }
                if age > START_TIMEOUT_MS {
                    let profile = run.profile.clone();
                    self.dict_sessions.remove(&profile);
                    self.dictation_end(Some("Claude Code didn't get ready for dictation in time.".into()), window, cx);
                    return false;
                }
                true
            }
            DictPhase::Listening => {
                let status_changed = state.status != run.last.status;
                self.dictation_rebase(cx);
                self.dictation_show(&state.text, window, cx);
                if is_problem(&state.status) && status_changed {
                    let s = state.status.clone();
                    self.set_dictation_status(Some(format!("{s} (press the mic again to stop)")), cx);
                }
                let run = self.dict_run.as_mut().unwrap();
                run.last = state;
                if age > MAX_LISTEN_MS {
                    self.dictation_release(cx);
                }
                true
            }
            DictPhase::Finishing => {
                self.dictation_rebase(cx);
                let run = self.dict_run.as_mut().unwrap();
                if state.text != run.last.text || state.status.contains("listening") {
                    run.stable_at = now;
                }
                run.last = state.clone();
                self.dictation_show(&state.text, window, cx);
                let stable = now - self.dict_run.as_ref().unwrap().stable_at > FINISH_STABLE_MS;
                if (stable && age > FINISH_STABLE_MS) || age > FINISH_TIMEOUT_MS {
                    let problem = (state.text.is_empty() && is_problem(&state.status)).then(|| format!("Dictation: {}", state.status));
                    self.dictation_end(problem, window, cx);
                    return false;
                }
                true
            }
        }
    }

    /// The hidden session for an account exited (its terminal event).
    pub fn dictation_session_exited(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let profile = id.trim_start_matches(TAB_PREFIX).to_string();
        self.dict_sessions.remove(&profile);
        if self.dict_run.as_ref().is_some_and(|r| r.profile == profile) {
            self.dictation_end(Some("The dictation session (Claude Code) exited.".into()), window, cx);
        }
    }

    /// Closes hidden sessions that haven't been used for a while.
    pub fn prune_dictation_sessions(&mut self) {
        let busy = self.dict_run.as_ref().map(|r| r.profile.clone());
        let now = now_ms();
        self.dict_sessions.retain(|p, s| Some(p) == busy.as_ref() || now - s.last_used < IDLE_KILL_MS);
    }
}
