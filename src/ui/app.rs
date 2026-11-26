//! The app window's state and actions (the Electron build's main process + App.tsx in one place).

use super::chat_store::{ChatState, Tone};
use super::composer::{Composer, ComposerEvent};
use super::terminal_view::TerminalView;
use crate::backend::account_info::{refresh_profile, ProfileLive};
use crate::backend::chat::{read_transcript_messages, ChatManager, ChatStartOptions};
use crate::backend::history::project_dir_name;
use crate::backend::hook_server::HookServer;
use crate::backend::profile_settings::{apply_voice_settings, DICTATE_SEQUENCE};
use crate::backend::profiles::*;
use crate::backend::sessions::{prune_session_files, SessionTracker};
use crate::backend::store::{data_dir, Store};
use crate::backend::terminal::SpawnOptions;
use crate::backend::types::*;
use crate::backend::{hooks_config, platform, updater, BackendEvent};
use gpui_kit::component::input::InputState;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::WindowExt;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

pub const HANDOFF_PROMPT: &str = "We are handing this work over to another Claude session on a different account. \
Update docs/STATUS.md (create it if missing) so a fresh session can continue without this chat: \
what was done in this session, what is in progress (with file paths), the exact next steps, and known problems. \
Be concise and specific. Then commit only docs/STATUS.md with the message \"Handoff: update STATUS.md\" \
(skip the commit if this folder is not a git repository). Do not change anything else.";

pub const CONTINUE_PROMPT: &str = "This work was handed over from another session. Read CLAUDE.md (if present) and docs/STATUS.md, \
check git status for uncommitted work, then briefly summarize where things stand and continue with the next step.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Project(String),
    Accounts,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Project,
    Login,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: String,
    pub kind: TabKind,
    pub mode: TabMode,
    pub project_id: Option<String>,
    pub profile: String,
    pub exit_code: Option<i32>,
    pub started_at: i64,
    /// Model alias for this tab (None = account default).
    pub model: Option<String>,
}

impl Tab {
    /// Key for the group of tabs shown in a view: a project id, or "accounts" for sign-in tabs.
    pub fn group(&self) -> String {
        match self.kind {
            TabKind::Login => "accounts".into(),
            TabKind::Project => self.project_id.clone().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffMode {
    Summary,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resize {
    Sidebar,
    Panel,
}

pub struct HandoffDialog {
    pub from_tab: String,
    pub to: Option<String>,
    pub mode: HandoffMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HandoffPhase {
    WaitIdle,
    Writing,
}

#[derive(Clone)]
pub struct Handoff {
    pub from: String,
    pub to: String,
    pub project_id: String,
    pub phase: HandoffPhase,
    pub saw_working: bool,
}

pub struct McpEntry {
    pub at: i64,
    pub result: Option<Result<Vec<McpServer>, String>>,
}

pub struct OpenRequest {
    pub kind: TabKind,
    pub project_id: Option<String>,
    pub profile: String,
    pub resume_session_id: Option<String>,
    pub resume_path: Option<String>,
    pub initial_prompt: Option<String>,
    pub mode: Option<TabMode>,
}

impl OpenRequest {
    pub fn project(project_id: &str, profile: &str) -> Self {
        Self {
            kind: TabKind::Project,
            project_id: Some(project_id.into()),
            profile: profile.into(),
            resume_session_id: None,
            resume_path: None,
            initial_prompt: None,
            mode: None,
        }
    }
}

pub struct LauncherApp {
    pub store: Store,
    pub live: HashMap<String, ProfileLive>,
    pub profiles: Vec<ProfileInfo>,
    pub refreshing: bool,
    pub view: View,
    pub tabs: Vec<Tab>,
    pub active: HashMap<String, String>,
    pub unseen: HashSet<String>,
    pub tracker: SessionTracker,
    pub chat: ChatManager,
    pub chats: HashMap<String, ChatState>,
    pub chat_scroll: HashMap<String, ScrollHandle>,
    pub terms: HashMap<String, Entity<TerminalView>>,
    pub events: async_channel::Sender<BackendEvent>,
    pub hooks: Option<HookServer>,
    pub update: UpdateState,
    pub git: HashMap<String, GitInfo>,
    pub history: HashMap<String, Vec<HistorySession>>,
    pub mcp: HashMap<String, McpEntry>,
    pub collapsed: HashSet<&'static str>,
    pub composer: Entity<Composer>,
    pub profile_name: Entity<InputState>,
    pub set_claude: Entity<InputState>,
    pub set_editor: Entity<InputState>,
    pub set_language: Option<String>,
    pub set_voice: Option<VoiceMode>,
    pub settings_saved: bool,
    pub handoff_dialog: Option<HandoffDialog>,
    pub handoff: Option<Handoff>,
    pub voice_on_since: i64,
    pub perm_inputs: HashMap<u64, Entity<InputState>>,
    pub tick: u64,
    pub resizing: Option<Resize>,
}

fn is_dir(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

impl LauncherApp {
    pub fn new(
        events_tx: async_channel::Sender<BackendEvent>,
        events_rx: async_channel::Receiver<BackendEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = Store::load();
        let hooks = HookServer::start(events_tx.clone()).ok(); // without it the session panel stays empty
        prune_session_files(&list_profile_names().iter().map(|n| profile_dir(n)).collect::<Vec<_>>());
        platform::prune_pasted_images();
        ensure_all_status_lines();

        let composer = cx.new(|cx| Composer::new(window, cx));
        cx.subscribe_in(&composer, window, Self::on_composer).detach();
        let profile_name = cx.new(|cx| InputState::new(window, cx).placeholder("new profile name, e.g. work"));
        let s = store.settings.clone();
        let set_claude = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("auto-detect");
            st.set_value(s.claude_path.clone().unwrap_or_default(), window, cx);
            st
        });
        let set_editor = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("zed");
            st.set_value(s.editor_command.clone().unwrap_or_default(), window, cx);
            st
        });

        let mut app = Self {
            set_language: s.claude_language.clone(),
            set_voice: s.voice_mode,
            store,
            live: HashMap::new(),
            profiles: vec![],
            refreshing: false,
            view: View::Dashboard,
            tabs: vec![],
            active: HashMap::new(),
            unseen: HashSet::new(),
            tracker: SessionTracker::default(),
            chat: ChatManager::new(events_tx.clone()),
            chats: HashMap::new(),
            chat_scroll: HashMap::new(),
            terms: HashMap::new(),
            events: events_tx,
            hooks,
            update: UpdateState::Idle,
            git: HashMap::new(),
            history: HashMap::new(),
            mcp: HashMap::new(),
            collapsed: ["Connectors"].into_iter().collect(),
            composer,
            profile_name,
            set_claude,
            set_editor,
            settings_saved: false,
            handoff_dialog: None,
            handoff: None,
            voice_on_since: 0,
            perm_inputs: HashMap::new(),
            tick: 0,
            resizing: None,
        };
        app.apply_profile_settings(None);
        app.reload_profiles();

        // Backend events → state.
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(ev) = events_rx.recv().await {
                if this.update_in(cx, |this, window, cx| this.on_backend(ev, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        // Status-line files (model, context, cost, usage) are written by running sessions.
        cx.spawn_in(window, async move |this, cx| {
            let mut n: u64 = 0;
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                n += 1;
                let ok = this
                    .update_in(cx, |this, _window, cx| {
                        if this.tracker.poll_status() {
                            cx.notify();
                        }
                        if n % 7 == 0 {
                            this.reload_profiles();
                            if let View::Project(id) = this.view.clone() {
                                this.load_git(&id, cx);
                            }
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !ok {
                    break;
                }
            }
        })
        .detach();

        app.refresh_accounts(None, cx);
        app.check_updates(cx);
        // Account email + /usage every 5 minutes, updates every 4 hours.
        cx.spawn(async move |this, cx| {
            let mut minutes: u64 = 0;
            loop {
                cx.background_executor().timer(Duration::from_secs(300)).await;
                minutes += 5;
                let ok = this
                    .update(cx, |this, cx| {
                        this.refresh_accounts(None, cx);
                        if minutes % 240 == 0 {
                            this.check_updates(cx);
                        }
                    })
                    .is_ok();
                if !ok {
                    break;
                }
            }
        })
        .detach();
        app
    }

    // ---------- helpers ----------

    pub fn claude_path(&self) -> Option<String> {
        if let Some(p) = self.store.settings.claude_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            return std::path::Path::new(p).exists().then(|| p.to_string());
        }
        let native = dirs::home_dir().unwrap_or_default().join(".local").join("bin").join("claude.exe");
        if native.exists() {
            return Some(native.to_string_lossy().to_string());
        }
        let path = std::env::var("PATH").unwrap_or_default();
        for dir in path.split(';').filter(|d| !d.is_empty()) {
            for ext in [".exe", ".cmd"] {
                let cand = PathBuf::from(dir).join(format!("claude{ext}"));
                if cand.exists() {
                    return Some(cand.to_string_lossy().to_string());
                }
            }
        }
        None
    }

    pub fn reload_profiles(&mut self) {
        self.profiles = list_profiles(&self.live);
    }

    /// Projects, most recently used first.
    pub fn projects_sorted(&self) -> Vec<Project> {
        let mut p = self.store.projects.clone();
        p.sort_by_key(|x| -(x.last_opened_at.unwrap_or(x.added_at)));
        p
    }

    pub fn project(&self, id: &str) -> Option<&Project> {
        self.store.projects.iter().find(|p| p.id == id)
    }

    pub fn profile(&self, name: &str) -> Option<&ProfileInfo> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Env for any claude process the app runs for a profile.
    fn claude_env(&self, profile: &str, tab: &str) -> Vec<(String, String)> {
        let mut env = profile_env(profile);
        env.push(("CCL_TAB_ID".into(), tab.into()));
        env.push(("CCL_HOOK_TOKEN".into(), self.hooks.as_ref().map(|h| h.token.clone()).unwrap_or_default()));
        env
    }

    /// Writes the app's language/voice choices and hook wiring into one profile (or all).
    fn apply_profile_settings(&self, only: Option<&str>) {
        let names = only.map(|n| vec![n.to_string()]).unwrap_or_else(list_profile_names);
        let port = self.hooks.as_ref().map(|h| h.port).unwrap_or(0);
        for n in names {
            apply_voice_settings(&profile_dir(&n), &self.store.settings);
            hooks_config::ensure_hooks(&profile_dir(&n), port);
        }
    }

    pub fn current_group(&self) -> Option<String> {
        match &self.view {
            View::Project(id) => Some(id.clone()),
            View::Accounts => Some("accounts".into()),
            _ => None,
        }
    }

    /// The selected, still-running tab of the current view.
    pub fn active_tab(&self) -> Option<&Tab> {
        let g = self.current_group()?;
        let id = self.active.get(&g)?;
        self.tabs.iter().find(|t| &t.id == id && t.exit_code.is_none())
    }

    pub fn tab_signal(&self, t: &Tab) -> Option<super::theme::Signal> {
        if t.exit_code.is_some() {
            return None;
        }
        super::theme::signal_of(self.tracker.get(&t.id).map(|s| s.activity), self.unseen.contains(&t.id))
    }

    pub fn show_error(&self, msg: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        window.push_notification(Notification::error(msg), cx);
    }

    pub fn show_info(&self, title: impl Into<SharedString>, msg: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        window.push_notification(Notification::info(msg).title(title), cx);
    }

    pub fn show_success(&self, title: impl Into<SharedString>, msg: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
        window.push_notification(Notification::success(msg).title(title), cx);
    }

    // ---------- background data ----------

    pub fn refresh_accounts(&mut self, only: Option<String>, cx: &mut Context<Self>) {
        if self.refreshing && only.is_none() {
            return;
        }
        let Some(claude) = self.claude_path() else { return };
        self.refreshing = true;
        let names = only.clone().map(|n| vec![n]).unwrap_or_else(list_profile_names);
        let jobs: Vec<(String, Vec<(String, String)>, Option<ProfileLive>)> =
            names.into_iter().map(|n| (n.clone(), self.claude_env(&n, ""), self.live.get(&n).cloned())).collect();
        let events = self.events.clone();
        let work = data_dir().join("usage-probe");
        cx.background_executor()
            .spawn(async move {
                for (name, env, prev) in jobs {
                    let live = refresh_profile(&claude, &env, &work.join(&name), prev);
                    let _ = events.send(BackendEvent::AccountRefreshed { profile: name, live }).await;
                }
                let _ = events.send(BackendEvent::AccountsRefreshDone).await;
            })
            .detach();
        cx.notify();
    }

    pub fn check_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update, UpdateState::Downloading { .. } | UpdateState::Ready { .. } | UpdateState::Checking) {
            return;
        }
        let events = self.events.clone();
        std::thread::spawn(move || updater::check(&events));
        cx.notify();
    }

    pub fn load_git(&mut self, project_id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.project(project_id).cloned() else { return };
        if !is_dir(&p.path) {
            return;
        }
        let id = p.id.clone();
        let task = cx.background_executor().spawn(async move { crate::backend::git::get_git_info(&p.path) });
        cx.spawn(async move |this, cx| {
            let info = task.await;
            let _ = this.update(cx, |this, cx| {
                this.git.insert(id, info);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn load_history(&mut self, project_id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.project(project_id).cloned() else { return };
        let id = p.id.clone();
        let profiles: Vec<(String, PathBuf)> = list_profile_names().into_iter().map(|n| (n.clone(), profile_dir(&n))).collect();
        let task = cx.background_executor().spawn(async move { crate::backend::history::list_history(&p.path, &profiles, 40) });
        cx.spawn(async move |this, cx| {
            let items = task.await;
            let _ = this.update(cx, |this, cx| {
                this.history.insert(id, items);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn load_mcp(&mut self, profile: &str, project_id: &str, force: bool, cx: &mut Context<Self>) {
        let Some(p) = self.project(project_id).cloned() else { return };
        let key = format!("{profile}|{}", p.path);
        if !force {
            if let Some(e) = self.mcp.get(&key) {
                if e.result.is_none() || now_ms() - e.at < 5 * 60_000 {
                    return;
                }
            }
        }
        let (Some(claude), true) = (self.claude_path(), is_dir(&p.path) && profile_exists(profile)) else { return };
        let env = self.claude_env(profile, "");
        self.mcp.insert(key.clone(), McpEntry { at: now_ms(), result: None });
        let task = cx.background_executor().spawn(async move { crate::backend::mcp::list_mcp(&claude, &p.path, &env) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.mcp.insert(key, McpEntry { at: now_ms(), result: Some(r) });
                cx.notify();
            });
        })
        .detach();
    }

    // ---------- backend events ----------

    fn on_backend(&mut self, ev: BackendEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            BackendEvent::Hook { tab, event, reply } => {
                // Chat tabs answer permission prompts in the app; the hook waits for your click.
                if let Some(reply) = reply {
                    if self.chat.has(&tab) {
                        if let Some(req) = self.chat.request_permission(&tab, &event, reply) {
                            self.chats.entry(tab.clone()).or_default().add_permission(req);
                        }
                    }
                }
                if let Some((before, after)) = self.tracker.handle(&tab, &event) {
                    self.on_activity(&tab, before, after, window, cx);
                }
            }
            BackendEvent::ChatEvent { tab, generation, event } => {
                if self.chat.note_event(&tab, generation, &event) {
                    let before = self.tracker.get(&tab).map(|s| s.activity);
                    self.tracker.chat_event(&tab, &event);
                    self.chats.entry(tab.clone()).or_default().apply(&event);
                    if event.get("type").and_then(|t| t.as_str()) == Some("result") {
                        // A finished turn without a Stop hook (e.g. an error) still ends "working".
                        if let Some(b) = before {
                            let after = self.tracker.get(&tab).map(|s| s.activity).unwrap_or(b);
                            self.on_activity(&tab, b, after, window, cx);
                        }
                    }
                }
            }
            BackendEvent::ChatExit { tab, generation, stderr } => {
                if self.chat.process_exited(&tab, generation) {
                    let before = self.tracker.get(&tab).map(|s| s.activity);
                    self.tracker.chat_process_ended(&tab);
                    // Exit code isn't known here; an error shows up through stderr.
                    let code = if stderr.trim().is_empty() { None } else { Some(1) };
                    if let Some(c) = self.chats.get_mut(&tab) {
                        c.exited(false, code, &stderr);
                    }
                    if let (Some(b), Some(s)) = (before, self.tracker.get(&tab).map(|s| s.activity)) {
                        self.on_activity(&tab, b, s, window, cx);
                    }
                }
            }
            BackendEvent::TermWakeup { tab } => {
                if let Some(t) = self.terms.get(&tab) {
                    t.update(cx, |_, cx| cx.notify());
                }
                return;
            }
            BackendEvent::TermExit { tab, code } => {
                self.tracker.exited(&tab);
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                    t.exit_code = Some(code);
                    // A session ended: login state or usage may have changed for that account.
                    let profile = t.profile.clone();
                    self.refresh_accounts(Some(profile), cx);
                }
                if let Some(v) = self.terms.get(&tab) {
                    v.update(cx, |v, cx| {
                        v.exit_code = Some(code);
                        cx.notify();
                    });
                }
                self.reload_profiles();
            }
            BackendEvent::AccountRefreshed { profile, live } => {
                self.live.insert(profile, live);
                self.reload_profiles();
            }
            BackendEvent::AccountsRefreshDone => {
                self.refreshing = false;
                let names = list_profile_names();
                self.live.retain(|k, _| names.contains(k));
                self.reload_profiles();
            }
            BackendEvent::Update(s) => {
                if let UpdateState::Ready { version } = &s {
                    if !matches!(self.update, UpdateState::Ready { .. }) {
                        self.show_success(
                            format!("Version {version} is ready"),
                            "Restart from the banner at the top (or Settings) to install.",
                            window,
                            cx,
                        );
                    }
                }
                self.update = s;
            }
            BackendEvent::NotificationClicked { tab } => {
                window.activate_window();
                if let Some(t) = self.tabs.iter().find(|t| t.id == tab).cloned() {
                    self.view = if t.kind == TabKind::Login { View::Accounts } else { View::Project(t.project_id.clone().unwrap_or_default()) };
                    self.select_tab(&t.id, window, cx);
                }
            }
            BackendEvent::Activate => window.activate_window(),
        }
        cx.notify();
    }

    fn is_tab_visible(&self, tab: &str, window: &Window) -> bool {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab) else { return false };
        window.is_window_active() && self.current_group().as_deref() == Some(t.group().as_str()) && self.active.get(&t.group()).map(|s| s.as_str()) == Some(tab)
    }

    fn on_activity(&mut self, tab: &str, before: Activity, after: Activity, window: &mut Window, cx: &mut Context<Self>) {
        if before != after {
            if matches!(after, Activity::NeedsYou | Activity::Done | Activity::Error) {
                self.attention(tab, window);
            }
            if matches!(after, Activity::Done | Activity::Ended) {
                self.tick += 1;
                if let View::Project(id) = self.view.clone() {
                    self.load_git(&id, cx);
                    self.load_history(&id, cx);
                }
            }
            if after == Activity::Working {
                self.unseen.remove(tab);
            }
        }
        self.drive_handoff(window, cx);
    }

    /// Marks a tab as unseen and notifies, unless you're looking at it right now.
    fn attention(&mut self, tab: &str, window: &Window) {
        if self.is_tab_visible(tab, window) {
            return;
        }
        let Some(t) = self.tabs.iter().find(|t| t.id == tab) else { return };
        let Some(info) = self.tracker.get(tab) else { return };
        self.unseen.insert(tab.into());
        let where_ = t.project_id.as_deref().and_then(|id| self.project(id)).map(|p| p.name.clone()).unwrap_or_else(|| "sign-in".into());
        let title = match info.activity {
            Activity::NeedsYou => format!("Claude needs you · {where_}"),
            Activity::Error => format!("Claude stopped with an error · {where_}"),
            _ => format!("Claude finished · {where_}"),
        };
        let body = match info.activity {
            Activity::Error => info.error.clone().unwrap_or_else(|| "The request failed.".into()),
            Activity::NeedsYou => info.message.clone().unwrap_or_else(|| "Waiting for your permission or answer.".into()),
            _ => info.message.clone().unwrap_or_else(|| "Done.".into()),
        };
        platform::notify(&format!("{title} ({})", t.profile), &body, tab, self.events.clone());
    }

    // ---------- tabs ----------

    pub fn select_tab(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab).cloned() else { return };
        self.active.insert(t.group(), t.id.clone());
        self.unseen.remove(tab);
        self.sync_composer(window, cx);
        if let Some(v) = self.terms.get(tab) {
            { let fh = v.read(cx).focus_handle(); window.focus(&fh, cx); }
        }
        cx.notify();
    }

    pub fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if let View::Project(id) = &view {
            let id = id.clone();
            self.load_git(&id, cx);
            self.load_history(&id, cx);
        }
        self.view = view;
        if let Some(t) = self.active_tab().map(|t| t.id.clone()) {
            self.unseen.remove(&t);
        }
        self.sync_composer(window, cx);
        cx.notify();
    }

    /// Points the message box at the active tab (each tab keeps its own draft).
    pub fn sync_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.active_tab().filter(|t| t.kind == TabKind::Project).map(|t| t.id.clone());
        self.composer.update(cx, |c, cx| c.set_tab(tab, window, cx));
    }

    fn add_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        let id = tab.id.clone();
        self.active.insert(tab.group(), id.clone());
        self.tabs.push(tab);
        self.sync_composer(window, cx);
        cx.notify();
    }

    fn touch_project(&mut self, project_id: &str, profile: &str) {
        if let Some(p) = self.store.projects.iter_mut().find(|p| p.id == project_id) {
            p.last_profile = Some(profile.into());
            p.last_opened_at = Some(now_ms());
        }
        self.store.save();
    }

    fn resume_path_ok(p: &str) -> bool {
        let root = profiles_root().to_string_lossy().to_lowercase();
        p.to_lowercase().starts_with(&format!("{root}\\")) && p.ends_with(".jsonl") && std::path::Path::new(p).exists()
    }

    /// Opens Claude Code: a chat tab (the app's own view) or a terminal tab.
    pub fn open_tab(&mut self, req: OpenRequest, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self.try_open_tab(req, window, cx) {
            self.show_error(e, window, cx);
        }
    }

    fn try_open_tab(&mut self, req: OpenRequest, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        let claude = self.claude_path().ok_or("Claude Code was not found. Install it or set its path in Settings.")?;
        if !profile_exists(&req.profile) {
            return Err(format!("Profile \"{}\" does not exist.", req.profile));
        }
        let mode = if req.kind == TabKind::Login { TabMode::Terminal } else { req.mode.unwrap_or(self.store.settings.tab_mode.unwrap_or_default()) };
        let (cwd, project_path) = match req.kind {
            TabKind::Project => {
                let p = req.project_id.as_deref().and_then(|id| self.project(id)).ok_or("Project not found.")?;
                if !is_dir(&p.path) {
                    return Err(format!("Folder not found: {}", p.path));
                }
                (p.path.clone(), Some(p.path.clone()))
            }
            TabKind::Login => (profile_dir(&req.profile).to_string_lossy().to_string(), None),
        };
        create_profile(&req.profile).map_err(|e| e.to_string())?; // also (re)installs the usage status line
        self.apply_profile_settings(Some(&req.profile));
        let tab_id = uuid::Uuid::new_v4().to_string();
        if let Some(p) = &req.resume_path {
            if !Self::resume_path_ok(p) {
                return Err("Conversation file not found.".into());
            }
        }
        let resume_id = req.resume_session_id.clone().filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));

        if mode == TabMode::Chat {
            let (resume, transcript) = if let Some(p) = &req.resume_path {
                (Some(p.clone()), Some(p.clone()))
            } else if let Some(id) = &resume_id {
                let t = profile_dir(&req.profile)
                    .join("projects")
                    .join(project_dir_name(project_path.as_deref().unwrap_or("")))
                    .join(format!("{id}.jsonl"));
                (Some(id.clone()), Some(t.to_string_lossy().to_string()))
            } else {
                (None, None)
            };
            self.tracker.open(&tab_id, profile_dir(&req.profile));
            self.chat.open(
                &tab_id,
                ChatStartOptions { claude, cwd, env: self.claude_env(&req.profile, &tab_id), resume, permission_mode: None, model: self.store.settings.model.clone() },
            );
            let mut state = ChatState::default();
            if let Some(t) = transcript {
                state.load_history(&read_transcript_messages(&t, 400));
            }
            self.chats.insert(tab_id.clone(), state);
            self.chat_scroll.insert(tab_id.clone(), ScrollHandle::new());
        } else {
            let mut args = vec![];
            if let Some(m) = &self.store.settings.model {
                args.push("--model".to_string());
                args.push(m.clone());
            }
            if let Some(p) = &req.resume_path {
                args.push("--resume".to_string());
                args.push(p.clone());
            } else if let Some(id) = &resume_id {
                args.push("--resume".into());
                args.push(id.clone());
            }
            if let Some(prompt) = req.initial_prompt.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
                args.push(prompt.chars().take(4000).collect());
            }
            let mut env = HashMap::new();
            env.insert("CLAUDE_CONFIG_DIR".to_string(), profile_dir(&req.profile).to_string_lossy().to_string());
            env.insert("COLORTERM".into(), "truecolor".into());
            env.insert("TERM".into(), "xterm-256color".into());
            env.insert("CCL_TAB_ID".into(), tab_id.clone());
            env.insert("CCL_HOOK_TOKEN".into(), self.hooks.as_ref().map(|h| h.token.clone()).unwrap_or_default());
            self.tracker.open(&tab_id, profile_dir(&req.profile));
            let events = self.events.clone();
            let opts = SpawnOptions { program: claude, args, cwd, env, cols: 120, rows: 32 };
            let term = match crate::backend::terminal::Terminal::spawn(&tab_id, opts, events) {
                Ok(t) => t,
                Err(e) => {
                    self.tracker.close(&tab_id);
                    return Err(format!("Could not start Claude Code: {e}"));
                }
            };
            let id2 = tab_id.clone();
            let view = cx.new(|cx| TerminalView::new(&id2, term, cx));
            { let fh = view.read(cx).focus_handle(); window.focus(&fh, cx); }
            self.terms.insert(tab_id.clone(), view);
        }
        if let Some(pid) = &req.project_id {
            self.touch_project(pid, &req.profile);
        }
        self.add_tab(
            Tab { id: tab_id.clone(), kind: req.kind, mode, project_id: req.project_id.clone(), profile: req.profile.clone(), exit_code: None, started_at: now_ms(), model: self.store.settings.model.clone() },
            window,
            cx,
        );
        if mode == TabMode::Chat {
            if let Some(prompt) = req.initial_prompt {
                self.chat_send(&tab_id, &prompt, &[], window, cx);
            }
        }
        Ok(())
    }

    /// Sends a message in a chat tab (shows it right away; Claude's reply streams in).
    pub fn chat_send(&mut self, tab: &str, text: &str, images: &[String], window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.chats.entry(tab.into()).or_default().add_user(text, images);
        if let Some(h) = self.chat_scroll.get(tab) {
            h.scroll_to_bottom();
        }
        if !self.chat.has(tab) {
            let c = self.chats.entry(tab.into()).or_default();
            c.busy = false;
            c.notice("This chat is closed.".into(), Tone::Error);
            return false;
        }
        self.tracker.chat_working(tab);
        if let Err(e) = self.chat.send_message(tab, text, images) {
            let c = self.chats.entry(tab.into()).or_default();
            c.busy = false;
            c.notice(e, Tone::Error);
            cx.notify();
            return false;
        }
        let _ = window;
        cx.notify();
        true
    }

    /// The chat process was ended to apply a setting: a running turn is over (the next message resumes).
    fn end_chat_turn(&mut self, tab: &str) {
        if self.chats.get(tab).map(|c| c.busy).unwrap_or(false) {
            self.tracker.chat_process_ended(tab);
            if let Some(c) = self.chats.get_mut(tab) {
                c.exited(true, Some(0), "");
            }
        }
    }

    pub fn chat_stop(&mut self, tab: &str, cx: &mut Context<Self>) {
        if self.chat.stop(tab) {
            self.tracker.chat_process_ended(tab);
            if let Some(c) = self.chats.get_mut(tab) {
                c.exited(true, Some(0), "");
            }
        }
        cx.notify();
    }

    fn really_close_tab(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab).cloned() else { return };
        if t.mode == TabMode::Chat {
            self.chat.close(tab);
            self.chats.remove(tab);
            self.chat_scroll.remove(tab);
        } else if let Some(v) = self.terms.remove(tab) {
            v.read(cx).kill();
        }
        self.tracker.close(tab);
        self.unseen.remove(tab);
        self.tabs.retain(|x| x.id != tab);
        let group = t.group();
        if self.active.get(&group).map(|s| s.as_str()) == Some(tab) {
            let next = self.tabs.iter().filter(|x| x.group() == group).last().map(|x| x.id.clone());
            match next {
                Some(n) => {
                    self.active.insert(group, n);
                }
                None => {
                    self.active.remove(&group);
                }
            }
        }
        self.sync_composer(window, cx);
        cx.notify();
    }

    pub fn close_tab(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab).cloned() else { return };
        let busy = if t.mode == TabMode::Chat {
            self.tracker.get(tab).map(|s| s.activity == Activity::Working).unwrap_or(false)
        } else {
            t.exit_code.is_none()
        };
        if !busy {
            return self.really_close_tab(tab, window, cx);
        }
        let detail = format!(
            "Claude Code ({}) is still {} in this tab. Closing it stops it.",
            t.profile,
            if t.mode == TabMode::Chat { "working" } else { "running" }
        );
        let answer = window.prompt(PromptLevel::Warning, "Close this session?", Some(&detail), &["Close tab", "Cancel"], cx);
        let tab = tab.to_string();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update_in(cx, |this, window, cx| this.really_close_tab(&tab, window, cx));
            }
        })
        .detach();
    }

    // ---------- projects & profiles ----------

    pub fn add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Add project".into()) });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let path = path.to_string_lossy().to_string();
            let _ = this.update_in(cx, |this, window, cx| {
                let existing = this.store.projects.iter().find(|p| p.path.eq_ignore_ascii_case(&path)).map(|p| p.id.clone());
                let id = match existing {
                    Some(id) => id,
                    None => {
                        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(path.clone());
                        let p = Project { id: uuid::Uuid::new_v4().to_string(), name, path: path.clone(), added_at: now_ms(), last_opened_at: None, last_profile: None };
                        let id = p.id.clone();
                        this.store.projects.push(p);
                        this.store.save();
                        id
                    }
                };
                this.set_view(View::Project(id), window, cx);
            });
        })
        .detach();
    }

    pub fn remove_project(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.project(id).cloned() else { return };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Remove \"{}\"?", p.name),
            Some("It is only removed from this list. Files on disk stay untouched. Open sessions for it are closed."),
            &["Remove", "Cancel"],
            cx,
        );
        let id = id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                for t in this.tabs.iter().filter(|t| t.project_id.as_deref() == Some(&id)).map(|t| t.id.clone()).collect::<Vec<_>>() {
                    this.really_close_tab(&t, window, cx);
                }
                this.store.projects.retain(|p| p.id != id);
                this.store.save();
                this.set_view(View::Dashboard, window, cx);
            });
        })
        .detach();
    }

    pub fn add_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.profile_name.read(cx).value().trim().to_lowercase();
        if name.is_empty() {
            return;
        }
        if let Some(e) = validate_profile_name(&name) {
            return self.show_error(e, window, cx);
        }
        if profile_exists(&name) {
            return self.show_error(format!("Profile \"{name}\" already exists."), window, cx);
        }
        if let Err(e) = create_profile(&name) {
            return self.show_error(e.to_string(), window, cx);
        }
        self.apply_profile_settings(Some(&name));
        self.profile_name.update(cx, |s, cx| s.set_value("", window, cx));
        self.reload_profiles();
        self.refresh_accounts(Some(name), cx);
        cx.notify();
    }

    pub fn remove_profile(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if validate_profile_name(name).is_some() {
            return;
        }
        let dir = profile_dir(name).to_string_lossy().to_string();
        let detail = format!(
            "The folder {dir} will be moved to the Recycle Bin. This signs that account out on this PC and removes its local session history. You can restore it from the Recycle Bin."
        );
        let answer = window.prompt(PromptLevel::Warning, &format!("Remove profile \"{name}\"?"), Some(&detail), &["Move to Recycle Bin", "Cancel"], cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                if let Err(e) = platform::move_to_trash(&dir) {
                    this.show_error(e, window, cx);
                }
                this.reload_profiles();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let claude = self.set_claude.read(cx).value().trim().to_string();
        let editor = self.set_editor.read(cx).value().trim().to_string();
        let was_tap = self.store.settings.voice_mode == Some(VoiceMode::Tap);
        let s = &mut self.store.settings;
        s.claude_path = (!claude.is_empty()).then_some(claude);
        s.editor_command = (!editor.is_empty()).then_some(editor);
        s.claude_language = self.set_language.clone();
        s.voice_mode = self.set_voice;
        if !was_tap && s.voice_mode == Some(VoiceMode::Tap) {
            self.voice_on_since = now_ms();
        }
        self.store.save();
        self.apply_profile_settings(None);
        self.settings_saved = true;
        let _ = window;
        cx.notify();
    }

    pub fn set_tab_mode(&mut self, mode: TabMode, cx: &mut Context<Self>) {
        self.store.settings.tab_mode = Some(mode);
        self.store.save();
        cx.notify();
    }

    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        let open = self.store.settings.panel_open.unwrap_or(true);
        self.store.settings.panel_open = Some(!open);
        self.store.save();
        cx.notify();
    }

    pub fn open_folder(&self, path: &str, window: &mut Window, cx: &mut App) {
        if let Err(e) = crate::backend::util::open_path(path) {
            self.show_error(e, window, cx);
        }
    }

    pub fn open_in_editor(&self, path: &str, window: &mut Window, cx: &mut App) {
        let editor = self.store.settings.editor_command.clone().unwrap_or_else(|| "zed".into());
        if let Err(e) = crate::backend::util::open_in_editor(&editor, path) {
            self.show_error(e, window, cx);
        }
    }

    pub fn install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match updater::install() {
            Ok(()) => {
                self.shutdown(cx);
                cx.quit();
            }
            Err(e) => self.show_error(format!("Update failed: {e}"), window, cx),
        }
    }

    pub fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.chat.close_all();
        for v in self.terms.values() {
            v.read(cx).kill();
        }
    }

    // ---------- voice dictation ----------

    pub fn dictate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab().cloned() else { return };
        if tab.mode == TabMode::Chat {
            return self.show_info(
                "Voice dictation works in terminal tabs",
                "Claude Code's dictation only exists in its terminal UI. Open this account as a Terminal tab to dictate.",
                window,
                cx,
            );
        }
        if self.store.settings.voice_mode != Some(VoiceMode::Tap) {
            let lang = if self.store.settings.claude_language.is_some() { "" } else { " Also set Claude language, or dictation expects English." };
            return self.show_info(
                "Voice dictation is off",
                format!("Turn it on for all accounts in Settings → Voice dictation (tap).{lang}"),
                window,
                cx,
            );
        }
        if tab.started_at < self.voice_on_since {
            return self.show_info(
                "This session started before voice was turned on",
                "Dictation works in sessions started after that. Open a new tab for this account.",
                window,
                cx,
            );
        }
        if let Some(v) = self.terms.get(&tab.id) {
            v.read(cx).write(DICTATE_SEQUENCE.to_vec());
            { let fh = v.read(cx).focus_handle(); window.focus(&fh, cx); }
        }
    }

    // ---------- composer ----------

    fn on_composer(&mut self, _c: &Entity<Composer>, ev: &ComposerEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab().cloned() else { return };
        match ev {
            ComposerEvent::SetModel(model) => {
                // Remembered for new tabs too.
                self.store.settings.model = model.clone();
                self.store.save();
                if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab.id) {
                    t.model = model.clone();
                }
                if tab.mode == TabMode::Chat {
                    // The next message restarts claude with --model and resumes the session.
                    self.chat.set_model(&tab.id, model.clone());
                    self.end_chat_turn(&tab.id);
                } else if let Some(v) = self.terms.get(&tab.id) {
                    let cmd = format!("/model {}", model.as_deref().unwrap_or("default"));
                    v.read(cx).paste_text(&cmd);
                    let v = v.clone();
                    cx.spawn(async move |_, cx| {
                        cx.background_executor().timer(Duration::from_millis(150)).await;
                        let _ = v.update(cx, |v, _| v.write(b"\r".to_vec()));
                    })
                    .detach();
                }
                cx.notify();
            }
            ComposerEvent::SetMode(mode) => {
                self.chat.set_permission_mode(&tab.id, mode.clone());
                self.end_chat_turn(&tab.id);
                if let Some(c) = self.chats.get_mut(&tab.id) {
                    c.permission_mode = mode.clone();
                }
                cx.notify();
            }
            ComposerEvent::Send { text, files } => {
                let images: Vec<String> = files.iter().filter(|f| super::composer::is_image(f)).cloned().collect();
                let others: Vec<String> = files.iter().filter(|f| !super::composer::is_image(f)).cloned().collect();
                let ok = if tab.mode == TabMode::Chat {
                    // Images go as image blocks; other files by path, Claude reads them itself.
                    let mut msg = text.clone();
                    if !others.is_empty() {
                        msg.push_str("\n\nAttached files:\n");
                        for f in &others {
                            msg.push_str(&format!("- {f}\n"));
                        }
                    }
                    self.chat_send(&tab.id, msg.trim(), &images, window, cx)
                } else if let Some(v) = self.terms.get(&tab.id).cloned() {
                    // Terminal: files first, one paste each, so Claude Code turns every image path into an attachment.
                    let text = text.clone();
                    let images = files.clone();
                    cx.spawn_in(window, async move |_this, cx| {
                        for p in &images {
                            let _ = v.update(cx, |v, _| v.paste_text(&format!("\"{p}\" ")));
                            cx.background_executor().timer(Duration::from_millis(120)).await;
                        }
                        if !text.is_empty() {
                            let _ = v.update(cx, |v, _| v.paste_text(&text));
                        }
                        cx.background_executor().timer(Duration::from_millis(150)).await;
                        let _ = v.update(cx, |v, _| v.write(b"\r".to_vec()));
                    })
                    .detach();
                    true
                } else {
                    false
                };
                if ok {
                    self.composer.update(cx, |c, cx| c.sent(text, window, cx));
                }
            }
            ComposerEvent::Stop => self.chat_stop(&tab.id, cx),
            ComposerEvent::Dictate => self.dictate(window, cx),
            ComposerEvent::FocusTerminal => {
                if let Some(v) = self.terms.get(&tab.id) {
                    { let fh = v.read(cx).focus_handle(); window.focus(&fh, cx); }
                }
            }
        }
    }

    // ---------- hand off to another account ----------

    pub fn send_to_tab(&mut self, tab: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat.has(tab) {
            self.chat_send(tab, text, &[], window, cx);
            return;
        }
        if let Some(v) = self.terms.get(tab).cloned() {
            v.read(cx).paste_text(text);
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                let _ = v.update(cx, |v, _| v.write(b"\r".to_vec()));
            })
            .detach();
        }
    }

    pub fn start_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.handoff_dialog.take() else { return };
        let Some(to) = d.to else { return };
        let Some(from) = self.tabs.iter().find(|t| t.id == d.from_tab).cloned() else { return };
        let s = self.tracker.get(&from.id).cloned();
        let project_id = from.project_id.clone().unwrap_or_default();
        if d.mode == HandoffMode::Full {
            let Some(path) = s.and_then(|s| s.transcript_path) else {
                return self.show_error("This session has no saved conversation yet.", window, cx);
            };
            let mut req = OpenRequest::project(&project_id, &to);
            req.resume_path = Some(path);
            self.open_tab(req, window, cx);
            self.show_success(format!("Continuing the conversation on {to}"), "Close the old tab so both accounts don't work on the same thing.", window, cx);
            return;
        }
        let busy = s.map(|s| s.activity == Activity::Working).unwrap_or(false);
        if !busy {
            self.send_to_tab(&from.id, HANDOFF_PROMPT, window, cx);
        }
        self.handoff = Some(Handoff { from: from.id.clone(), to: to.clone(), project_id, phase: if busy { HandoffPhase::WaitIdle } else { HandoffPhase::Writing }, saw_working: false });
        self.show_info(
            if busy { format!("Waiting for Claude ({}) to finish…", from.profile) } else { format!("Claude ({}) is updating docs/STATUS.md…", from.profile) },
            format!("Then a new {to} session continues from it. Progress shows in the session panel."),
            window,
            cx,
        );
        cx.notify();
    }

    pub fn finish_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.handoff.take() else { return };
        let mut req = OpenRequest::project(&h.project_id, &h.to);
        req.initial_prompt = Some(CONTINUE_PROMPT.into());
        self.open_tab(req, window, cx);
        self.show_success(format!("Handed off to {}", h.to), "It reads docs/STATUS.md and continues. You can close the old tab.", window, cx);
    }

    /// Drives the summary handoff from the old tab's activity.
    fn drive_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.handoff.clone() else { return };
        let a = self.tracker.get(&h.from).map(|s| s.activity);
        match (h.phase, a) {
            (HandoffPhase::WaitIdle, Some(Activity::Done | Activity::Idle)) => {
                self.send_to_tab(&h.from, HANDOFF_PROMPT, window, cx);
                self.handoff = Some(Handoff { phase: HandoffPhase::Writing, saw_working: false, ..h });
            }
            (HandoffPhase::Writing, Some(Activity::Working)) if !h.saw_working => {
                self.handoff = Some(Handoff { saw_working: true, ..h });
            }
            (HandoffPhase::Writing, Some(Activity::Done)) if h.saw_working => self.finish_handoff(window, cx),
            (_, Some(Activity::Error | Activity::Ended)) | (_, None) => {
                self.handoff = None;
                self.show_error("Handoff stopped: the old session failed or closed.", window, cx);
            }
            _ => {}
        }
    }
}
