//! Pseudo-terminals running Claude Code (ConPTY on Windows) with a VT parser from
//! alacritty_terminal. The UI draws `Term`'s grid (ui/terminal_view.rs).

use super::BackendEvent;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

#[derive(Clone)]
pub struct Listener {
    tab: String,
    events: async_channel::Sender<BackendEvent>,
    sender: Arc<OnceLock<EventLoopSender>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                let _ = self.events.try_send(BackendEvent::TermWakeup { tab: self.tab.clone() });
            }
            Event::PtyWrite(s) => {
                if let Some(tx) = self.sender.get() {
                    let _ = tx.send(Msg::Input(Cow::Owned(s.into_bytes())));
                }
            }
            Event::ChildExit(status) => {
                let code = status.code().unwrap_or(-1);
                let _ = self.events.send_blocking(BackendEvent::TermExit { tab: self.tab.clone(), code });
            }
            _ => {}
        }
    }
}

pub struct Size {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

pub struct Terminal {
    pub term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    pub cols: usize,
    pub rows: usize,
}

pub struct SpawnOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// Added to the app's environment (which already has inherited Claude vars removed).
    pub env: HashMap<String, String>,
    pub cols: usize,
    pub rows: usize,
}

impl Terminal {
    pub fn spawn(tab: &str, opts: SpawnOptions, events: async_channel::Sender<BackendEvent>) -> anyhow::Result<Self> {
        let cols = opts.cols.max(20);
        let rows = opts.rows.max(5);
        let pty_opts = tty::Options {
            shell: Some(tty::Shell::new(opts.program, opts.args)),
            working_directory: Some(opts.cwd.into()),
            drain_on_exit: true,
            env: opts.env,
            #[cfg(windows)]
            escape_args: true,
        };
        let ws = WindowSize { num_lines: rows as u16, num_cols: cols as u16, cell_width: 8, cell_height: 16 };
        let pty = tty::new(&pty_opts, ws, 0)?;
        let sender_cell = Arc::new(OnceLock::new());
        let listener = Listener { tab: tab.to_string(), events, sender: sender_cell.clone() };
        let config = Config { scrolling_history: 10_000, ..Config::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &Size { cols, rows }, listener.clone())));
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)?;
        let sender = event_loop.channel();
        let _ = sender_cell.set(sender.clone());
        event_loop.spawn();
        Ok(Self { term, sender, cols, rows })
    }

    pub fn write(&self, data: impl Into<Vec<u8>>) {
        let _ = self.sender.send(Msg::Input(Cow::Owned(data.into())));
    }

    pub fn resize(&mut self, cols: usize, rows: usize, cell_w: f32, cell_h: f32) {
        if cols < 2 || rows < 2 || (cols == self.cols && rows == self.rows) {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.term.lock().resize(Size { cols, rows });
        let _ = self.sender.send(Msg::Resize(WindowSize {
            num_lines: rows as u16,
            num_cols: cols as u16,
            cell_width: cell_w as u16,
            cell_height: cell_h as u16,
        }));
    }

    pub fn kill(&self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.kill();
    }
}
