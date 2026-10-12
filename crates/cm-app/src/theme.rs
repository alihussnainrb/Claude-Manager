//! Meadow: cream grounds with coral as the only brand color.
//!
//! Our own views read [`Palette`]; gpui-component widgets (inputs, editor, scrollbars) read the
//! matching theme registered from `assets/themes/meadow.json`.

use std::borrow::Cow;

use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;

pub const FONT_UI: &str = "Figtree";
pub const FONT_MONO: &str = "Cascadia Mono";

/// Colors as 0xRRGGBB, converted on use.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub side: u32,
    pub side_fg: u32,
    pub side_muted: u32,
    pub side_sel: u32,
    pub side_line: u32,
    pub line: u32,
    pub fg: u32,
    pub muted: u32,
    pub primary: u32,
    pub primary_fg: u32,
    pub accent: u32,
    pub accent_soft: u32,
    pub accent_ink: u32,
    pub surface: u32,
    pub code: u32,
    pub kbd: u32,
    pub ok: u32,
    pub warn: u32,
    pub bad: u32,
    pub dark: bool,
}

pub const LIGHT: Palette = Palette {
    bg: 0xFBF8F2,
    side: 0xF2EBDD,
    side_fg: 0x2B2622,
    side_muted: 0x85796D,
    side_sel: 0xFFFDF8,
    side_line: 0xE5DACA,
    line: 0xE9E0D1,
    fg: 0x2A2420,
    muted: 0x7A7065,
    primary: 0xC95A40,
    primary_fg: 0xFFF8F1,
    accent: 0xE2725B,
    accent_soft: 0xF9E1D7,
    accent_ink: 0xA9442D,
    surface: 0xFFFDF8,
    code: 0xF7F1E7,
    kbd: 0xF4EDE1,
    ok: 0x4C8C5E,
    warn: 0xB9771A,
    bad: 0xB83A3A,
    dark: false,
};

pub const DARK: Palette = Palette {
    bg: 0x1D1A17,
    side: 0x161311,
    side_fg: 0xEFE7DA,
    side_muted: 0xA0958A,
    side_sel: 0x2A2521,
    side_line: 0x29231F,
    line: 0x322B25,
    fg: 0xF1E9DD,
    muted: 0xA3988C,
    primary: 0xEE8B6F,
    primary_fg: 0x1F1410,
    accent: 0xF2957A,
    accent_soft: 0x43271F,
    accent_ink: 0xF5A98F,
    surface: 0x231F1B,
    code: 0x1A1714,
    kbd: 0x2A2420,
    ok: 0x6CC08A,
    warn: 0xE5AA4F,
    bad: 0xF07D6C,
    dark: true,
};

pub fn palette(cx: &App) -> &'static Palette {
    if Theme::global(cx).is_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

/// `0xRRGGBB` → color.
pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// `0xRRGGBB` with an alpha in 0..=1.
pub fn ca(hex: u32, alpha: f32) -> Hsla {
    let mut h: Hsla = rgb(hex).into();
    h.a = alpha;
    h
}

/// Mix `hex` over `base` (both 0xRRGGBB) — the tinted-tile wash.
pub fn mix(hex: u32, base: u32, amount: f32) -> Hsla {
    let ch = |v: u32, s: u32| ((v >> s) & 0xFF) as f32;
    let blend = |s| (ch(hex, s) * amount + ch(base, s) * (1.0 - amount)).round() as u32;
    c((blend(16) << 16) | (blend(8) << 8) | blend(0))
}

const FONTS: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/Figtree-Regular.ttf"),
    include_bytes!("../../../assets/fonts/Figtree-Medium.ttf"),
    include_bytes!("../../../assets/fonts/Figtree-SemiBold.ttf"),
    include_bytes!("../../../assets/fonts/Figtree-Bold.ttf"),
];

const THEMES: &str = include_str!("../../../assets/themes/meadow.json");

pub fn init(cx: &mut App) {
    if let Err(e) = cx
        .text_system()
        .add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect())
    {
        tracing::warn!("loading bundled fonts failed: {e}");
    }
    if let Err(e) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEMES) {
        tracing::error!("meadow theme failed to parse: {e}");
    }
    let registry = ThemeRegistry::global(cx);
    let light = registry.themes().get("Meadow Light").cloned();
    let dark = registry.themes().get("Meadow Dark").cloned();
    {
        let theme = Theme::global_mut(cx);
        if let Some(l) = light {
            theme.light_theme = l;
        }
        if let Some(d) = dark {
            theme.dark_theme = d;
        }
    }
    let mode = forced_mode().unwrap_or(match cx.window_appearance() {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
        _ => ThemeMode::Light,
    });
    Theme::change(mode, None, cx);
}

/// Follow the Windows light/dark setting.
pub fn sync(window: &mut Window, cx: &mut App) {
    match forced_mode() {
        Some(mode) => Theme::change(mode, Some(window), cx),
        None => Theme::sync_system_appearance(Some(window), cx),
    }
}

/// Debug builds: `CM_THEME=light|dark` overrides the system setting for visual checks.
fn forced_mode() -> Option<ThemeMode> {
    if !cfg!(debug_assertions) {
        return None;
    }
    match std::env::var("CM_THEME").ok()?.as_str() {
        "light" => Some(ThemeMode::Light),
        "dark" => Some(ThemeMode::Dark),
        _ => None,
    }
}
