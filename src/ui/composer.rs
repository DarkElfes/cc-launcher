//! The message box under Claude tabs, laid out like the Claude app: one box with the text on top and
//! a row of controls below (attach, voice, permission mode, model, send/stop).
//! Chat tabs send through the app; terminal tabs paste into Claude Code's prompt and press Enter.

use super::icons::icon;
use super::theme::{self, *};
use crate::backend::platform;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::*;
use gpui_kit::prelude::*;
use std::collections::HashMap;

pub enum ComposerEvent {
    /// `files`: attached files (images become image blocks in chat; other files are passed as paths).
    Send { text: String, files: Vec<String> },
    Stop,
    Dictate,
    FocusTerminal,
    SetModel(Option<String>),
    SetMode(Option<String>),
}

impl EventEmitter<ComposerEvent> for Composer {}

/// Models offered in the picker: (alias passed to `--model` / `/model`, label). "" = account default.
pub const MODELS: &[(&str, &str)] = &[("", "Default model"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];

/// Permission modes for chat tabs: (value for `--permission-mode`, label, hint). Auto is the default.
pub const MODES: &[(&str, &str, &str)] = &[
    ("default", "Ask permissions", "Ask before edits and commands"),
    ("acceptEdits", "Accept edits", "Edit files without asking"),
    ("plan", "Plan mode", "Plan first, no changes until you approve"),
    ("auto", "Auto mode", "Decide on its own what needs asking"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Model,
    Mode,
}

pub struct Composer {
    pub tab: Option<String>,
    input: Entity<TextareaState>,
    files: Vec<String>,
    drafts: HashMap<String, (String, Vec<String>)>,
    history: HashMap<String, Vec<String>>,
    hist_index: Option<usize>,
    menu: Option<Menu>,
    /// The menu a click outside just closed: that same click on its button must not reopen it.
    menu_closed: Option<(Menu, std::time::Instant)>,
    show_help: bool,
    // Set by the app each frame.
    pub busy: bool,
    pub waiting: bool,
    pub is_chat: bool,
    pub voice_on: bool,
    /// Chat-tab dictation in progress: its status line ("Listening…", an error, …).
    pub dictation: Option<String>,
    pub model: Option<String>,
    /// Permission mode picked for the tab (None = auto).
    pub mode: Option<String>,
    /// The mode Claude Code reports it is in (init / status events).
    pub actual_mode: Option<String>,
}

const HELP: &[(&str, &str)] = &[
    ("Enter", "Send to Claude"),
    ("Shift+Enter", "New line"),
    ("Ctrl+↑ / Ctrl+↓", "Previous messages"),
    ("Ctrl+V", "Paste text, or a screenshot as an image"),
    ("+ / drag & drop", "Attach files (images are shown to Claude, other files by path)"),
    ("Mic / Ctrl+Space", "Voice dictation with Claude Code (press again to stop). Chat tabs: the words appear in this box; terminal tabs: in Claude Code's prompt."),
    ("Esc", "Go to the terminal"),
    ("Terminal", "Answer Claude's questions and menus there. Right click copies/pastes."),
];

pub fn is_image(p: &str) -> bool {
    let l = p.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"].iter().any(|e| l.ends_with(e))
}

/// "Screenshot 23:41" for pasted images (their file names are generated), else the file name.
fn file_label(path: &str) -> String {
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let re = regex::Regex::new(r"^paste-\d{4}-\d{2}-\d{2}T(\d{2})-(\d{2})-(\d{2})-\d{3}Z\.").unwrap();
    match re.captures(&name) {
        Some(c) => {
            use chrono::{Local, NaiveTime, TimeZone, Utc};
            let t = NaiveTime::from_hms_opt(c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0), 0).unwrap_or_default();
            let utc = Utc::now().date_naive().and_time(t);
            let local = Local.from_utc_datetime(&utc);
            format!("Screenshot {}", local.format("%H:%M"))
        }
        None => name,
    }
}

/// A small borderless button in the control row.
fn ctl(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .h(px(28.))
        .px(px(6.))
        .text_xs()
        .text_color(muted_fg())
        .cursor_pointer()
        .hover(|s| s.bg(hover()).text_color(foreground()))
}

impl Composer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx).placeholder("Message Claude…").submit_on_enter(true);
            s.set_auto_grow(1, 10, cx);
            s
        });
        cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
            InputEvent::Change => this.hist_index = None,
            _ => {}
        })
        .detach();
        Self {
            tab: None,
            input,
            files: vec![],
            drafts: HashMap::new(),
            history: HashMap::new(),
            hist_index: None,
            menu: None,
            menu_closed: None,
            show_help: false,
            busy: false,
            waiting: false,
            is_chat: false,
            voice_on: false,
            dictation: None,
            model: None,
            mode: None,
            actual_mode: None,
        }
    }

    pub fn text(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let t = text.to_string();
        self.input.update(cx, |s, cx| s.set_value(t, window, cx));
    }

    /// Switches drafts with the tab.
    pub fn set_tab(&mut self, tab: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if tab == self.tab {
            return;
        }
        if let Some(old) = &self.tab {
            let text = self.text(cx);
            self.drafts.insert(old.clone(), (text, std::mem::take(&mut self.files)));
        }
        let (text, files) = tab.as_ref().and_then(|t| self.drafts.remove(t)).unwrap_or_default();
        self.files = files;
        self.set_text(&text, window, cx);
        self.tab = tab;
        self.hist_index = None;
        self.menu = None;
        cx.notify();
    }

    fn can_send(&self, cx: &App) -> bool {
        !self.waiting && (!self.text(cx).trim().is_empty() || !self.files.is_empty())
    }

    fn send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        let text = self.text(cx).trim_end().to_string();
        cx.emit(ComposerEvent::Send { text, files: self.files.clone() });
    }

    /// The app accepted the message: clear the box and remember it.
    pub fn sent(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = &self.tab {
            if !text.is_empty() {
                let h = self.history.entry(tab.clone()).or_default();
                h.retain(|x| x != text);
                h.insert(0, text.to_string());
                h.truncate(50);
            }
        }
        self.files.clear();
        self.hist_index = None;
        self.set_text("", window, cx);
        self.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| s.focus(window, cx));
    }

    pub fn add_files(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        for p in paths {
            if !self.files.contains(&p) && std::path::Path::new(&p).is_file() {
                self.files.push(p);
            }
        }
        cx.notify();
    }

    fn walk_history(&mut self, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = &self.tab else { return };
        let past = self.history.get(tab).cloned().unwrap_or_default();
        if past.is_empty() {
            return;
        }
        let next = match (self.hist_index, up) {
            (None, true) => Some(0),
            (Some(i), true) => Some((i + 1).min(past.len() - 1)),
            (Some(0), false) | (None, false) => None,
            (Some(i), false) => Some(i - 1),
        };
        let text = next.map(|i| past[i].clone()).unwrap_or_default();
        self.set_text(&text, window, cx);
        self.hist_index = next;
    }

    fn pick_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Attach".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| this.add_files(paths.into_iter().map(|p| p.to_string_lossy().to_string()).collect(), cx));
            }
        })
        .detach();
    }

    /// The picked mode; what Claude Code reports wins when it differs (e.g. auto wasn't
    /// available, or an approved plan moved on). Nothing picked = auto.
    fn mode_label(&self) -> String {
        let label = |v: &str| MODES.iter().find(|(m, _, _)| *m == v).map(|(_, l, _)| l.to_string()).unwrap_or_else(|| v.to_string());
        let actual = self.actual_mode.as_deref().map(|m| if m == "manual" { "default" } else { m });
        let picked = self.mode.as_deref().unwrap_or("auto");
        label(actual.unwrap_or(picked))
    }

    fn toggle_menu(&mut self, m: Menu, cx: &mut Context<Self>) {
        let just_closed = self.menu_closed.take().is_some_and(|(c, at)| c == m && at.elapsed().as_millis() < 400);
        self.menu = if self.menu == Some(m) || just_closed { None } else { Some(m) };
        cx.notify();
    }

    fn render_files(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        if self.files.is_empty() {
            return None;
        }
        let files = self.files.clone();
        Some(div().flex().flex_wrap().gap(px(8.)).px(px(4.)).pt(px(4.)).children(files.into_iter().enumerate().map(|(i, p)| {
            let p_open = p.clone();
            let p_rm = p.clone();
            let thumb = if is_image(&p) {
                div().size_full().child(img(std::path::PathBuf::from(p.clone())).size_full().object_fit(ObjectFit::Cover))
            } else {
                div().size_full().flex().items_center().justify_center().child(icon("file-text", muted_fg()).size(px(22.)))
            };
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .w(px(112.))
                .child(
                    div()
                        .id(("file", i))
                        .relative()
                        .h(px(72.))
                        .w(px(112.))
                        .border_1()
                        .border_color(border())
                        .bg(background())
                        .overflow_hidden()
                        .cursor_pointer()
                        .on_click(move |_, _, _| {
                            let _ = crate::backend::util::open_path(&p_open);
                        })
                        .child(thumb)
                        .child(
                            div()
                                .id(("rm", i))
                                .absolute()
                                .top_0()
                                .right_0()
                                .size(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(background().opacity(0.85))
                                .cursor_pointer()
                                .hover(|s| s.bg(secondary()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.files.retain(|x| x != &p_rm);
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .child(icon("x", foreground()).size(px(11.))),
                        ),
                )
                .child(div().text_size(px(10.)).text_color(muted_fg()).truncate().child(file_label(&p)))
        })))
    }

    fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu?;
        let mut list = div()
            .w(px(260.))
            .flex()
            .flex_col()
            .p(px(4.))
            .bg(card())
            .border_1()
            .border_color(input_border())
            .shadow_lg()
            .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                if this.menu == Some(menu) {
                    this.menu = None;
                    this.menu_closed = Some((menu, std::time::Instant::now()));
                    cx.notify();
                }
            }));
        match menu {
            Menu::Model => {
                let cur = self.model.clone().unwrap_or_default();
                for (i, (alias, label)) in MODELS.iter().enumerate() {
                    let on = *alias == cur;
                    let alias = alias.to_string();
                    list = list.child(
                        div()
                            .id(("m", i))
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(8.))
                            .py(px(6.))
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(hover()))
                            .child(*label)
                            .when(on, |d| d.child(icon("check", foreground())))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.menu = None;
                                cx.emit(ComposerEvent::SetModel((!alias.is_empty()).then(|| alias.clone())));
                                cx.notify();
                            })),
                    );
                }
            }
            Menu::Mode => {
                let cur = self.mode.clone().unwrap_or_else(|| "auto".into());
                for (i, (value, label, hint)) in MODES.iter().enumerate() {
                    let on = *value == cur;
                    let value = value.to_string();
                    list = list.child(
                        div()
                            .id(("p", i))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.))
                            .px(px(8.))
                            .py(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(hover()))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_xs().child(*label))
                                    .child(div().text_size(px(10.)).text_color(muted_fg()).child(*hint)),
                            )
                            .when(on, |d| d.child(icon("check", foreground())))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.menu = None;
                                cx.emit(ComposerEvent::SetMode((!value.is_empty()).then(|| value.clone())));
                                cx.notify();
                            })),
                    );
                }
            }
        }
        Some(deferred(div().absolute().bottom(px(40.)).right(px(40.)).child(list)).with_priority(1).into_any_element())
    }
}

impl Render for Composer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let can_send = self.can_send(cx);
        let model_label = MODELS.iter().find(|(a, _)| Some(*a) == self.model.as_deref()).map(|(_, l)| *l).unwrap_or("Default model");
        let mode_label = self.mode_label();
        let files = self.render_files(cx);
        let menu = self.render_menu(cx);

        let send_or_stop = if self.busy && self.is_chat {
            div()
                .id("stop")
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .bg(primary())
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .child(icon("stop-fill", primary_fg()).size(px(12.)))
                .on_click(cx.listener(|_, _, _, cx| cx.emit(ComposerEvent::Stop)))
        } else {
            div()
                .id("send")
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .bg(if can_send { primary() } else { secondary() })
                .when(can_send, |d| d.cursor_pointer().hover(|s| s.opacity(0.9)))
                .child(icon("arrow-up", if can_send { primary_fg() } else { muted_fg() }))
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
        };

        let is_chat = self.is_chat;
        let help_button = theme::icon_button("help", "question").on_click(cx.listener(|this, _, _, cx| {
            this.show_help = !this.show_help;
            cx.notify();
        }));
        // Chat tabs: a card centered with the message column (same width and padding as
        // chat_view's column); terminal tabs keep the full-width box with help beside it.
        let column = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .when(is_chat, |d| d.max_w(px(theme::CHAT_COLUMN)).px(px(16.)))
            .when(!is_chat, |d| d.px(px(12.)));

        let column = column
            .when_some(self.dictation.clone(), |d, status| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(11.))
                        .text_color(muted_fg())
                        .child(icon("microphone-fill", theme::red()).size(px(12.)))
                        .child(status),
                )
            })
            .when(self.waiting, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(11.))
                        .text_color(amber())
                        .child(icon("warning-circle", amber()))
                        .child(if self.is_chat {
                            "Claude is waiting for your answer above (permission or a question)."
                        } else {
                            "Claude is asking something in the terminal above (permission or a choice). Answer there first."
                        }),
                )
            })
            .when(self.show_help, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .p(px(8.))
                        .border_1()
                        .border_color(border())
                        .bg(card())
                        .text_size(px(11.))
                        .children(HELP.iter().map(|(k, v)| {
                            div()
                                .flex()
                                .gap(px(12.))
                                .child(div().w(px(130.)).flex_none().font_weight(FontWeight::MEDIUM).child(*k))
                                .child(div().text_color(muted_fg()).child(*v))
                        })),
                )
            })
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(6.))
                    .child(
                        // The box: attachments, text, controls.
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .p(px(6.))
                            .border_1()
                            .border_color(input_border())
                            .bg(input_bg())
                            .when(self.waiting, |d| d.opacity(0.7))
                            .children(files)
                            .child(div().px(px(2.)).child(Textarea::new(&self.input).appearance(false)))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.))
                                    .child(ctl("attach").child(icon("plus", muted_fg()).size(px(16.))).on_click(cx.listener(|this, _, w, cx| this.pick_files(w, cx))))
                                    .child(
                                        ctl("mic")
                                            .child(icon(if self.dictation.is_some() { "microphone-fill" } else { "microphone" }, if self.dictation.is_some() { theme::red() } else if self.voice_on { foreground() } else { muted_fg() }).size(px(16.)))
                                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ComposerEvent::Dictate))),
                                    )
                                    .child(div().flex_1())
                                    .when(is_chat, |d| {
                                        d.child(
                                            ctl("mode")
                                                .child(icon("shield-warning", muted_fg()))
                                                .child(mode_label)
                                                .child(icon("caret-down", muted_fg()).size(px(10.)))
                                                .on_click(cx.listener(|this, _, _, cx| this.toggle_menu(Menu::Mode, cx))),
                                        )
                                    })
                                    .child(
                                        ctl("model")
                                            .child(model_label)
                                            .child(icon("caret-down", muted_fg()).size(px(10.)))
                                            .on_click(cx.listener(|this, _, _, cx| this.toggle_menu(Menu::Model, cx))),
                                    )
                                    .when(is_chat, |d| {
                                        d.child(ctl("help-inline").child(icon("question", muted_fg())).on_click(cx.listener(|this, _, _, cx| {
                                            this.show_help = !this.show_help;
                                            cx.notify();
                                        })))
                                    })
                                    .child(div().w(px(4.)))
                                    .child(send_or_stop),
                            )
                            .children(menu),
                    )
                    .when(!is_chat, |d| d.child(help_button.size(px(28.)).mb(px(6.)))),
            );

        div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(4.))
            .pb(px(12.))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_files(paths.paths().iter().map(|p| p.to_string_lossy().to_string()).collect(), cx);
            }))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                if k.modifiers.control && k.key == "v" {
                    // A screenshot in the clipboard becomes an attachment; text pastes normally.
                    if let Some(path) = platform::save_clipboard_image() {
                        this.add_files(vec![path], cx);
                        cx.stop_propagation();
                    }
                } else if k.key == "escape" {
                    if this.menu.take().is_some() {
                        cx.notify();
                    } else {
                        cx.emit(ComposerEvent::FocusTerminal);
                    }
                    cx.stop_propagation();
                } else if k.modifiers.control && (k.key == "up" || k.key == "down") {
                    this.walk_history(k.key == "up", window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(column)
    }
}
