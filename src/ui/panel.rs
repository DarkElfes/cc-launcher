//! The session side panel: live session info, plan, tasks, background work, outputs, git changes,
//! connectors and history.

use super::app::{LauncherApp, OpenRequest};
use super::icons::icon;
use super::theme::{self, *};
use crate::backend::types::*;
use gpui_kit::*;
use gpui_kit::prelude::*;

fn row(label: &str, value: impl IntoElement) -> Div {
    div()
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(12.))
        .py(px(1.))
        .text_xs()
        .child(div().flex_none().text_color(muted_fg()).child(label.to_string()))
        .child(div().min_w_0().truncate().child(value))
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1000 {
        if n >= 100_000 { format!("{}k", n / 1000) } else { format!("{:.1}k", n as f64 / 1000.) }
    } else {
        n.to_string()
    }
}

fn fmt_duration(ms: f64) -> String {
    let m = (ms / 60000.).round() as i64;
    if m < 60 { format!("{m} min") } else { format!("{} h {} min", m / 60, m % 60) }
}

fn rel_path(path: &str, root: &str) -> String {
    let norm = |p: &str| p.replace('/', "\\").to_lowercase();
    if norm(path).starts_with(&(norm(root) + "\\")) { path[root.len() + 1..].to_string() } else { path.to_string() }
}

impl LauncherApp {
    fn section(&mut self, title: &'static str, right: Option<AnyElement>, body: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let open = !self.collapsed.contains(title);
        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(12.))
                    .py(px(8.))
                    .child(
                        div()
                            .id(title)
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .child(icon(if open { "caret-down" } else { "caret-right" }, muted_fg()).size(px(10.)))
                            .child(title)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.collapsed.remove(title) {
                                    this.collapsed.insert(title);
                                }
                                cx.notify();
                            })),
                    )
                    .children(right),
            )
            .when(open, |d| d.child(div().px(px(12.)).pb(px(12.)).child(body)))
            .into_any_element()
    }

    pub fn render_panel(&mut self, project: &Project, cx: &mut Context<Self>) -> AnyElement {
        let panel_tab = self.active.get(&project.id).and_then(|id| self.tabs.iter().find(|t| &t.id == id)).cloned();
        let s = panel_tab.as_ref().and_then(|t| self.tracker.get(&t.id)).cloned();
        let profile = panel_tab.as_ref().and_then(|t| self.profile(&t.profile)).cloned();
        let can_handoff = panel_tab.as_ref().map(|t| t.exit_code.is_none()).unwrap_or(false) && self.handoff.is_none();

        // --- session ---
        let session = match &s {
            None => muted("Open a Claude Code tab to see its session here."),
            Some(s) => {
                let pct = s.context.as_ref().map(|c| c.used_percent.unwrap_or(c.tokens as f64 / c.size as f64 * 100.).round());
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(profile.as_ref().map(|p| profile_tag(&p.name)).unwrap_or_else(div))
                            .child(activity_badge(s.activity)),
                    )
                    .when_some(profile.as_ref().and_then(|p| p.account.as_ref()).and_then(|a| a.email.clone()), |d, e| d.child(muted(e)))
                    .when_some(s.session_name.clone(), |d, n| d.child(row("Session", n)))
                    .child(row("Model", s.model.clone().unwrap_or("—".into())))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div().flex().justify_between().text_xs().child(div().text_color(muted_fg()).child("Context")).child(match &s.context {
                                    Some(c) => format!("{}% · {} / {}", pct.unwrap_or(0.), fmt_tokens(c.tokens), fmt_tokens(c.size)),
                                    None => "—".into(),
                                }),
                            )
                            .child(progress(
                                pct.unwrap_or(0.),
                                if pct.unwrap_or(0.) >= 85. { red_strong() } else if pct.unwrap_or(0.) >= 60. { amber() } else { sky() },
                            )),
                    )
                    .child(row("API-price equivalent", s.cost_usd.map(|c| format!("${c:.2}")).unwrap_or("—".into())))
                    .when(s.lines_added.is_some() || s.lines_removed.is_some(), |d| {
                        d.child(row(
                            "Lines",
                            div()
                                .flex()
                                .gap(px(4.))
                                .child(div().text_color(emerald()).child(format!("+{}", s.lines_added.unwrap_or(0))))
                                .child(div().text_color(theme::red()).child(format!("−{}", s.lines_removed.unwrap_or(0)))),
                        ))
                    })
                    .when_some(s.duration_ms, |d, ms| d.child(row("Time", fmt_duration(ms))))
                    .when_some(s.error.clone(), |d, e| d.child(div().text_size(px(11.)).text_color(theme::red()).child(e)))
                    .when(s.error.is_none() && matches!(s.activity, Activity::NeedsYou | Activity::Done), |d| {
                        d.when_some(s.message.clone(), |d, m| d.child(muted(m)))
                    })
            }
        };

        let mut sections: Vec<AnyElement> = vec![];
        if let Some(plan) = s.as_ref().and_then(|s| s.plan.clone()) {
            let body = div()
                .id("plan-text")
                .max_h(px(256.))
                .overflow_y_scroll()
                .border_1()
                .border_color(border())
                .bg(secondary().opacity(0.3))
                .p(px(8.))
                .text_size(px(11.))
                .child(plan)
                .into_any_element();
            sections.push(self.section("Plan", None, body, cx));
        }

        // --- tasks ---
        let tasks = s.as_ref().map(|s| s.tasks.clone()).unwrap_or_default();
        let done = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();
        let tasks_body = if tasks.is_empty() {
            muted("No task list yet. Claude creates one for multi-step work.").into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(tasks.iter().map(|t| {
                    let (glyph, color) = match t.status {
                        TaskStatus::Completed => ("check", emerald()),
                        TaskStatus::InProgress => ("circle-dashed", sky()),
                        TaskStatus::Pending => ("circle", muted_fg()),
                    };
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.))
                        .text_xs()
                        .child(icon(glyph, color).mt(px(1.)))
                        .child(div().when(t.status == TaskStatus::Completed, |d| d.text_color(muted_fg()).line_through()).child(t.subject.clone()))
                }))
                .into_any_element()
        };
        let right = (!tasks.is_empty()).then(|| muted(format!("{done}/{}", tasks.len())).into_any_element());
        sections.push(self.section("Tasks", right, tasks_body, cx));

        // --- background ---
        let mut agents = s.as_ref().map(|s| s.agents.clone()).unwrap_or_default();
        agents.sort_by(|a, b| b.running.cmp(&a.running).then(b.started_at.cmp(&a.started_at)));
        let running = agents.iter().filter(|a| a.running).count();
        let bg_body = if agents.is_empty() {
            muted("No subagents or background commands.").into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .children(agents.iter().take(20).map(|a| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_xs()
                        .child(icon(if a.kind == "bash" { "terminal-window" } else { "robot" }, muted_fg()))
                        .child(div().flex_1().min_w_0().truncate().child(if a.kind == "bash" {
                            a.label.clone().unwrap_or("background command".into())
                        } else {
                            a.kind.clone()
                        }))
                        .child(if a.running { badge("running", sky()) } else { muted(a.ended_at.map(|t| time_ago(Some(t))).unwrap_or("done".into())) })
                }))
                .into_any_element()
        };
        let right = (running > 0).then(|| div().text_size(px(10.)).text_color(sky()).flex().items_center().gap(px(4.)).child(icon("circle-dashed", sky()).size(px(10.))).child(format!("{running}")).into_any_element());
        sections.push(self.section("Background", right, bg_body, cx));

        // --- outputs ---
        let outputs = s.as_ref().map(|s| s.outputs.clone()).unwrap_or_default();
        let root = project.path.clone();
        let out_body = if outputs.is_empty() {
            muted("Files Claude writes or edits in this session show up here.").into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .children(outputs.iter().take(50).enumerate().map(|(i, o)| {
                    let path = o.path.clone();
                    div()
                        .id(("out", i))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .py(px(1.))
                        .text_xs()
                        .cursor_pointer()
                        .hover(|d| d.bg(hover()))
                        .on_click(cx.listener(move |this, _, w, cx| this.open_in_editor(&path, w, cx)))
                        .child(
                            div()
                                .w(px(12.))
                                .flex_none()
                                .text_size(px(10.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if o.op == OutputOp::Written { emerald() } else { amber() })
                                .child(if o.op == OutputOp::Written { "W" } else { "E" }),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(rel_path(&o.path, &root)))
                        .when(o.count > 1, |d| d.child(muted(format!("×{}", o.count))))
                }))
                .into_any_element()
        };
        sections.push(self.section("Outputs", None, out_body, cx));

        // --- changes ---
        let changes = match self.git.get(&project.id).cloned() {
            None => muted("Loading…").into_any_element(),
            Some(g) if !g.is_repo => muted("Not a git repository.").into_any_element(),
            Some(g) => {
                let root = project.path.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_xs()
                            .child(icon("git-branch", muted_fg()))
                            .child(div().font_weight(FontWeight::MEDIUM).child(g.branch.clone().unwrap_or("—".into())))
                            .when(g.ahead > 0, |d| d.child(muted(format!("↑{}", g.ahead))))
                            .when(g.behind > 0, |d| d.child(muted(format!("↓{}", g.behind))))
                            .child(
                                div()
                                    .ml_auto()
                                    .flex()
                                    .gap(px(4.))
                                    .child(div().text_color(emerald()).child(format!("+{}", g.added)))
                                    .child(div().text_color(theme::red()).child(format!("−{}", g.removed))),
                            ),
                    )
                    .child(if g.files.is_empty() {
                        muted("No uncommitted changes.")
                    } else {
                        div().flex().flex_col().children(g.files.iter().take(60).enumerate().map(|(i, f)| {
                            let (mark, color) = match f.status {
                                GitStatus::Modified => ("M", amber()),
                                GitStatus::Added => ("A", emerald()),
                                GitStatus::Untracked => ("U", emerald()),
                                GitStatus::Deleted => ("D", theme::red()),
                                GitStatus::Renamed => ("R", sky()),
                                GitStatus::Conflict => ("!", red_strong()),
                            };
                            let full = format!("{}\\{}", root, f.path.replace('/', "\\"));
                            div()
                                .id(("chg", i))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .py(px(1.))
                                .text_xs()
                                .cursor_pointer()
                                .hover(|d| d.bg(hover()))
                                .on_click(cx.listener(move |this, _, w, cx| this.open_in_editor(&full, w, cx)))
                                .child(div().w(px(12.)).flex_none().text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(color).child(mark))
                                .child(div().flex_1().min_w_0().truncate().child(f.path.clone()))
                                .when(f.added.unwrap_or(0) + f.removed.unwrap_or(0) > 0, |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .gap(px(4.))
                                            .text_size(px(10.))
                                            .child(div().text_color(emerald()).child(format!("+{}", f.added.unwrap_or(0))))
                                            .child(div().text_color(theme::red()).child(format!("−{}", f.removed.unwrap_or(0)))),
                                    )
                                })
                        }))
                    })
                    .when_some(g.last_commit.clone(), |d, (hash, subject, at)| {
                        d.child(div().text_size(px(10.)).text_color(muted_fg()).truncate().child(format!("Last commit {hash} · {} · {subject}", time_ago(Some(at)))))
                    })
                    .into_any_element()
            }
        };
        sections.push(self.section("Changes", None, changes, cx));

        // --- connectors ---
        let conn_profile = profile.as_ref().map(|p| p.name.clone());
        if let Some(pn) = &conn_profile {
            if !self.collapsed.contains("Connectors") {
                self.load_mcp(pn, &project.id, false, cx);
            }
        }
        let key = conn_profile.as_ref().map(|pn| format!("{pn}|{}", project.path));
        let conn_body = match (conn_profile.as_ref(), key.as_ref().and_then(|k| self.mcp.get(k))) {
            (None, _) => muted("Open a Claude Code tab to see its connectors."),
            (Some(_), None) | (Some(_), Some(super::app::McpEntry { result: None, .. })) => muted("Checking connectors (Claude Code tests each one)…"),
            (Some(_), Some(super::app::McpEntry { result: Some(Err(e)), .. })) => div().text_size(px(11.)).text_color(theme::red()).child(e.clone()),
            (Some(_), Some(super::app::McpEntry { result: Some(Ok(list)), .. })) if list.is_empty() => muted("No MCP servers or connectors for this account here."),
            (Some(_), Some(super::app::McpEntry { result: Some(Ok(list)), .. })) => div().flex().flex_col().gap(px(4.)).children(list.iter().map(|m| {
                let color = match m.status {
                    McpStatus::Connected => emerald(),
                    McpStatus::NeedsAuth | McpStatus::Pending => amber(),
                    McpStatus::Failed => theme::red(),
                    _ => muted_fg(),
                };
                let label = match m.status {
                    McpStatus::Connected => "connected",
                    McpStatus::NeedsAuth => "needs auth",
                    McpStatus::Failed => "failed",
                    McpStatus::Pending => "pending",
                    McpStatus::Disabled => "disabled",
                    McpStatus::Unknown => "unknown",
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_xs()
                    .child(div().text_size(px(8.)).text_color(color).child("●"))
                    .child(div().flex_1().min_w_0().truncate().child(m.name.clone()))
                    .child(div().text_size(px(10.)).text_color(color).child(label))
            })),
        };
        sections.push(self.section("Connectors", None, conn_body.into_any_element(), cx));

        // --- history ---
        let open_ids: Vec<String> = self
            .tabs
            .iter()
            .filter(|t| t.exit_code.is_none())
            .filter_map(|t| self.tracker.get(&t.id).and_then(|s| s.session_id.clone()))
            .collect();
        let pid = project.id.clone();
        let hist_body = match self.history.get(&project.id).cloned() {
            None => muted("Loading…"),
            Some(items) if items.is_empty() => muted("No past sessions for this project yet."),
            Some(items) => div().flex().flex_col().gap(px(4.)).children(items.into_iter().enumerate().map(|(i, h)| {
                let is_open = open_ids.contains(&h.id);
                let pid = pid.clone();
                // Shared sessions open with any account (you pick); a session in a folder that isn't
                // shared yet only with its own.
                let accounts: Vec<String> = if h.profile.is_empty() { self.profiles.iter().map(|p| p.name.clone()).collect() } else { vec![h.profile.clone()] };
                let picking = self.history_pick.as_deref() == Some(h.id.as_str()) && !is_open;
                let hid = h.id.clone();
                let row = div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_xs()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().when(h.title.is_empty(), |d| d.text_color(muted_fg())).child(if h.title.is_empty() {
                                "Untitled session".to_string()
                            } else {
                                h.title.clone()
                            }))
                            .child(div().flex().items_center().gap(px(8.)).text_size(px(10.)).text_color(muted_fg()).when(!h.profile.is_empty(), |d| d.child(profile_tag(&h.profile))).child(time_ago(Some(h.modified_at)))),
                    )
                    .child(if is_open {
                        muted("open").into_any_element()
                    } else {
                        let (pid, hid, accounts) = (pid.clone(), hid.clone(), accounts.clone());
                        theme::icon_button(("resume", i), "play")
                            .border_1()
                            .border_color(if picking { primary() } else { border() })
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if accounts.len() == 1 {
                                    let mut req = OpenRequest::project(&pid, &accounts[0]);
                                    req.resume_session_id = Some(hid.clone());
                                    this.open_tab(req, w, cx);
                                } else {
                                    this.history_pick = if this.history_pick.as_deref() == Some(hid.as_str()) { None } else { Some(hid.clone()) };
                                    cx.notify();
                                }
                            }))
                            .into_any_element()
                    });
                div().flex().flex_col().gap(px(4.)).child(row).when(picking, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(10.))
                            .text_color(muted_fg())
                            .child("Open with")
                            .children(accounts.iter().enumerate().map(|(j, a)| {
                                let (pid, hid, a2) = (pid.clone(), hid.clone(), a.clone());
                                theme::btn_outline(SharedString::from(format!("hist-open-{i}-{j}")))
                                    .child(profile_tag(a))
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.history_pick = None;
                                        let mut req = OpenRequest::project(&pid, &a2);
                                        req.resume_session_id = Some(hid.clone());
                                        this.open_tab(req, w, cx);
                                    }))
                            })),
                    )
                })
            })),
        };
        sections.push(self.section("History", None, hist_body.into_any_element(), cx));

        let pid2 = project.id.clone();
        div()
            .id("panel")
            .w(px(self.panel_width()))
            .relative()
            .child(super::resize_handle("panel-resize", false).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.resizing = Some(super::app::Resize::Panel);
                cx.stop_propagation();
            })))
            .flex_none()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(border())
            .bg(sidebar().opacity(0.4))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .px(px(12.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(border())
                    .child(section_label("Session"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .when(can_handoff, |d| {
                                let from = panel_tab.as_ref().map(|t| t.id.clone()).unwrap_or_default();
                                d.child(theme::button_icon("handoff", "arrows-left-right", "Hand off").h(px(22.)).on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_handoff_dialog(&from);
                                    cx.notify();
                                })))
                            })
                            .child(theme::icon_button("panel-refresh", "arrow-clockwise").on_click(cx.listener(move |this, _, _, cx| {
                                this.load_git(&pid2, cx);
                                this.load_history(&pid2, cx);
                                if let Some(p) = this.active.get(&pid2).and_then(|id| this.tabs.iter().find(|t| &t.id == id)).map(|t| t.profile.clone()) {
                                    this.load_mcp(&p, &pid2, true, cx);
                                }
                            }))),
                    ),
            )
            .when(self.handoff.is_some(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .px(px(12.))
                        .py(px(8.))
                        .border_b_1()
                        .border_color(border())
                        .bg(amber().opacity(0.06))
                        .child(div().text_xs().text_color(amber()).child("Handoff in progress: Claude is updating docs/STATUS.md…"))
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .child(theme::button("handoff-now", "Open now").h(px(22.)).on_click(cx.listener(|this, _, w, cx| this.finish_handoff(w, cx))))
                                .child(theme::button("handoff-cancel", "Cancel").h(px(22.)).on_click(cx.listener(|this, _, _, cx| {
                                    this.handoff = None;
                                    cx.notify();
                                }))),
                        ),
                )
            })
            .child(div().px(px(12.)).py(px(12.)).border_b_1().border_color(border()).child(session))
            .children(sections)
            .into_any_element()
    }
}
