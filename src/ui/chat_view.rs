//! Draws a chat tab: messages, Markdown, tool cards, permission cards.

use super::app::LauncherApp;
use super::chat_store::{ChatItem, PermState, Tone};
use super::icons::icon;
use super::theme::{self, muted, muted_fg};
use crate::backend::types::ChatPermissionDecision;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::text::TextView;
use gpui_kit::*;
use gpui_kit::prelude::*;
use serde_json::Value;

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn base_name(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()
}

/// (glyph, title, detail) for a tool call.
fn tool_meta(name: &str, input: &Value) -> (&'static str, String, Option<String>) {
    let lines = |t: String| if t.is_empty() { 0 } else { t.split('\n').count() };
    match name {
        "Read" => ("file-text", format!("Read {}", base_name(&s(input, "file_path"))), Some(s(input, "file_path"))),
        "Write" => ("note-pencil", format!("Write {}", base_name(&s(input, "file_path"))), Some(format!("{} lines", lines(s(input, "content"))))),
        "Edit" => ("note-pencil", format!("Edit {}", base_name(&s(input, "file_path"))), Some(s(input, "file_path"))),
        "MultiEdit" => (
            "note-pencil",
            format!("Edit {}", base_name(&s(input, "file_path"))),
            Some(format!("{} edits", input.get("edits").and_then(|e| e.as_array()).map(|a| a.len()).unwrap_or(0))),
        ),
        "Bash" | "PowerShell" => ("terminal-window", Some(s(input, "description")).filter(|d| !d.is_empty()).unwrap_or("Run a command".into()), Some(s(input, "command"))),
        "Grep" => ("magnifying-glass", format!("Search for \"{}\"", s(input, "pattern")), Some(s(input, "path")).filter(|p| !p.is_empty()).or(Some(s(input, "glob")))),
        "Glob" => ("magnifying-glass", format!("Find files {}", s(input, "pattern")), Some(s(input, "path"))),
        "WebFetch" => ("globe", "Fetch a web page".into(), Some(s(input, "url"))),
        "WebSearch" => ("globe", format!("Web search \"{}\"", s(input, "query")), None),
        "Agent" | "Task" => (
            "robot",
            Some(s(input, "description")).filter(|d| !d.is_empty()).unwrap_or(format!("Subagent {}", s(input, "subagent_type"))),
            Some(s(input, "subagent_type")),
        ),
        "TodoWrite" | "TaskUpdate" => ("list-checks", "Update tasks".into(), None),
        "TaskCreate" => ("list-checks", format!("Task: {}", s(input, "subject")), None),
        _ => ("wrench", name.to_string(), None),
    }
}

fn pre(text: String, color: Hsla, max_h: f32) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("pre-{}", text.len())))
        .max_h(px(max_h))
        .overflow_y_scroll()
        .bg(theme::background())
        .p(px(8.))
        .font_family(theme::MONO)
        .text_size(px(11.))
        .text_color(color)
        .child(text)
}

fn diff(old: &str, new: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .bg(theme::background())
        .p(px(8.))
        .font_family(theme::MONO)
        .text_size(px(11.))
        .children(old.split('\n').map(|l| div().bg(theme::red_strong().opacity(0.10)).text_color(theme::c(0xfca5a5)).child(format!("− {l}"))))
        .children(new.split('\n').map(|l| div().bg(theme::emerald().opacity(0.10)).text_color(theme::c(0x6ee7b7)).child(format!("+ {l}"))))
}

fn tool_body(name: &str, input: &Value, result: Option<(&str, bool)>) -> Div {
    let mut d = div().flex().flex_col().gap(px(8.)).border_t_1().border_color(theme::border()).p(px(8.));
    match name {
        "Edit" => d = d.child(diff(&s(input, "old_string"), &s(input, "new_string"))),
        "MultiEdit" => {
            for e in input.get("edits").and_then(|e| e.as_array()).cloned().unwrap_or_default() {
                d = d.child(diff(&s(&e, "old_string"), &s(&e, "new_string")));
            }
        }
        "Write" => d = d.child(pre(s(input, "content").chars().take(8000).collect(), theme::foreground(), 320.)),
        "Bash" | "PowerShell" => d = d.child(pre(format!("$ {}", s(input, "command")), theme::c(0xbae6fd), 200.)),
        _ => d = d.child(pre(serde_json::to_string_pretty(input).unwrap_or_default(), muted_fg(), 192.)),
    }
    if let Some((text, is_err)) = result {
        let mut t: String = text.chars().take(12000).collect();
        if text.len() > 12000 {
            t.push_str("\n…");
        }
        if t.is_empty() {
            t = "(no output)".into();
        }
        d = d.child(pre(t, if is_err { theme::c(0xfca5a5) } else { theme::foreground() }, 320.));
    }
    d
}

fn markdown(id: impl Into<ElementId>, text: &str) -> impl IntoElement {
    div().text_sm().child(TextView::markdown(id, keep_line_breaks(text)).selectable(true))
}

/// Claude means a single line break as a line break (as Claude Code shows it), but Markdown joins
/// such lines into one paragraph. Turns them into hard breaks, outside code fences and tables.
fn keep_line_breaks(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::with_capacity(text.len() + lines.len() * 2);
    let mut in_fence = false;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let fence = t.starts_with("```") || t.starts_with("~~~");
        if fence {
            in_fence = !in_fence;
        }
        out.push_str(line);
        if i + 1 == lines.len() {
            break;
        }
        let next = lines[i + 1].trim_start();
        let soft = !in_fence
            && !fence
            && !t.is_empty()
            && !next.is_empty()
            && !t.starts_with('|')
            && !t.starts_with('#')
            && !line.ends_with("  ")
            && !line.ends_with('\\');
        if soft {
            out.push_str("  ");
        }
        out.push('\n');
    }
    out
}


impl LauncherApp {
    fn decide(&mut self, tab: &str, request_id: &str, d: ChatPermissionDecision, cx: &mut Context<Self>) {
        if let Some(name) = self.chat.reply_permission(tab, request_id, d) {
            if let Some(c) = self.chats.get_mut(tab) {
                c.permission_done(request_id, name);
            }
        }
        cx.notify();
    }

    fn reason_input(&mut self, item_id: u64, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        self.perm_inputs
            .entry(item_id)
            .or_insert_with(|| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)))
            .clone()
    }

    pub fn render_chat(&mut self, tab: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.chats.get(tab) else { return div().into_any_element() };
        let items = state.items.clone();
        let busy = state.busy;
        let scroll = self.chat_scroll.entry(tab.into()).or_insert_with(ScrollHandle::new).clone();

        let mut body: Vec<AnyElement> = vec![];
        if items.is_empty() {
            body.push(div().py(px(48.)).flex().justify_center().child(muted("Ask Claude anything about this project.")).into_any_element());
        }
        for it in items {
            body.push(self.render_item(tab, it, window, cx));
        }
        if busy {
            body.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_xs()
                    .text_color(theme::sky())
                    .child(theme::spinner(theme::sky()))
                    .child("Working…")
                    .into_any_element(),
            );
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(SharedString::from(format!("chat-{tab}")))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .child(div().flex().justify_center().child(div().w_full().max_w(px(theme::CHAT_COLUMN)).flex().flex_col().gap(px(12.)).px(px(16.)).py(px(16.)).children(body))),
            )
            .into_any_element()
    }

    fn render_item(&mut self, tab: &str, item: ChatItem, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match item {
            ChatItem::User { text, images, image_count, .. } => div()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(relative(0.8))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .border_1()
                        .border_color(theme::border())
                        .bg(theme::secondary())
                        .px(px(12.))
                        .py(px(8.))
                        .when(!images.is_empty(), |d| {
                            d.child(div().flex().flex_wrap().gap(px(8.)).children(images.into_iter().map(|p| {
                                div().h(px(80.)).w(px(112.)).border_1().border_color(theme::border()).overflow_hidden().child(
                                    img(std::path::PathBuf::from(p)).size_full().object_fit(ObjectFit::Cover),
                                )
                            })))
                        })
                        .when(image_count > 0, |d| d.child(muted(format!("{image_count} image(s)"))))
                        .when(!text.is_empty(), |d| d.child(div().text_sm().child(text))),
                )
                .into_any_element(),
            ChatItem::Text { id, text, streaming, .. } => div()
                .flex()
                .flex_col()
                .child(markdown(SharedString::from(format!("md-{tab}-{id}")), &text))
                .when(streaming, |d| d.child(div().w(px(6.)).h(px(12.)).bg(theme::foreground().opacity(0.7))))
                .into_any_element(),
            ChatItem::Thinking { text, streaming: false, .. } if text.is_empty() => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_xs()
                .text_color(muted_fg())
                .child(icon("brain", muted_fg()))
                .child("Thought (text not saved by Claude Code)")
                .into_any_element(),
            ChatItem::Thinking { id, text, streaming, open, .. } => {
                let tab = tab.to_string();
                div()
                    .flex()
                    .flex_col()
                    .text_xs()
                    .text_color(muted_fg())
                    .child(
                        div()
                            .id(("think", id))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icon("brain", muted_fg()))
                            .child(if streaming { "Thinking…" } else { "Thought" })
                            .child(icon(if open { "caret-down" } else { "caret-right" }, muted_fg()).size(px(12.)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(c) = this.chats.get_mut(&tab) {
                                    for x in &mut c.items {
                                        if let ChatItem::Thinking { id: i, open, .. } = x {
                                            if *i == id {
                                                *open = !*open;
                                            }
                                        }
                                    }
                                }
                                cx.notify();
                            })),
                    )
                    .when(open, |d| d.child(div().mt(px(4.)).border_l_1().border_color(theme::border()).pl(px(12.)).child(text)))
                    .into_any_element()
            }
            ChatItem::Tool { id, name, input, result, steps, open, .. } => {
                let (glyph, title, detail) = tool_meta(&name, &input);
                let status = match &result {
                    None => div().child(theme::spinner(theme::sky())),
                    Some(r) if r.is_error => div().child(icon("x-circle-fill", theme::red())),
                    Some(_) => div().child(icon("check-circle-fill", theme::emerald())),
                };
                let tab = tab.to_string();
                div()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::card().opacity(0.4))
                    .child(
                        div()
                            .id(("tool", id))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(8.))
                            .py(px(6.))
                            .text_xs()
                            .cursor_pointer()
                            .hover(|d| d.bg(theme::hover()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(c) = this.chats.get_mut(&tab) {
                                    for x in &mut c.items {
                                        if let ChatItem::Tool { id: i, open, .. } = x {
                                            if *i == id {
                                                *open = !*open;
                                            }
                                        }
                                    }
                                }
                                cx.notify();
                            }))
                            .child(icon(if open { "caret-down" } else { "caret-right" }, muted_fg()).size(px(12.)))
                            .child(icon(glyph, muted_fg()))
                            .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(title))
                            .when_some(detail.filter(|d| !d.is_empty()), |d, det| d.child(div().min_w_0().truncate().text_color(muted_fg()).child(det)))
                            .when(steps > 0, |d| d.child(div().flex_none().text_size(px(10.)).text_color(muted_fg()).child(format!("{steps} steps"))))
                            .child(div().ml_auto().flex_none().child(status)),
                    )
                    .when(open, |d| d.child(tool_body(&name, &input, result.as_ref().map(|r| (r.text.as_str(), r.is_error)))))
                    .into_any_element()
            }
            ChatItem::Permission { id, request, state, answers } => self.render_permission(tab, id, request, state, answers, window, cx),
            ChatItem::Notice { text, tone, .. } => div()
                .text_size(px(11.))
                .text_color(match tone {
                    Tone::Error => theme::red(),
                    Tone::Warn => theme::amber(),
                    Tone::Info => muted_fg(),
                })
                .flex()
                .items_center()
                .gap(px(6.))
                .child(icon("warning-circle", match tone { Tone::Error => theme::red(), Tone::Warn => theme::amber(), Tone::Info => muted_fg() }))
                .child(text)
                .into_any_element(),
            ChatItem::Result { duration_ms, cost_usd, is_error, subtype, denials, .. } => {
                let mut line = if is_error { format!("Stopped: {}", subtype.unwrap_or_default()) } else { "Done".into() };
                if let Some(d) = duration_ms {
                    line.push_str(&format!(" · {:.1} s", d / 1000.));
                }
                if let Some(c) = cost_usd {
                    line.push_str(&format!(" · ≈ ${c:.2} API-price"));
                }
                let mut uniq: Vec<String> = vec![];
                for d in denials {
                    if !uniq.contains(&d) {
                        uniq.push(d);
                    }
                }
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .text_size(px(10.))
                    .text_color(muted_fg())
                    .child(line)
                    .when(!uniq.is_empty(), |d| {
                        d.child(div().text_color(theme::amber()).child(format!(
                            "Not allowed: {}. Change the permission mode above, or allow it when asked.",
                            uniq.join(", ")
                        )))
                    })
                    .into_any_element()
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_permission(
        &mut self,
        tab: &str,
        item_id: u64,
        req: crate::backend::types::ChatPermissionRequest,
        state: PermState,
        answers: Vec<(String, Vec<String>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pending = state == PermState::Pending;
        let request_id = req.request_id.clone();

        if req.tool_name == "AskUserQuestion" {
            let questions = req.input.get("questions").and_then(|q| q.as_array()).cloned().unwrap_or_default();
            let chosen = |q: &str| answers.iter().find(|(k, _)| k == q).map(|(_, v)| v.clone()).unwrap_or_default();
            let complete = questions.iter().all(|q| !chosen(&s(q, "question")).is_empty());
            let mut card = div().flex().flex_col().gap(px(12.)).border_1().border_color(theme::sky().opacity(0.4)).bg(theme::sky().opacity(0.05)).p(px(12.));
            for (qi, q) in questions.iter().enumerate() {
                let question = s(q, "question");
                let multi = q.get("multiSelect").and_then(|m| m.as_bool()) == Some(true);
                let sel = chosen(&question);
                let opts = q.get("options").and_then(|o| o.as_array()).cloned().unwrap_or_default();
                card = card.child(
                    div().flex().flex_col().gap(px(6.)).child(div().text_xs().font_weight(FontWeight::MEDIUM).child(question.clone())).children(
                        opts.into_iter().enumerate().map(|(oi, o)| {
                            let label = s(&o, "label");
                            let desc = s(&o, "description");
                            let on = sel.contains(&label);
                            let tab = tab.to_string();
                            let question = question.clone();
                            let label2 = label.clone();
                            div()
                                .id(SharedString::from(format!("q-{item_id}-{qi}-{oi}")))
                                .flex()
                                .flex_col()
                                .px(px(8.))
                                .py(px(4.))
                                .border_1()
                                .border_color(if on { theme::sky() } else { theme::border() })
                                .when(on, |d| d.bg(theme::sky().opacity(0.1)))
                                .text_xs()
                                .when(pending, |d| d.cursor_pointer().hover(|d| d.border_color(theme::muted_fg())))
                                .child(label)
                                .when(!desc.is_empty(), |d| d.child(muted(desc)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !pending {
                                        return;
                                    }
                                    if let Some(c) = this.chats.get_mut(&tab) {
                                        for x in &mut c.items {
                                            if let ChatItem::Permission { id, answers, .. } = x {
                                                if *id == item_id {
                                                    let entry = match answers.iter_mut().find(|(k, _)| *k == question) {
                                                        Some(e) => e,
                                                        None => {
                                                            answers.push((question.clone(), vec![]));
                                                            answers.last_mut().unwrap()
                                                        }
                                                    };
                                                    if multi {
                                                        if let Some(p) = entry.1.iter().position(|x| *x == label2) {
                                                            entry.1.remove(p);
                                                        } else {
                                                            entry.1.push(label2.clone());
                                                        }
                                                    } else {
                                                        entry.1 = vec![label2.clone()];
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    cx.notify();
                                }))
                        }),
                    ),
                );
            }
            if pending {
                let (tab1, tab2) = (tab.to_string(), tab.to_string());
                let (rid1, rid2) = (request_id.clone(), request_id.clone());
                let input = req.input.clone();
                let qs = questions.clone();
                card = card.child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(theme::disabled(
                            theme::button_primary(SharedString::from(format!("ans-{item_id}")), "Answer").on_click(cx.listener(move |this, _, _, cx| {
                                if !complete {
                                    return;
                                }
                                let mut out = serde_json::Map::new();
                                for q in &qs {
                                    let question = s(q, "question");
                                    let multi = q.get("multiSelect").and_then(|m| m.as_bool()) == Some(true);
                                    let sel = answers.iter().find(|(k, _)| *k == question).map(|(_, v)| v.clone()).unwrap_or_default();
                                    out.insert(question, if multi { serde_json::json!(sel) } else { serde_json::json!(sel.first()) });
                                }
                                let mut updated = input.clone();
                                updated["answers"] = Value::Object(out);
                                this.decide(&tab1, &rid1, ChatPermissionDecision::Allow { updated_input: Some(updated), updated_permissions: None }, cx);
                            })),
                            !complete,
                        ))
                        .child(theme::button(SharedString::from(format!("skip-{item_id}")), "Skip").on_click(cx.listener(move |this, _, _, cx| {
                            this.decide(&tab2, &rid2, ChatPermissionDecision::Deny { message: Some("The user skipped the question.".into()), interrupt: false }, cx);
                        }))),
                );
            } else {
                card = card.child(muted(if state == PermState::Allow { "Answered." } else { "Skipped." }));
            }
            return card.into_any_element();
        }

        let is_plan = req.tool_name == "ExitPlanMode";
        let (glyph, title, detail) = tool_meta(&req.tool_name, &req.input);
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .border_1()
            .when(pending, |d| d.border_color(theme::amber().opacity(0.5)).bg(theme::amber().opacity(0.05)))
            .when(!pending, |d| d.border_color(theme::border()).opacity(0.7))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_xs()
                    .child(icon("shield-warning", theme::amber()).size(px(16.)))
                    .child(div().font_weight(FontWeight::MEDIUM).child(if is_plan { "Claude has a plan. Approve it?" } else { "Claude wants to:" }))
                    .when(!is_plan, |d| d.child(icon(glyph, theme::foreground())).child(div().min_w_0().truncate().child(title))),
            );
        if is_plan {
            card = card.child(
                div()
                    .id(("plan", item_id))
                    .max_h(px(384.))
                    .overflow_y_scroll()
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::background())
                    .p(px(8.))
                    .child(markdown(SharedString::from(format!("plan-{item_id}")), &s(&req.input, "plan"))),
            );
        } else {
            if let Some(d) = detail.filter(|d| !d.is_empty()) {
                card = card.child(pre(d, theme::foreground(), 160.));
            }
            if matches!(req.tool_name.as_str(), "Edit" | "MultiEdit" | "Write") {
                card = card.child(tool_body(&req.tool_name, &req.input, None));
            }
        }
        if pending {
            let reason = self.reason_input(item_id, if is_plan { "What to change (optional)" } else { "Why not / what instead (optional)" }, window, cx);
            let (t1, t2, t3) = (tab.to_string(), tab.to_string(), tab.to_string());
            let (r1, r2, r3) = (request_id.clone(), request_id.clone(), request_id.clone());
            let input1 = req.input.clone();
            let input2 = req.input.clone();
            let sugg = req.suggestions.clone();
            let reason2 = reason.clone();
            card = card.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(theme::button_primary(SharedString::from(format!("allow-{item_id}")), if is_plan { "Approve plan" } else { "Allow" }).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.decide(&t1, &r1, ChatPermissionDecision::Allow { updated_input: Some(input1.clone()), updated_permissions: None }, cx)
                        }),
                    ))
                    .when(!is_plan && !sugg.is_empty(), |d| {
                        d.child(theme::button(SharedString::from(format!("always-{item_id}")), "Always allow").on_click(cx.listener(move |this, _, _, cx| {
                            this.decide(&t2, &r2, ChatPermissionDecision::Allow { updated_input: Some(input2.clone()), updated_permissions: Some(sugg.clone()) }, cx)
                        })))
                    })
                    .child(div().flex_1().min_w(px(160.)).child(Input::new(&reason)))
                    .child(theme::button_danger(SharedString::from(format!("deny-{item_id}")), if is_plan { "Keep planning" } else { "Deny" }).on_click(
                        cx.listener(move |this, _, _, cx| {
                            let text = reason2.read(cx).value().trim().to_string();
                            let msg = if !text.is_empty() {
                                text
                            } else if is_plan {
                                "Keep planning; the user did not approve this plan yet.".into()
                            } else {
                                "The user denied this.".into()
                            };
                            this.decide(&t3, &r3, ChatPermissionDecision::Deny { message: Some(msg), interrupt: false }, cx)
                        }),
                    )),
            );
        } else {
            card = card.child(muted(match state {
                PermState::Allow => "Allowed.",
                PermState::Deny => "Denied.",
                _ => "Timed out (denied).",
            }));
        }
        card.into_any_element()
    }
}
