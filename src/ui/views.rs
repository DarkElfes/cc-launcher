//! Dashboard, project header, Accounts and Settings pages.

use super::app::{LauncherApp, OpenRequest, TabKind, View};
use super::icons::icon;
use super::theme::{self, *};
use crate::backend::types::*;
use gpui_kit::component::input::Input;
use gpui_kit::*;
use gpui_kit::prelude::*;

/// Claude Code's voice-dictation languages (the `language` setting also sets the reply language).
const LANGUAGES: &[&str] = &[
    "ukrainian", "english", "russian", "polish", "german", "french", "spanish", "italian", "portuguese", "dutch", "czech", "danish",
    "norwegian", "swedish", "greek", "turkish", "hindi", "indonesian", "japanese", "korean",
];

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// One row of the language dropdown; `value` "" is the default (setting left unset).
#[derive(Clone)]
pub struct LangChoice {
    value: &'static str,
    title: SharedString,
}

impl gpui_kit::component::select::SelectItem for LangChoice {
    type Value = &'static str;
    fn value(&self) -> &&'static str {
        &self.value
    }
    fn title(&self) -> SharedString {
        self.title.clone()
    }
}

pub type LangSelect = gpui_kit::component::select::SelectState<Vec<LangChoice>>;

/// The language dropdown, with `current` selected.
pub fn lang_select(current: Option<&str>, window: &mut Window, cx: &mut Context<LangSelect>) -> LangSelect {
    let mut items = vec![LangChoice { value: "", title: "Default (not set: English for dictation)".into() }];
    items.extend(LANGUAGES.iter().map(|l| LangChoice { value: l, title: cap(l).into() }));
    let ix = current.and_then(|c| items.iter().position(|i| i.value == c)).unwrap_or(0);
    gpui_kit::component::select::SelectState::new(items, Some(gpui_kit::component::IndexPath::new(ix)), window, cx)
}

fn page_header(title: &str, description: Option<String>) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
        .when_some(description, |d, desc| d.child(div().mt(px(4.)).text_xs().text_color(muted_fg()).child(desc)))
}

/// A row of choice chips.
fn chip(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(id.into())
        .px(px(8.))
        .py(px(3.))
        .border_1()
        .border_color(if on { primary() } else { border() })
        .when(on, |d| d.bg(secondary()))
        .text_xs()
        .text_color(if on { foreground() } else { muted_fg() })
        .cursor_pointer()
        .hover(|d| d.text_color(foreground()))
        .child(label.into())
}

/// A choice chip with an outline icon before its label.
fn chip_icon(id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    let color = if on { foreground() } else { muted_fg() };
    let label: SharedString = label.into();
    chip(id, "", on).flex().items_center().gap(px(6.)).child(icon(icon_name, color)).child(label)
}

impl LauncherApp {
    pub fn render_dashboard(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let projects = self.projects_sorted();
        let running = |id: &str| self.tabs.iter().filter(|t| t.project_id.as_deref() == Some(id) && t.exit_code.is_none()).count();
        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(16.))
            .child(page_header("Projects", Some("Each project keeps its own state. Open it with any of your accounts.".into())))
            .child(theme::button_primary("add-project", "+ Add project").on_click(cx.listener(|this, _, w, cx| this.add_project(w, cx))));
        let body = if projects.is_empty() {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .py(px(48.))
                .border_1()
                .border_dashed()
                .border_color(border())
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("No projects yet"))
                .child(muted("Add an existing folder, or create a new one from the folder picker."))
                .child(theme::button_primary("add-project-2", "Add project").on_click(cx.listener(|this, _, w, cx| this.add_project(w, cx))))
        } else {
            div().flex().flex_wrap().gap(px(16.)).children(projects.into_iter().map(|p| {
                let n = running(&p.id);
                let id = p.id.clone();
                let exists = p.exists();
                div()
                    .id(SharedString::from(format!("card-{}", p.id)))
                    .w(px(300.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(16.))
                    .border_1()
                    .border_color(border())
                    .bg(card())
                    .cursor_pointer()
                    .when(!exists, |d| d.opacity(0.6))
                    .hover(|d| d.border_color(primary().opacity(0.4)))
                    .on_click(cx.listener(move |this, _, w, cx| this.set_view(View::Project(id.clone()), w, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon("folder-simple", muted_fg()))
                            .child(p.name.clone())
                            .when(n > 0, |d| d.child(div().ml_auto().child(badge(format!("{n} running"), sky())))),
                    )
                    .child(div().text_xs().text_color(muted_fg()).truncate().child(p.path.clone()))
                    .child(if exists {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .text_xs()
                            .text_color(muted_fg())
                            .child(format!("Opened {}", time_ago(p.last_opened_at)))
                            .when_some(p.last_profile.clone(), |d, lp| d.child(profile_tag(&lp)))
                    } else {
                        div().child(badge("folder not found", theme::red()))
                    })
            }))
        };
        div()
            .id("dashboard")
            .flex_1()
            .overflow_y_scroll()
            .p(px(24.))
            .child(div().flex().flex_col().gap(px(24.)).child(header).child(body))
            .into_any_element()
    }

    pub fn render_project_header(&mut self, project: &Project, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.store.settings.tab_mode.unwrap_or_default();
        let path = project.path.clone();
        let path2 = project.path.clone();
        let id = project.id.clone();
        let mut launch = div().flex().flex_wrap().items_center().gap(px(8.));
        if !project.exists() {
            launch = launch.child(div().text_xs().text_color(theme::red()).child("The folder no longer exists. Remove the project or restore the folder."));
        } else {
            launch = launch
                .child(
                    div()
                        .flex()
                        .border_1()
                        .border_color(border())
                        .child(chip_icon("mode-chat", "chat-circle", "Chat", mode == TabMode::Chat).border_0().on_click(cx.listener(|this, _, _, cx| this.set_tab_mode(TabMode::Chat, cx))))
                        .child(
                            chip_icon("mode-term", "terminal-window", "Terminal", mode == TabMode::Terminal)
                                .border_0()
                                .on_click(cx.listener(|this, _, _, cx| this.set_tab_mode(TabMode::Terminal, cx))),
                        ),
                )
                .child(muted(if self.profiles.is_empty() { "No accounts yet. Add one under Accounts first." } else { "with" }));
            for p in self.profiles.clone() {
                let hot = usage_hot_pct(p.usage.as_ref()) >= 90.;
                let last = project.last_profile.as_deref() == Some(p.name.as_str());
                let pid = id.clone();
                let name = p.name.clone();
                launch = launch.child(
                    theme::btn_outline(SharedString::from(format!("launch-{}", p.name)))
                        .child(icon("play", foreground()).size(px(12.)))
                        .when(last, |d| d.border_color(primary()))
                        .when(hot, |d| d.border_color(red_strong().opacity(0.6)))
                        .child(profile_tag(&p.name))
                        .children(usage_inline(p.usage.as_ref()))
                        .when(!p.signed_in, |d| d.child(badge("login", amber())))
                        .on_click(cx.listener(move |this, _, w, cx| this.open_tab(OpenRequest::project(&pid, &name), w, cx))),
                );
            }
        }
        let remove_id = id.clone();
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(16.))
            .p(px(24.))
            .pb(px(16.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.))
                    .child(page_header(&project.name, Some(project.path.clone())))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(theme::button("explorer", "Explorer").on_click(cx.listener(move |this, _, w, cx| this.open_folder(&path, w, cx))))
                            .child(theme::button("editor", "Editor").on_click(cx.listener(move |this, _, w, cx| this.open_in_editor(&path2, w, cx))))
                            .child(theme::button_danger("remove", "Remove").on_click(cx.listener(move |this, _, w, cx| this.remove_project(&remove_id, w, cx)))),
                    ),
            )
            .child(launch)
            .into_any_element()
    }

    pub fn render_accounts(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let refreshing = self.refreshing;
        let rows = self.profiles.clone().into_iter().map(|p| {
            let name = p.name.clone();
            let name2 = p.name.clone();
            let name3 = p.name.clone();
            let dir = p.dir.clone();
            let status = if p.error.is_some() {
                "refresh failed".to_string()
            } else if let Some(u) = &p.usage {
                format!("updated {}", time_ago(Some(u.updated_at)))
            } else if p.signed_in {
                "loading usage…".into()
            } else {
                "sign in to see usage".into()
            };
            let account = match &p.account {
                None => muted("checking account…"),
                Some(a) if !a.logged_in => div().text_size(px(10.)).text_color(amber()).child("not signed in"),
                Some(a) => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .text_color(muted_fg())
                    .child(div().truncate().child(a.email.clone().unwrap_or("unknown email".into())))
                    .when_some(a.plan.clone(), |d, plan| d.child(badge(plan.to_uppercase(), muted_fg()))),
            };
            div()
                .flex()
                .items_center()
                .gap(px(16.))
                .px(px(12.))
                .py(px(10.))
                .border_b_1()
                .border_color(border())
                .child(
                    div()
                        .w(px(240.))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(div().flex().items_center().gap(px(8.)).child(profile_tag(&p.name)).when(!p.signed_in, |d| d.child(badge("not signed in", amber()))))
                        .child(account)
                        .child(div().text_size(px(10.)).text_color(muted_fg()).child(status)),
                )
                .child(div().flex_1().child(usage_meter(p.usage.as_ref().and_then(|u| u.five_hour.as_ref()))))
                .child(div().flex_1().child(usage_meter(p.usage.as_ref().and_then(|u| u.week.as_ref()))))
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            theme::button(SharedString::from(format!("signin-{name}")), if p.signed_in { "Open" } else { "Sign in" }).on_click(cx.listener(
                                move |this, _, w, cx| {
                                    let mut req = OpenRequest::project("", &name);
                                    req.kind = TabKind::Login;
                                    req.project_id = None;
                                    this.open_tab(req, w, cx)
                                },
                            )),
                        )
                        .child(
                            theme::btn_outline(SharedString::from(format!("folder-{name2}")))
                                .child(icon("folder-open", foreground()))
                                .on_click(cx.listener(move |this, _, w, cx| this.open_folder(&dir, w, cx))),
                        )
                        .child(
                            theme::btn_danger(SharedString::from(format!("rm-{name3}")))
                                .child(icon("trash", theme::red()))
                                .on_click(cx.listener(move |this, _, w, cx| this.remove_profile(&name3, w, cx))),
                        ),
                )
        });
        div()
            .id("accounts")
            .flex_none()
            .max_h(relative(0.55))
            .overflow_y_scroll()
            .p(px(24.))
            .pb(px(16.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(16.))
                            .child(page_header(
                                "Accounts",
                                Some("One profile per Claude account, each in its own folder under ~/.claude-profiles. The normal ~/.claude folder isn't used, so `claude` in other terminals can never switch an account here.".into()),
                            ))
                            .child(theme::disabled(
                                theme::button_icon("refresh", "arrow-clockwise", if refreshing { "Refreshing…" } else { "Refresh" })
                                    .on_click(cx.listener(|this, _, _, cx| this.refresh_accounts(None, cx))),
                                refreshing,
                            )),
                    )
                    .when(self.profiles.is_empty(), |d| d.child(div().text_xs().text_color(amber()).child("No accounts yet. Add one below, then click Sign in.")))
                    .child(
                        div()
                            .border_1()
                            .border_color(border())
                            .child(
                                div()
                                    .flex()
                                    .gap(px(16.))
                                    .px(px(12.))
                                    .py(px(6.))
                                    .border_b_1()
                                    .border_color(border())
                                    .text_xs()
                                    .text_color(muted_fg())
                                    .child(div().w(px(240.)).child("Account"))
                                    .child(div().flex_1().child("5-hour limit"))
                                    .child(div().flex_1().child("Weekly · all models"))
                                    .child(div().w(px(150.))),
                            )
                            .children(rows),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(div().w(px(288.)).child(Input::new(&self.profile_name)))
                            .child(theme::button_primary("add-profile", "+ Add profile").on_click(cx.listener(|this, _, w, cx| this.add_profile(w, cx)))),
                    )
                    .child(muted("To sign in, click Sign in. Claude Code opens below; follow its login steps (type /login if it doesn't ask)."))
                    .child(muted(
                        "Email and plan come from `claude auth status`. Limits come from Claude Code's own /usage, which runs locally and doesn't use your limits. They refresh every 5 minutes, and live from the status line while a session is running.",
                    )),
            )
            .into_any_element()
    }

    pub fn render_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let claude_now = self.claude_path().unwrap_or("not found".into());
        let voice = self.set_voice;
        let field = |label: &str, control: AnyElement, desc: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().text_xs().font_weight(FontWeight::MEDIUM).child(label.to_string()))
                .child(control)
                .child(div().text_size(px(11.)).text_color(muted_fg()).child(desc))
        };
        let langs = div().w(px(320.)).child(gpui_kit::component::select::Select::new(&self.lang_select).menu_max_h(px(320.)));
        let voice_opts: [(Option<VoiceMode>, &str); 4] = [
            (None, "Default (as each account has it; off unless turned on with /voice)"),
            (Some(VoiceMode::Tap), "On: tap (mic button or Ctrl+Space: start, again to send)"),
            (Some(VoiceMode::Hold), "On: hold Space while talking"),
            (Some(VoiceMode::Off), "Off"),
        ];
        let voices = div().flex().flex_wrap().gap(px(4.)).children(voice_opts.into_iter().enumerate().map(|(i, (v, label))| {
            chip(("voice", i), label, voice == v).on_click(cx.listener(move |this, _, _, cx| {
                this.set_voice = v;
                this.settings_saved = false;
                cx.notify();
            }))
        }));

        let update_text = match &self.update {
            UpdateState::Disabled => "Auto-update is off in this build (development build).".to_string(),
            UpdateState::Checking => "Checking for updates…".into(),
            UpdateState::None => "You have the latest version.".into(),
            UpdateState::Downloading { version, percent } => format!("Downloading {version}… {percent}%"),
            UpdateState::Ready { version } => format!("Version {version} is downloaded and ready to install."),
            UpdateState::Error(e) => format!("Update check failed: {e}"),
            UpdateState::Idle => "Updates are checked at startup and every 4 hours.".into(),
        };
        let ready = matches!(self.update, UpdateState::Ready { .. });
        let busy = matches!(self.update, UpdateState::Disabled | UpdateState::Checking | UpdateState::Downloading { .. });

        div()
            .id("settings")
            .flex_1()
            .overflow_y_scroll()
            .p(px(24.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(24.))
                    .max_w(px(640.))
                    .child(page_header("Settings", None))
                    .child(field("Claude Code executable", Input::new(&self.set_claude).into_any_element(), format!("Currently using: {claude_now}")))
                    .child(field("Editor command", Input::new(&self.set_editor).into_any_element(), "Runs as: <command> \"<project folder>\"".into()))
                    .child(field(
                        "Claude language (replies and voice)",
                        langs.into_any_element(),
                        "Applied to every account on Save. Claude replies in this language, and voice dictation recognizes it. Default removes the setting: Claude answers in the language you write in, and dictation expects English.".into(),
                    ))
                    .child(field(
                        "Voice dictation",
                        voices.into_any_element(),
                        "Uses Claude Code's own dictation (doesn't count toward your limits). Applies to sessions started after saving. Windows must allow microphone access for desktop apps.".into(),
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(theme::button_primary("save", "Save").on_click(cx.listener(|this, _, w, cx| this.save_settings(w, cx))))
                            .when(self.settings_saved, |d| d.child(muted("Saved."))),
                    )
                    .child(field(
                        &format!("Version {}", env!("CARGO_PKG_VERSION")),
                        div()
                            .flex()
                            .child(if ready {
                                theme::button_primary("install", "Restart and update").on_click(cx.listener(|this, _, w, cx| this.install_update(w, cx)))
                            } else {
                                theme::disabled(
                                    theme::button("check", "Check for updates").on_click(cx.listener(move |this, _, _, cx| {
                                        if !busy {
                                            this.check_updates(cx)
                                        }
                                    })),
                                    busy,
                                )
                            })
                            .into_any_element(),
                        update_text,
                    )),
            )
            .into_any_element()
    }
}
