//! Per-profile taskbar buttons: render each profile's icon to an .ico and tag its Claude
//! windows with their own AppUserModelID, icon and title.

use std::collections::HashMap;
use std::path::PathBuf;

use cm_core::paths::Paths;
use cm_core::{Profile, ProfileId};
use cm_win::window::WindowHandle;
use gpui_kit::AssetSource;

use crate::assets::{AppAssets, profile_icon};

// The taskbar never asks for more than 64px; skipping 256 keeps the files small.
const ICO_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 96];
/// Bump when the icon artwork changes so cached files are regenerated.
const ICON_VERSION: u32 = 2;

/// Includes the look, so a recolored profile gets a fresh taskbar entry (Windows caches the
/// button icon per ID).
pub fn app_id(p: &Profile) -> String {
    format!(
        "ClaudeManager.Profile.{}.{}{}",
        p.id,
        p.color.label(),
        p.icon.label()
    )
}

/// What a tagged window should currently look like; re-tag when this changes.
pub fn tag_key(p: &Profile) -> String {
    format!("{}|{}|{}", p.color.label(), p.icon.label(), p.name)
}

/// `%APPDATA%\ClaudeManager\icons\<color>-<icon>.ico`, rendered on first use.
pub fn ensure_icon(paths: &Paths, p: &Profile) -> Option<PathBuf> {
    let dir = paths.icons_dir();
    let file = dir.join(format!(
        "{}-{}-v{ICON_VERSION}.ico",
        p.color.label().to_lowercase(),
        p.icon.label().to_lowercase()
    ));
    if file.exists() {
        return Some(file);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let svg = AppAssets.load(&profile_icon(p.icon).path()).ok()??;
    let inner = svg_inner(std::str::from_utf8(&svg).ok()?)?;
    let images: Vec<(u32, Vec<u8>)> = ICO_SIZES
        .iter()
        .filter_map(|&s| Some((s, render(inner, p.color.rgb(), s)?)))
        .collect();
    if images.len() != ICO_SIZES.len() {
        return None;
    }
    let tmp = file.with_extension("ico.tmp");
    std::fs::write(&tmp, crate::icons::ico_bytes(&images)).ok()?;
    std::fs::rename(&tmp, &file).ok()?;
    Some(file)
}

/// The child elements of a Lucide `<svg>` document.
fn svg_inner(svg: &str) -> Option<&str> {
    let open = svg.find("<svg")?;
    let start = open + svg[open..].find('>')? + 1;
    let end = svg.rfind("</svg>")?;
    (start <= end).then(|| &svg[start..end])
}

/// A solid rounded tile in the profile color with the glyph in white, as straight-alpha RGBA.
fn render(inner: &str, rgb: u32, size: u32) -> Option<Vec<u8>> {
    // Small sizes get a thicker stroke so the glyph stays legible at 16px.
    let stroke = if size <= 24 { 2.6 } else { 2.2 };
    let doc = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
<rect x="0.5" y="0.5" width="23" height="23" rx="5.5" fill="#{rgb:06X}"/>
<g transform="translate(4.8 4.8) scale(0.6)" fill="none" stroke="#FFFFFF" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round">{inner}</g>
</svg>"##
    );
    let tree = resvg::usvg::Tree::from_str(&doc, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / 24.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia is premultiplied; .ico wants straight alpha.
    let mut px = pixmap.take();
    for p in px.chunks_mut(4) {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(px)
}

/// Windows we've tagged, and with what.
#[derive(Default)]
pub struct Tagged(pub HashMap<WindowHandle, String>);

pub struct TagRequest {
    pub hwnd: WindowHandle,
    pub app_id: String,
    pub icon: PathBuf,
    pub key: String,
}

/// Work out which windows need (re)tagging and which should be cleared.
pub fn plan(
    paths: &Paths,
    profiles: &[Profile],
    windows: &[(ProfileId, Vec<WindowHandle>)],
    tagged: &Tagged,
    enabled: bool,
) -> Plan {
    if !enabled {
        let clear: Vec<_> = tagged.0.keys().copied().collect();
        // Hand the plain title back too.
        let titles = clear.iter().map(|h| (*h, "Claude".to_string())).collect();
        return Plan {
            tag: Vec::new(),
            clear,
            titles,
        };
    }
    let mut tag = Vec::new();
    let mut titles = Vec::new();
    for (id, hwnds) in windows {
        let Some(p) = profiles.iter().find(|p| p.id == *id) else {
            continue;
        };
        let key = tag_key(p);
        let wanted = window_title(p);
        for &hwnd in hwnds {
            // Claude resets its title when it navigates, so check every scan.
            if cm_win::window::title(hwnd) != wanted {
                titles.push((hwnd, wanted.clone()));
            }
            if tagged.0.get(&hwnd) == Some(&key) {
                continue;
            }
            let Some(icon) = ensure_icon(paths, p) else {
                continue;
            };
            tag.push(TagRequest {
                hwnd,
                app_id: app_id(p),
                icon,
                key: key.clone(),
            });
        }
    }
    Plan {
        tag,
        clear: Vec::new(),
        titles,
    }
}

pub struct Plan {
    pub tag: Vec<TagRequest>,
    pub clear: Vec<WindowHandle>,
    pub titles: Vec<(WindowHandle, String)>,
}

/// "Work · Claude": the profile name first, so Alt-Tab and taskbar previews tell windows apart.
pub fn window_title(p: &Profile) -> String {
    format!("{} · Claude", p.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_svg_children() {
        let svg = r#"<svg xmlns="x" width="24"><path d="M1 1"/><circle r="2"/></svg>"#;
        assert_eq!(svg_inner(svg), Some(r#"<path d="M1 1"/><circle r="2"/>"#));
    }

    #[test]
    fn renders_tile_with_glyph() {
        let px = render(r#"<path d="M4 12h16"/>"#, 0xE07A5F, 32).unwrap();
        assert_eq!(px.len(), 32 * 32 * 4);
        // Corner is transparent (rounded), center line is white, tile body is the profile color.
        assert_eq!(px[3], 0);
        let at = |x: usize, y: usize| &px[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        // Anti-aliased stroke: mostly white, with a little coral bleeding in.
        assert!(
            at(16, 16)[1] > 200 && at(16, 16)[2] > 200,
            "glyph pixel {:?}",
            at(16, 16)
        );
        assert_eq!(at(16, 6)[..3], [0xE0, 0x7A, 0x5F]);
    }
}
