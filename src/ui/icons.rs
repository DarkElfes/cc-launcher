//! Phosphor icons (the same set the Electron build used), embedded in the exe as SVGs and drawn in
//! the current text color. Files: assets/phosphor/<name>.svg (regular) and <name>-fill.svg.

use gpui_kit::*;
use std::borrow::Cow;

#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "phosphor/*.svg"]
struct Embedded;

/// Our icons first, then gpui-kit's own assets (its components use them).
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some(f) = Embedded::get(path) {
            return Ok(Some(f.data));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> = Embedded::iter().filter(|p| p.starts_with(path)).map(|p| SharedString::from(p.to_string())).collect();
        out.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(out)
    }
}

/// A 14px icon. GPUI draws an SVG only in the color given here (it doesn't inherit).
pub fn icon(name: &str, color: Hsla) -> Svg {
    svg().path(SharedString::from(format!("phosphor/{name}.svg"))).size(px(14.)).flex_none().text_color(color)
}
