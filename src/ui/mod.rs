//! The window: sidebar, title bar, pages, Claude tabs, message box, session panel.

pub mod app;
mod chat_store;
mod chat_view;
mod composer;
pub mod icons;
mod panel;
mod terminal_view;
pub mod theme;
mod views;

use app::{HandoffDialog, HandoffMode, LauncherApp, TabKind, View};
use crate::backend::types::{Activity, TabMode, UpdateState};
use icons::icon;
use gpui_kit::*;
use gpui_kit::prelude::*;
use theme::*;

pub const TITLEBAR_H: f32 = 40.;

/// A thin strip on a panel's edge that can be dragged to resize it.
pub fn resize_handle(id: &'static str, right_edge: bool) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(6.))
        .when(right_edge, |d| d.right(px(-3.)))
        .when(!right_edge, |d| d.left(px(-3.)))
        .cursor_col_resize()
        .hover(|s| s.bg(hsla(0., 0., 1., 0.12)))
}

impl LauncherApp {
    pub fn sidebar_width(&self) -> f32 {
        self.store.settings.sidebar_width.unwrap_or(240.)
    }

    pub fn panel_width(&self) -> f32 {
        self.store.settings.panel_width.unwrap_or(320.)
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        if self.resizing.take().is_some() {
            self.store.save();
            cx.notify();
        }
    }

    pub fn open_handoff_dialog(&mut self, from_tab: &str) {
        let Some(from) = self.tabs.iter().find(|t| t.id == from_tab) else { return };
        // Default: the signed-in account with the most room left.
        let mut targets: Vec<_> = self.profiles.iter().filter(|p| p.name != from.profile).collect();
        targets.sort_by(|a, b| usage_hot_pct(a.usage.as_ref()).partial_cmp(&usage_hot_pct(b.usage.as_ref())).unwrap());
        let to = targets.iter().find(|p| p.signed_in).or(targets.first()).map(|p| p.name.clone());
        self.handoff_dialog = Some(HandoffDialog { from_tab: from_tab.into(), to, mode: HandoffMode::Summary });
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let item = |id: SharedString, label: String, icon_name: &'static str, active: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(30.))
                .px(px(8.))
                .text_sm()
                .text_color(if active { foreground() } else { foreground().opacity(0.85) })
                .cursor_pointer()
                .when(active, |d| d.bg(secondary()).font_weight(FontWeight::MEDIUM))
                .hover(|d| d.bg(hover()))
                .child(icon(icon_name, if active { foreground() } else { muted_fg() }).size(px(16.)))
                .child(div().flex_1().min_w_0().truncate().child(label))
        };
        let group_badge = |this: &LauncherApp, group: &str| -> Option<Div> {
            let tabs: Vec<_> = this.tabs.iter().filter(|t| t.group() == group && t.exit_code.is_none()).collect();
            if tabs.is_empty() {
                return None;
            }
            let sig = tabs.iter().fold(None, |acc, t| strongest(acc, this.tab_signal(t)));
            Some(div().flex().items_center().gap(px(6.)).text_xs().text_color(muted_fg()).children(signal_dot(sig)).child(tabs.len().to_string()))
        };

        let mut projects = div().flex().flex_col().gap(px(1.));
        let list = self.store.projects.clone();
        for p in list.iter() {
            let active = self.view == View::Project(p.id.clone());
            let id = p.id.clone();
            let exists = p.exists();
            projects = projects.child(
                item(SharedString::from(format!("side-{}", p.id)), p.name.clone(), "folder-simple", active)
                    .when(!exists, |d| d.text_color(muted_fg()).line_through())
                    .children(group_badge(self, &p.id))
                    .on_click(cx.listener(move |this, _, w, cx| this.set_view(View::Project(id.clone()), w, cx))),
            );
        }
        if list.is_empty() {
            projects = projects.child(div().px(px(8.)).py(px(4.)).child(muted("No projects yet.")));
        }

        div()
            .w(px(self.sidebar_width()))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(sidebar())
            .border_r_1()
            .border_color(border())
            // Empty drag strip level with the title bar.
            .child(div().h(px(TITLEBAR_H)).flex_none().window_control_area(WindowControlArea::Drag))
            .child(
                div()
                    .id("side-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .px(px(8.))
                    .pb(px(8.))
                    .child(
                        item("side-dash".into(), "Dashboard".into(), "squares-four", self.view == View::Dashboard)
                            .on_click(cx.listener(|this, _, w, cx| this.set_view(View::Dashboard, w, cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .pl(px(8.))
                                    .h(px(26.))
                                    .text_xs()
                                    .text_color(muted_fg())
                                    .child("Projects")
                                    .child(icon_button("side-add", "plus").on_click(cx.listener(|this, _, w, cx| this.add_project(w, cx)))),
                            )
                            .child(projects),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .p(px(8.))
                    .border_t_1()
                    .border_color(border())
                    .child(
                        item("side-acc".into(), "Accounts".into(), "users-three", self.view == View::Accounts)
                            .children(group_badge(self, "accounts"))
                            .on_click(cx.listener(|this, _, w, cx| this.set_view(View::Accounts, w, cx))),
                    )
                    .child(
                        item("side-set".into(), "Settings".into(), "gear-six", self.view == View::Settings)
                            .on_click(cx.listener(|this, _, w, cx| this.set_view(View::Settings, w, cx))),
                    ),
            )
            .child(resize_handle("side-resize", true).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.resizing = Some(app::Resize::Sidebar);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    /// Flat title bar: breadcrumbs, a drag area and the window buttons.
    fn render_titlebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let crumbs: Vec<String> = match &self.view {
            View::Project(id) => vec!["Projects".into(), self.project(id).map(|p| p.name.clone()).unwrap_or_default()],
            View::Dashboard => vec!["Dashboard".into()],
            View::Accounts => vec!["Accounts".into()],
            View::Settings => vec!["Settings".into()],
        };
        let n = crumbs.len();
        let panel_toggle = matches!(&self.view, View::Project(id) if self.project(id).is_some());
        let panel_open = self.store.settings.panel_open.unwrap_or(true);
        let maximized = window.is_maximized();
        let win_button = |id: &'static str, icon_name: &'static str, area: WindowControlArea, close: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .w(px(46.))
                .h_full()
                .window_control_area(area)
                .when(close, |d| d.hover(|s| s.bg(c(0xc42b1c))))
                .when(!close, |d| d.hover(|s| s.bg(hover())))
                .child(icon(icon_name, foreground().opacity(0.8)).size(px(12.)))
        };
        div()
            .h(px(TITLEBAR_H))
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(border())
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .pl(px(16.))
                    .text_xs()
                    .window_control_area(WindowControlArea::Drag)
                    .children(crumbs.into_iter().enumerate().map(|(i, c)| {
                        div()
                            .flex()
                            .gap(px(8.))
                            .when(i > 0, |d| d.child(div().text_color(muted_fg()).child("/")))
                            .child(div().when(i + 1 < n, |d| d.text_color(muted_fg())).when(i + 1 == n, |d| d.font_weight(FontWeight::MEDIUM)).child(c))
                    })),
            )
            .when(panel_toggle, |t| {
                t.child(
                    div().px(px(6.)).child(
                        div()
                            .id("toggle-panel")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(26.))
                            .cursor_pointer()
                            .hover(|s| s.bg(hover()))
                            .child(icon("sidebar-simple", if panel_open { foreground() } else { muted_fg() }).size(px(16.)).with_transformation(Transformation::scale(size(-1., 1.))))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_panel(cx))),
                    ),
                )
            })
            .child(win_button("win-min", "minus", WindowControlArea::Min, false))
            .child(win_button("win-max", if maximized { "copy" } else { "square" }, WindowControlArea::Max, false))
            .child(win_button("win-close", "x", WindowControlArea::Close, true))
            .into_any_element()
    }

    fn render_tabs(&mut self, group: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let group_tabs: Vec<_> = self.tabs.iter().filter(|t| t.group() == group).cloned().collect();
        let active_id = self.active.get(group).cloned();
        let strip = (!group_tabs.is_empty()).then(|| {
            div()
                .id("tab-strip")
                .flex_none()
                .flex()
                .overflow_x_scroll()
                .border_b_1()
                .border_color(border())
                .bg(sidebar())
                .children(group_tabs.iter().map(|t| {
                    let active = active_id.as_deref() == Some(t.id.as_str());
                    let id = t.id.clone();
                    let id2 = t.id.clone();
                    let name = match t.kind {
                        TabKind::Login => "sign in".to_string(),
                        TabKind::Project => self.tracker.get(&t.id).and_then(|s| s.session_name.clone()).map(|n| n.chars().take(28).collect()).unwrap_or("claude".into()),
                    };
                    div()
                        .id(SharedString::from(format!("tab-{}", t.id)))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(8.))
                        .py(px(4.))
                        .pl(px(12.))
                        .pr(px(4.))
                        .border_r_1()
                        .border_color(border())
                        .text_xs()
                        .cursor_pointer()
                        .text_color(if active { foreground() } else { muted_fg() })
                        .when(active, |d| d.bg(background()))
                        .on_click(cx.listener(move |this, _, w, cx| this.select_tab(&id, w, cx)))
                        .children(signal_dot(self.tab_signal(t)))
                        .child(icon(if t.mode == TabMode::Chat { "chat-circle" } else { "terminal-window" }, if active { foreground() } else { muted_fg() }))
                        .child(profile_tag(&t.profile))
                        .child(div().when(t.exit_code.is_some(), |d| d.line_through()).child(name))
                        .child(icon_button(SharedString::from(format!("close-{}", t.id)), "x").on_click(cx.listener(move |this, _, w, cx| {
                            cx.stop_propagation();
                            this.close_tab(&id2, w, cx);
                        })))
                }))
        });

        let content: AnyElement = match group_tabs.iter().find(|t| Some(&t.id) == active_id.as_ref()) {
            Some(t) if t.mode == TabMode::Chat => self.render_chat(&t.id.clone(), window, cx),
            Some(t) => match self.terms.get(&t.id) {
                Some(v) => v.clone().into_any_element(),
                None => div().into_any_element(),
            },
            None => div()
                .p(px(24.))
                .child(muted(if group == "accounts" {
                    "Sign-in terminals appear here."
                } else {
                    "No Claude Code session open. Pick an account above to start one."
                }))
                .into_any_element(),
        };

        // The message box under project tabs.
        let composer = self.active_tab().filter(|t| t.kind == TabKind::Project).cloned().map(|t| {
            let activity = self.tracker.get(&t.id).map(|s| s.activity);
            let busy = self.chats.get(&t.id).map(|c| c.busy).unwrap_or(false);
            let mode = self.chats.get(&t.id).and_then(|c| c.permission_mode.clone());
            let model = t.model.clone();
            let voice_on = self.store.settings.voice_mode == Some(crate::backend::types::VoiceMode::Tap) && t.mode == TabMode::Terminal;
            self.composer.update(cx, |c, _| {
                c.waiting = activity == Some(Activity::NeedsYou);
                c.busy = busy;
                c.is_chat = t.mode == TabMode::Chat;
                c.voice_on = voice_on;
                c.mode = mode;
                c.model = model;
            });
            self.composer.clone()
        });

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(border())
            .children(strip)
            .child(div().flex_1().min_h_0().relative().bg(background()).child(content))
            .children(composer)
            .into_any_element()
    }

    fn render_handoff_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.handoff_dialog.as_ref()?;
        let from = self.tabs.iter().find(|t| t.id == d.from_tab)?.clone();
        let session = self.tracker.get(&from.id).cloned();
        let targets: Vec<_> = self.profiles.iter().filter(|p| p.name != from.profile).cloned().collect();
        let to = d.to.clone();
        let mode = d.mode;
        let can_full = session.as_ref().and_then(|s| s.transcript_path.as_ref()).is_some();
        let busy = session.as_ref().map(|s| s.activity == Activity::Working).unwrap_or(false);
        let tokens = session.as_ref().and_then(|s| s.context.as_ref()).map(|c| c.tokens);

        let option = |id: &'static str, selected: bool, enabled: bool, title: &str, desc: String| {
            div()
                .id(id)
                .flex()
                .gap(px(12.))
                .px(px(12.))
                .py(px(8.))
                .border_1()
                .border_color(if selected { primary() } else { border() })
                .when(enabled, |d| d.cursor_pointer())
                .when(!enabled, |d| d.opacity(0.5))
                .child(div().flex_none().child(if selected { "◉" } else { "○" }))
                .child(div().flex().flex_col().gap(px(2.)).child(div().text_xs().child(title.to_string())).child(muted(desc)))
        };

        let body = if targets.is_empty() {
            div().text_xs().text_color(amber()).child("Add another account under Accounts first.")
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(
                    div().flex().flex_col().gap(px(6.)).child(div().text_xs().font_weight(FontWeight::MEDIUM).child("To account")).children(targets.iter().map(|p| {
                        let name = p.name.clone();
                        let sel = to.as_deref() == Some(p.name.as_str());
                        div()
                            .id(SharedString::from(format!("ho-{}", p.name)))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .px(px(12.))
                            .py(px(8.))
                            .border_1()
                            .border_color(if sel { primary() } else { border() })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(d) = this.handoff_dialog.as_mut() {
                                    d.to = Some(name.clone());
                                }
                                cx.notify();
                            }))
                            .child(div().child(if sel { "◉" } else { "○" }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(profile_tag(&p.name))
                                    .child(muted(p.account.as_ref().and_then(|a| a.email.clone()).unwrap_or(if p.signed_in { String::new() } else { "not signed in".into() }))),
                            )
                            .children(usage_inline(p.usage.as_ref()))
                    })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("How"))
                        .child(
                            option(
                                "ho-summary",
                                mode == HandoffMode::Summary,
                                true,
                                "Summary via docs/STATUS.md (recommended)",
                                "Claude here writes the current state to docs/STATUS.md and commits it. Then a new session on the other account reads it and continues. Small and cheap, and the handover is saved in git.".into(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.handoff_dialog.as_mut() {
                                    d.mode = HandoffMode::Summary;
                                }
                                cx.notify();
                            })),
                        )
                        .child(
                            option(
                                "ho-full",
                                mode == HandoffMode::Full,
                                can_full,
                                "Continue the full conversation",
                                format!(
                                    "The other account opens this exact conversation and remembers everything. But the whole conversation{} is sent again with its next message, which uses more of its limit.{}",
                                    tokens.map(|t| format!(" (about {}k tokens)", t / 1000)).unwrap_or_default(),
                                    if can_full { "" } else { " Not available yet: send at least one message in this session first." }
                                ),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if can_full {
                                    if let Some(d) = this.handoff_dialog.as_mut() {
                                        d.mode = HandoffMode::Full;
                                    }
                                }
                                cx.notify();
                            })),
                        ),
                )
                .when(mode == HandoffMode::Summary && busy, |d| {
                    d.child(div().text_size(px(11.)).text_color(amber()).child("Claude is still working in this tab. The handoff starts when it finishes."))
                })
        };

        Some(
            div()
                .id("handoff-overlay")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0., 0.6))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .w(px(520.))
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .p(px(20.))
                        .bg(card())
                        .border_1()
                        .border_color(border())
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child("⇄ Hand off to another account"))
                                .child(muted("Continue this project's work with a different Claude account.")),
                        )
                        .child(body)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(button("ho-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| {
                                    this.handoff_dialog = None;
                                    cx.notify();
                                })))
                                .child(disabled(
                                    button_primary("ho-go", format!("Hand off to {}", to.clone().unwrap_or_default()))
                                        .on_click(cx.listener(|this, _, w, cx| this.start_handoff(w, cx))),
                                    to.is_none() || targets.is_empty(),
                                )),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for LauncherApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let group = self.current_group();
        let project = match &self.view {
            View::Project(id) => self.project(id).cloned(),
            _ => None,
        };
        let page: AnyElement = match &self.view {
            View::Dashboard => self.render_dashboard(cx),
            View::Project(_) => match &project {
                Some(p) => self.render_project_header(p, cx),
                None => div().p(px(32.)).child(muted("This project was removed.")).into_any_element(),
            },
            View::Accounts => self.render_accounts(cx),
            View::Settings => self.render_settings(cx),
        };
        let tabs = group.as_ref().filter(|_| project.is_some() || self.view == View::Accounts).map(|g| self.render_tabs(&g.clone(), window, cx));
        let panel = project.as_ref().filter(|_| self.store.settings.panel_open.unwrap_or(true)).map(|p| self.render_panel(p, cx));
        let dialog = self.render_handoff_dialog(cx);
        let update_ready = match &self.update {
            UpdateState::Ready { version } => Some(version.clone()),
            _ => None,
        };

        div()
            .size_full()
            .relative()
            .flex()
            .bg(background())
            .text_color(foreground())
            .font_family(UI_FONT)
            // Ctrl+Space: voice dictation in the active terminal tab (before the terminal sees it).
            // Dragging a panel edge.
            .when(self.resizing.is_some(), |d| d.cursor_col_resize())
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, cx| {
                let Some(r) = this.resizing else { return };
                if ev.pressed_button != Some(MouseButton::Left) {
                    this.end_resize(cx);
                    return;
                }
                let x = f32::from(ev.position.x);
                match r {
                    app::Resize::Sidebar => this.store.settings.sidebar_width = Some(x.clamp(180., 420.)),
                    app::Resize::Panel => {
                        let w = f32::from(window.viewport_size().width);
                        this.store.settings.panel_width = Some((w - x).clamp(260., 600.));
                    }
                }
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.end_resize(cx)))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                let k = &ev.keystroke;
                if k.modifiers.control && !k.modifiers.alt && !k.modifiers.shift && k.key == "space" && this.active_tab().is_some() {
                    cx.stop_propagation();
                    this.dictate(w, cx);
                }
            }))
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(self.render_titlebar(window, cx))
                    .when_some(update_ready, |d, v| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .px(px(16.))
                                .py(px(6.))
                                .bg(emerald().opacity(0.1))
                                .text_xs()
                                .child(format!("Version {v} is ready. It installs when you restart."))
                                .child(button_primary("banner-restart", "Restart").h(px(22.)).on_click(cx.listener(|this, _, w, cx| this.install_update(w, cx)))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .when(self.claude_path().is_none(), |d| {
                                        d.child(
                                            div()
                                                .m(px(16.))
                                                .mb_0()
                                                .p(px(12.))
                                                .border_1()
                                                .border_color(red_strong().opacity(0.5))
                                                .text_xs()
                                                .child(div().flex().items_center().gap(px(6.)).font_weight(FontWeight::MEDIUM).text_color(theme::red()).child(icon("warning", theme::red())).child("Claude Code was not found"))
                                                .child(muted("Install it, or set the path to claude.exe in Settings.")),
                                        )
                                    })
                                    .child(page)
                                    .children(tabs),
                            )
                            .children(panel),
                    ),
            )
            .children(dialog)
    }
}
