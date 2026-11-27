//! Colors and small shared elements. Dark only, close to the Electron build (shadcn neutral, lyra).

use super::icons::icon;
use crate::backend::profiles::profile_hue;
use crate::backend::types::{now_ms, Activity, ProfileUsage, UsageWindow};
use gpui_kit::*;
use std::time::Duration;

pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

// Neutral dark grey (not pure black) with soft grey text (not pure white), so text doesn't glare.
pub fn background() -> Hsla { c(0x1c1c1c) }
pub fn foreground() -> Hsla { c(0xd4d4d4) }
pub fn muted_fg() -> Hsla { c(0x8f8f8f) }
pub fn border() -> Hsla { hsla(0., 0., 1., 0.09) }
pub fn input_border() -> Hsla { hsla(0., 0., 1., 0.14) }
pub fn input_bg() -> Hsla { hsla(0., 0., 1., 0.04) }
pub fn sidebar() -> Hsla { c(0x161616) }
pub fn card() -> Hsla { c(0x232323) }
pub fn secondary() -> Hsla { c(0x2b2b2b) }
pub fn hover() -> Hsla { hsla(0., 0., 1., 0.06) }
pub fn primary() -> Hsla { c(0xd4d4d4) }
pub fn primary_fg() -> Hsla { c(0x1c1c1c) }
pub fn sky() -> Hsla { c(0x38bdf8) }
pub fn amber() -> Hsla { c(0xfbbf24) }
pub fn emerald() -> Hsla { c(0x34d399) }
pub fn red() -> Hsla { c(0xf87171) }
pub fn red_strong() -> Hsla { c(0xef4444) }

pub const MONO: &str = "Cascadia Mono";
/// Inter, embedded in the exe (assets/fonts, SIL OFL) and registered by `load_fonts`.
pub const UI_FONT: &str = "Inter";
/// Width of the chat message column; the chat message box lines up with it.
pub const CHAT_COLUMN: f32 = 820.;

#[derive(rust_embed::RustEmbed)]
#[folder = "assets/fonts"]
#[include = "*.ttf"]
struct Fonts;

/// Gives gpui-component's own widgets (inputs, Markdown, menus, notifications) our fonts and
/// colors, so they match the rest of the app instead of its default black/white theme.
pub fn apply_component_theme(cx: &mut App) {
    let t = gpui_kit::component::Theme::global_mut(cx);
    t.font_family = UI_FONT.into();
    t.mono_font_family = MONO.into();
    let k = &mut t.colors;
    k.background = background();
    k.foreground = foreground();
    k.border = border();
    k.input = input_border();
    k.muted = secondary();
    k.muted_foreground = muted_fg();
    k.popover = card();
    k.popover_foreground = foreground();
    k.secondary = secondary();
    k.secondary_foreground = foreground();
    k.primary = primary();
    k.primary_foreground = primary_fg();
    k.accent = secondary();
    k.sidebar = sidebar();
    k.sidebar_foreground = foreground();
    k.title_bar = sidebar();
    k.list = background();
    k.table = background();
}

/// Registers the embedded Inter faces with the text system.
pub fn load_fonts(cx: &mut App) {
    let fonts: Vec<std::borrow::Cow<'static, [u8]>> = Fonts::iter().filter_map(|f| Fonts::get(&f).map(|e| e.data)).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("could not load Inter: {e}");
    }
}

/// Stable color per profile name so each account is easy to recognise.
pub fn profile_color(name: &str) -> Hsla {
    hsla(profile_hue(name) / 360., 0.70, 0.62, 1.)
}

/// Colored square + name.
pub fn profile_tag(name: &str) -> Div {
    let color = profile_color(name);
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(div().size(px(8.)).flex_none().bg(color))
        .child(name.to_string())
}

/// Small outlined label.
pub fn badge(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .px(px(6.))
        .h(px(18.))
        .border_1()
        .border_color(color.opacity(0.45))
        .text_color(color)
        .text_size(px(10.))
        .child(text.into())
}

/// A spinning Phosphor spinner.
pub fn spinner(color: Hsla) -> impl IntoElement {
    icon("spinner", color).size(px(12.)).with_animation(
        "spin",
        Animation::new(Duration::from_millis(900)).repeat(),
        |s, delta| s.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

fn btn_base(id: ElementId) -> Stateful<Div> {
    div().id(id).flex().flex_none().items_center().justify_center().gap(px(6.)).px(px(10.)).h(px(28.)).text_xs().cursor_pointer()
}

/// Outline button (shadcn "outline").
pub fn btn_outline(id: impl Into<ElementId>) -> Stateful<Div> {
    btn_base(id.into())
        .border_1()
        .border_color(input_border())
        .bg(input_bg())
        .text_color(foreground())
        .hover(|s| s.bg(hover()))
}

pub fn btn_primary(id: impl Into<ElementId>) -> Stateful<Div> {
    btn_base(id.into()).bg(primary()).font_weight(FontWeight::MEDIUM).text_color(primary_fg()).hover(|s| s.opacity(0.9))
}

pub fn btn_danger(id: impl Into<ElementId>) -> Stateful<Div> {
    btn_base(id.into()).bg(red_strong().opacity(0.15)).text_color(red()).hover(|s| s.bg(red_strong().opacity(0.25)))
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    btn_outline(id).child(label.into())
}

pub fn button_primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    btn_primary(id).child(label.into())
}

pub fn button_danger(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    btn_danger(id).child(label.into())
}

/// Outline button with an icon before the label.
pub fn button_icon(id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>) -> Stateful<Div> {
    btn_outline(id).child(icon(icon_name, foreground())).child(label.into())
}

/// Small square ghost button with an icon.
pub fn icon_button(id: impl Into<ElementId>, icon_name: &str) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(24.))
        .cursor_pointer()
        .hover(|s| s.bg(hover()))
        .child(icon(icon_name, muted_fg()))
}

/// Hover hint for an element (icon-only buttons).
pub fn tip(el: Stateful<Div>, text: &'static str) -> Stateful<Div> {
    el.tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(text).build(w, cx))
}

/// Bordered icon-only button with a hover hint.
pub fn icon_btn_tip(id: impl Into<ElementId>, icon_name: &str, color: Hsla, text: &'static str) -> Stateful<Div> {
    tip(
        div()
            .id(id.into())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(30.))
            .border_1()
            .border_color(border())
            .cursor_pointer()
            .hover(|s| s.bg(hover()))
            .child(icon(icon_name, color).size(px(16.))),
        text,
    )
}

pub fn disabled(el: Stateful<Div>, off: bool) -> Stateful<Div> {
    if off { el.opacity(0.45).cursor_default() } else { el }
}

/// Thin progress bar.
pub fn progress(pct: f64, color: Hsla) -> Div {
    div()
        .h(px(6.))
        .w_full()
        .bg(secondary())
        .child(div().h_full().w(relative((pct.clamp(0., 100.) / 100.) as f32)).bg(color))
}

pub fn bar_color(pct: f64) -> Hsla {
    if pct >= 90. {
        red_strong()
    } else if pct >= 70. {
        amber()
    } else {
        emerald()
    }
}

/// "2h 13m", "3d 4h", "5m" until the given epoch ms.
pub fn time_until(ts: i64) -> String {
    let m = ((ts - now_ms()) as f64 / 60000.).round().max(0.) as i64;
    if m < 60 {
        return format!("{m}m");
    }
    let h = m / 60;
    if h < 24 {
        return format!("{h}h {}m", m % 60);
    }
    format!("{}d {}h", h / 24, h % 24)
}

pub fn time_ago(ts: Option<i64>) -> String {
    let Some(ts) = ts.filter(|t| *t > 0) else { return "never".into() };
    let s = ((now_ms() - ts) as f64 / 1000.).round() as i64;
    if s < 60 {
        return "just now".into();
    }
    let m = (s as f64 / 60.).round() as i64;
    if m < 60 {
        return format!("{m} min ago");
    }
    let h = (m as f64 / 60.).round() as i64;
    if h < 24 {
        return format!("{h} h ago");
    }
    format!("{} d ago", (h as f64 / 24.).round() as i64)
}

/// "Resets at 11:00 PM · in 2h 7m", like the Claude app.
pub fn reset_text(w: &UsageWindow) -> String {
    use chrono::{Local, TimeZone};
    if w.stale {
        return "Window restarted. New usage shows after the next refresh.".into();
    }
    if let Some(at) = w.resets_at {
        let d = Local.timestamp_millis_opt(at).single();
        let today = Local::now().date_naive();
        let days = (at - now_ms()) as f64 / 86_400_000.;
        let when = match d {
            Some(d) if d.date_naive() == today => format!("at {}", d.format("%H:%M")),
            Some(d) if days < 6.5 => d.format("%a %H:%M").to_string(),
            Some(d) => d.format("%b %-d, %H:%M").to_string(),
            None => String::new(),
        };
        return format!("Resets {when} · in {}", time_until(at));
    }
    if let Some(l) = &w.resets_label {
        return format!("Resets {l}");
    }
    if w.used_percent == 0. { "Not started. The window begins with your next message.".into() } else { String::new() }
}

/// One usage window: percent, bar and reset time.
pub fn usage_meter(w: Option<&UsageWindow>) -> Div {
    let Some(w) = w else { return div().text_xs().text_color(muted_fg()).child("—") };
    let pct = if w.stale { 0. } else { w.used_percent.round() };
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .min_w(px(150.))
        .child(div().flex().justify_end().text_xs().font_weight(FontWeight::MEDIUM).child(format!("{pct}%")))
        .child(progress(pct, bar_color(pct)))
        .child(div().text_size(px(10.)).text_color(muted_fg()).child(reset_text(w)))
}

pub fn usage_hot_pct(u: Option<&ProfileUsage>) -> f64 {
    let Some(u) = u else { return 0. };
    let p = |w: &Option<UsageWindow>| w.as_ref().filter(|w| !w.stale).map(|w| w.used_percent).unwrap_or(0.);
    p(&u.five_hour).max(p(&u.week))
}

/// Compact "5h 23% · wk 41%" for buttons.
pub fn usage_inline(u: Option<&ProfileUsage>) -> Option<Div> {
    let u = u?;
    let f = |w: &Option<UsageWindow>| match w {
        Some(w) if w.stale => "0%".to_string(),
        Some(w) => format!("{}%", w.used_percent.round()),
        None => "–".into(),
    };
    let hot = usage_hot_pct(Some(u));
    let color = if hot >= 90. { red() } else if hot >= 70. { amber() } else { muted_fg() };
    Some(div().text_size(px(10.)).text_color(color).child(format!("5h {} · wk {}", f(&u.five_hour), f(&u.week))))
}

/// What a tab/project should signal (see signal_of).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Signal {
    Working,
    Done,
    Error,
    NeedsYou,
}

impl Signal {
    fn rank(self) -> u8 {
        match self {
            Signal::NeedsYou => 4,
            Signal::Error => 3,
            Signal::Done => 2,
            Signal::Working => 1,
        }
    }
}

pub fn signal_of(activity: Option<Activity>, unseen: bool) -> Option<Signal> {
    match activity? {
        Activity::NeedsYou => Some(Signal::NeedsYou),
        Activity::Error if unseen => Some(Signal::Error),
        Activity::Done if unseen => Some(Signal::Done),
        Activity::Working => Some(Signal::Working),
        _ => None,
    }
}

pub fn strongest(a: Option<Signal>, b: Option<Signal>) -> Option<Signal> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if y.rank() > x.rank() { y } else { x }),
        (x, y) => x.or(y),
    }
}

pub fn signal_dot(s: Option<Signal>) -> Option<AnyElement> {
    let s = s?;
    let color = match s {
        Signal::Working => return Some(spinner(sky()).into_any_element()),
        Signal::NeedsYou => amber(),
        Signal::Error => red_strong(),
        Signal::Done => emerald(),
    };
    Some(icon("circle-fill", color).size(px(9.)).into_any_element())
}

pub fn activity_badge(a: Activity) -> Div {
    let (label, color) = match a {
        Activity::Starting => ("starting", muted_fg()),
        Activity::Idle => ("ready", muted_fg()),
        Activity::Working => ("working", sky()),
        Activity::NeedsYou => ("needs you", amber()),
        Activity::Done => ("finished", emerald()),
        Activity::Error => ("error", red()),
        Activity::Ended => ("ended", muted_fg()),
    };
    badge(label, color)
}

pub fn section_label(text: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).child(text.into())
}

pub fn muted(text: impl Into<SharedString>) -> Div {
    div().text_size(px(11.)).text_color(muted_fg()).child(text.into())
}
