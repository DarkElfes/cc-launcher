//! Draws one Claude Code terminal (alacritty_terminal grid) and turns keys into terminal input.

use super::theme::{self, MONO};
use crate::backend::terminal::Terminal;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as TPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color as AColor, CursorShape, NamedColor, Rgb};
use gpui_kit::*;
use gpui_kit::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

const FONT_SIZE: f32 = 13.;

pub struct TerminalView {
    pub tab: String,
    term: Terminal,
    focus: FocusHandle,
    pub exit_code: Option<i32>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cell: Option<(Pixels, Pixels)>,
    selecting: bool,
}

impl TerminalView {
    pub fn new(tab: &str, term: Terminal, cx: &mut Context<Self>) -> Self {
        Self {
            tab: tab.into(),
            term,
            focus: cx.focus_handle(),
            exit_code: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            cell: None,
            selecting: false,
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    pub fn write(&self, data: impl Into<Vec<u8>>) {
        if self.exit_code.is_none() {
            self.term.write(data);
        }
    }

    /// Pastes text the way a terminal does (bracketed paste when the app asked for it).
    pub fn paste_text(&self, text: &str) {
        let bracketed = self.term.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\n").replace('\n', "\r");
        if bracketed {
            self.write(format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes());
        } else {
            self.write(text.into_bytes());
        }
    }

    pub fn kill(&self) {
        self.term.kill();
    }

    fn cell_size(&mut self, window: &mut Window) -> (Pixels, Pixels) {
        if let Some(c) = self.cell {
            return c;
        }
        let font = font(MONO);
        let id = window.text_system().resolve_font(&font);
        let w = window
            .text_system()
            .advance(id, px(FONT_SIZE), 'm')
            .map(|s| s.width)
            .unwrap_or(px(FONT_SIZE * 0.6));
        let h = px((FONT_SIZE * 1.3).round());
        self.cell = Some((w, h));
        (w, h)
    }

    fn point_at(&self, pos: gpui_kit::Point<Pixels>) -> (TPoint, Side) {
        let b = self.bounds.get();
        let (cw, ch) = self.cell.unwrap_or((px(8.), px(17.)));
        let x = ((pos.x - b.origin.x) / cw).max(0.);
        let y = ((pos.y - b.origin.y) / ch).max(0.);
        let term = self.term.term.lock();
        let col = (x as usize).min(term.columns().saturating_sub(1));
        let row = (y as usize).min(term.screen_lines().saturating_sub(1));
        let offset = term.grid().display_offset() as i32;
        let side = if x.fract() < 0.5 { Side::Left } else { Side::Right };
        (TPoint::new(Line(row as i32 - offset), Column(col)), side)
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = &k.modifiers;
        // Copy / paste.
        if m.control && m.shift && k.key == "c" || m.control && k.key == "c" && self.has_selection() {
            if let Some(text) = self.term.term.lock().selection_to_string() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            self.term.term.lock().selection = None;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if m.control && (k.key == "v") {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                self.paste_text(&text);
            }
            cx.stop_propagation();
            return;
        }
        let app_cursor = self.term.term.lock().mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = key_bytes(k, app_cursor) {
            // Typing scrolls back to the bottom and clears a selection.
            {
                let mut t = self.term.term.lock();
                t.scroll_display(Scroll::Bottom);
                t.selection = None;
            }
            self.write(bytes);
            cx.stop_propagation();
        }
    }

    fn has_selection(&self) -> bool {
        self.term.term.lock().selection.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
    }
}

/// Bytes a terminal sends for a key press, or None to let it bubble.
fn key_bytes(k: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let m = &k.modifiers;
    let arrow = |c: char| {
        if m.control || m.alt || m.shift {
            let n = 1 + (m.shift as u8) + 2 * (m.alt as u8) + 4 * (m.control as u8);
            format!("\x1b[1;{n}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let s: Option<Vec<u8>> = match k.key.as_str() {
        "enter" if m.shift || m.alt => Some(b"\x1b\r".to_vec()),
        "enter" => Some(b"\r".to_vec()),
        "backspace" if m.control || m.alt => Some(b"\x1b\x7f".to_vec()),
        "backspace" => Some(b"\x7f".to_vec()),
        "tab" if m.shift => Some(b"\x1b[Z".to_vec()),
        "tab" => Some(b"\t".to_vec()),
        "escape" => Some(b"\x1b".to_vec()),
        "up" => Some(arrow('A')),
        "down" => Some(arrow('B')),
        "right" => Some(arrow('C')),
        "left" => Some(arrow('D')),
        "home" => Some(if app_cursor { b"\x1bOH".to_vec() } else { b"\x1b[H".to_vec() }),
        "end" => Some(if app_cursor { b"\x1bOF".to_vec() } else { b"\x1b[F".to_vec() }),
        "pageup" => Some(b"\x1b[5~".to_vec()),
        "pagedown" => Some(b"\x1b[6~".to_vec()),
        "delete" => Some(b"\x1b[3~".to_vec()),
        "insert" => Some(b"\x1b[2~".to_vec()),
        "space" if m.control => Some(vec![0]),
        _ => None,
    };
    if s.is_some() {
        return s;
    }
    if m.platform {
        return None;
    }
    // Ctrl+letter → control code.
    if m.control && !m.alt && k.key.len() == 1 {
        let ch = k.key.as_bytes()[0].to_ascii_lowercase();
        return match ch {
            b'a'..=b'z' => Some(vec![ch - b'a' + 1]),
            b'[' => Some(vec![0x1b]),
            b'\\' => Some(vec![0x1c]),
            b']' => Some(vec![0x1d]),
            _ => None,
        };
    }
    // Plain typing (any layout, IME, AltGr) arrives through the text input handler instead.
    if !(m.alt && !m.control) {
        return None;
    }
    // Alt+key → ESC prefix (meta).
    let text = k.key_char.clone().or_else(|| (k.key.chars().count() == 1).then(|| k.key.clone()))?;
    let mut out = vec![0x1b];
    out.extend_from_slice(text.as_bytes());
    Some(out)
}

impl EntityInputHandler for TerminalView {
    fn text_for_range(&mut self, _: std::ops::Range<usize>, _: &mut Option<std::ops::Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }
    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        None
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<std::ops::Range<usize>> {
        None
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn replace_text_in_range(&mut self, _: Option<std::ops::Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        {
            let mut t = self.term.term.lock();
            t.scroll_display(Scroll::Bottom);
            t.selection = None;
        }
        self.write(text.as_bytes().to_vec());
        cx.notify();
    }
    fn replace_and_mark_text_in_range(&mut self, _: Option<std::ops::Range<usize>>, _: &str, _: Option<std::ops::Range<usize>>, _: &mut Window, _: &mut Context<Self>) {}
    fn bounds_for_range(&mut self, _: std::ops::Range<usize>, element_bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }
    fn character_index_for_point(&mut self, _: gpui_kit::Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

// ---------- colors ----------

fn rgb_to_hsla(c: Rgb) -> Hsla {
    Rgba { r: c.r as f32 / 255., g: c.g as f32 / 255., b: c.b as f32 / 255., a: 1. }.into()
}

const ANSI: [u32; 16] = [
    0x1d1d1d, 0xf87171, 0x4ade80, 0xfacc15, 0x60a5fa, 0xc084fc, 0x22d3ee, 0xd4d4d4, 0x737373, 0xfca5a5, 0x86efac, 0xfde047,
    0x93c5fd, 0xd8b4fe, 0x67e8f9, 0xfafafa,
];

fn indexed(i: u8) -> Hsla {
    let i = i as u32;
    let hex = if i < 16 {
        ANSI[i as usize]
    } else if i < 232 {
        let i = i - 16;
        let v = |x: u32| if x == 0 { 0 } else { 55 + x * 40 };
        (v(i / 36) << 16) | (v((i / 6) % 6) << 8) | v(i % 6)
    } else {
        let g = 8 + (i - 232) * 10;
        (g << 16) | (g << 8) | g
    };
    theme::c(hex)
}

fn resolve(color: AColor, colors: &alacritty_terminal::term::color::Colors, fg: bool) -> Hsla {
    match color {
        AColor::Spec(rgb) => rgb_to_hsla(rgb),
        AColor::Indexed(i) => colors[i as usize].map(rgb_to_hsla).unwrap_or_else(|| indexed(i)),
        AColor::Named(n) => {
            if let Some(c) = colors[n] {
                return rgb_to_hsla(c);
            }
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground => theme::c(0xe5e5e5),
                NamedColor::DimForeground => theme::c(0xa3a3a3),
                NamedColor::Background => theme::background(),
                NamedColor::Cursor => theme::c(0xe5e5e5),
                n if (n as usize) < 16 => indexed(n as u8),
                n if (n as usize) >= NamedColor::DimBlack as usize && (n as usize) <= NamedColor::DimWhite as usize => {
                    indexed((n as usize - NamedColor::DimBlack as usize) as u8).opacity(0.7)
                }
                _ => {
                    if fg {
                        theme::c(0xe5e5e5)
                    } else {
                        theme::background()
                    }
                }
            }
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (cw, ch) = self.cell_size(window);
        let focused = self.focus.is_focused(window);
        let base_font = font(MONO);
        let mut lines: Vec<AnyElement> = vec![];
        {
            let term = self.term.term.lock();
            let content = term.renderable_content();
            let rows = term.screen_lines();
            let offset = content.display_offset as i32;
            let colors = content.colors;
            let sel = content.selection;
            let cursor = content.cursor;
            let mut text: Vec<String> = vec![String::new(); rows];
            let mut runs: Vec<Vec<TextRun>> = vec![vec![]; rows];
            for cell in content.display_iter {
                let row = (cell.point.line.0 + offset) as usize;
                if row >= rows {
                    continue;
                }
                let flags = cell.flags;
                if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                let mut fg = resolve(cell.fg, colors, true);
                let mut bg = resolve(cell.bg, colors, false);
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if flags.contains(Flags::DIM) {
                    fg = fg.opacity(0.65);
                }
                let selected = sel.map(|s| s.contains(cell.point)).unwrap_or(false);
                let is_cursor = cell.point == cursor.point && cursor.shape != CursorShape::Hidden && content.display_offset == 0;
                let mut bg_opt = if bg == theme::background() { None } else { Some(bg) };
                if selected {
                    bg_opt = Some(theme::sky().opacity(0.35));
                }
                if is_cursor {
                    if focused && cursor.shape == CursorShape::Block {
                        bg_opt = Some(theme::c(0xe5e5e5));
                        fg = theme::background();
                    } else {
                        bg_opt = Some(theme::c(0xe5e5e5).opacity(0.35));
                    }
                }
                let mut c = cell.c;
                if flags.contains(Flags::HIDDEN) || c == '\0' {
                    c = ' ';
                }
                let mut s = String::new();
                s.push(c);
                if let Some(zw) = cell.zerowidth() {
                    for z in zw {
                        s.push(*z);
                    }
                }
                let f = Font {
                    weight: if flags.contains(Flags::BOLD) { FontWeight::BOLD } else { FontWeight::NORMAL },
                    style: if flags.contains(Flags::ITALIC) { FontStyle::Italic } else { FontStyle::Normal },
                    ..base_font.clone()
                };
                let underline = flags.intersects(Flags::ALL_UNDERLINES).then(|| UnderlineStyle {
                    thickness: px(1.),
                    color: Some(fg),
                    wavy: flags.contains(Flags::UNDERCURL),
                });
                let strike = flags.contains(Flags::STRIKEOUT).then(|| StrikethroughStyle { thickness: px(1.), color: Some(fg) });
                let line_runs = &mut runs[row];
                // Merge with the previous run when the style is the same.
                if let Some(last) = line_runs.last_mut() {
                    if last.color == fg && last.background_color == bg_opt && last.font == f && last.underline == underline && last.strikethrough == strike {
                        last.len += s.len();
                        text[row].push_str(&s);
                        continue;
                    }
                }
                line_runs.push(TextRun { len: s.len(), font: f, color: fg, background_color: bg_opt, underline, strikethrough: strike });
                text[row].push_str(&s);
            }
            for (i, (t, r)) in text.into_iter().zip(runs).enumerate() {
                let el = if t.is_empty() {
                    div().h(ch).into_any_element()
                } else {
                    div()
                        .h(ch)
                        .whitespace_nowrap()
                        .child(StyledText::new(t).with_runs(r))
                        .into_any_element()
                };
                lines.push(div().id(("l", i)).h(ch).child(el).into_any_element());
            }
        }

        let bounds = self.bounds.clone();
        let entity = cx.entity().downgrade();
        let strong = cx.entity();
        let focus_for_input = self.focus.clone();
        let exited = self.exit_code;
        div()
            .id(SharedString::from(format!("term-{}", self.tab)))
            .track_focus(&self.focus)
            .key_context("Terminal")
            .size_full()
            .relative()
            .bg(theme::background())
            .px(px(8.))
            .py(px(4.))
            .overflow_hidden()
            .font_family(MONO)
            .text_size(px(FONT_SIZE))
            .line_height(ch)
            .text_color(theme::c(0xe5e5e5))
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    this.focus(window, cx);
                    let (p, side) = this.point_at(ev.position);
                    let kind = match ev.click_count {
                        2 => SelectionType::Semantic,
                        3 => SelectionType::Lines,
                        _ => SelectionType::Simple,
                    };
                    this.term.term.lock().selection = Some(Selection::new(kind, p, side));
                    this.selecting = true;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if this.selecting && ev.pressed_button == Some(MouseButton::Left) {
                    let (p, side) = this.point_at(ev.position);
                    if let Some(sel) = this.term.term.lock().selection.as_mut() {
                        sel.update(p, side);
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.selecting = false;
                    // A plain click (no drag) leaves no selection.
                    let empty = this.term.term.lock().selection.as_ref().map(|s| s.is_empty()).unwrap_or(true);
                    if empty {
                        this.term.term.lock().selection = None;
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    // Right click: copy the selection, or paste when there is none (like Windows Terminal).
                    let sel = this.term.term.lock().selection_to_string().filter(|s| !s.is_empty());
                    match sel {
                        Some(text) => {
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                            this.term.term.lock().selection = None;
                        }
                        None => {
                            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                                this.paste_text(&text);
                            }
                        }
                    }
                    cx.notify();
                }),
            )
            .on_scroll(cx.listener(move |this, ev: &ScrollEvent, _, cx| {
                let lines = match ev.delta {
                    ScrollDelta::Lines(p) => (p.y * 3.) as i32,
                    ScrollDelta::Pixels(p) => (p.y / ch) as i32,
                };
                if lines != 0 {
                    this.term.term.lock().scroll_display(Scroll::Delta(lines));
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |b, _window, cx| {
                        bounds.set(b);
                        // Fit the terminal to its area.
                        let cols = ((b.size.width / cw).floor() as usize).max(20);
                        let rows = ((b.size.height / ch).floor() as usize).max(5);
                        if let Some(e) = entity.upgrade() {
                            e.update(cx, |this, _| this.term.resize(cols, rows, f32::from(cw), f32::from(ch)));
                        }
                    },
                    move |b, _, window, cx| {
                        // Typed text (any keyboard layout, IME) comes in through this handler.
                        window.handle_input(&focus_for_input, ElementInputHandler::new(b, strong.clone()), cx);
                    },
                )
                .absolute()
                .top(px(4.))
                .left(px(8.))
                .right(px(8.))
                .bottom(px(4.)),
            )
            .child(div().flex().flex_col().children(lines))
            .when_some(exited, |d, code| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(8.))
                        .right(px(12.))
                        .px(px(8.))
                        .py(px(2.))
                        .bg(theme::secondary())
                        .text_xs()
                        .font_family(theme::UI_FONT)
                        .text_color(theme::muted_fg())
                        .child(format!("Claude Code exited (code {code}). Close this tab or open a new one.")),
                )
            })
    }
}
