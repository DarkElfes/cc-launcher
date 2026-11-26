// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod ui;

use gpui_kit::*;
use gpui_kit::prelude::*;
use ui::app::LauncherApp;

fn main() {
    // Don't leak settings from whatever launched this app (for example a Claude Code session) into
    // the claude processes it starts. Done first, before any thread exists.
    for k in ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_SSE_PORT", "CLAUDE_CONFIG_DIR"] {
        // SAFETY: single-threaded at this point.
        unsafe { std::env::remove_var(k) };
    }

    let (tx, rx) = async_channel::unbounded();
    if !backend::platform::single_instance(tx.clone()) {
        return; // the running copy shows its window
    }

    gpui_kit::application().with_assets(ui::icons::AppAssets).run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
        ui::theme::load_fonts(cx);
        ui::theme::apply_component_theme(cx);

        let window_bounds = saved_window_bounds(cx)
            .unwrap_or_else(|| WindowBounds::Windowed(Bounds::centered(None, size(px(1320.), px(840.)), cx)));
        let options = WindowOptions {
            window_bounds: Some(window_bounds),
            window_min_size: Some(size(px(900.), px(560.))),
            titlebar: Some(TitlebarOptions { title: Some("cc-launcher".into()), appears_transparent: true, traffic_light_position: None }),
            app_id: Some("dev.cclauncher.app".into()),
            ..Default::default()
        };
        let tx2 = tx.clone();
        let rx2 = rx.clone();
        let (_, view) = gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| LauncherApp::new(tx2, rx2, window, cx)))
            .expect("failed to open window");

        // Closing the window ends every Claude process and quits.
        let v = view.clone();
        cx.on_window_closed(move |cx, _| {
            v.update(cx, |app, cx| app.shutdown(cx));
            cx.quit();
        })
        .detach();
        cx.activate(true);
    });
}

/// The window placement saved last time, if it is big enough and still on a connected display.
fn saved_window_bounds(cx: &App) -> Option<WindowBounds> {
    let w = backend::store::Store::load().settings.window?;
    if w.width < 900. || w.height < 560. {
        return None;
    }
    let b = Bounds::new(point(px(w.x), px(w.y)), size(px(w.width), px(w.height)));
    // At least the top-left corner area (where the title bar is) must be visible on some display.
    let grab = Bounds::new(b.origin, size(px(120.), px(40.)));
    if !cx.displays().iter().any(|d| d.bounds().intersects(&grab)) {
        return None;
    }
    Some(if w.maximized { WindowBounds::Maximized(b) } else { WindowBounds::Windowed(b) })
}
