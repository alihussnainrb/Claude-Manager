//! Small building blocks shared by every window, styled from the Meadow palette.

use cm_core::Profile;
use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::assets::{LOGO, profile_icon};
use crate::model::Status;
use crate::theme::{FONT_MONO, Palette, c, ca, mix};

pub fn icon(name: IconName, size: f32, color: Hsla) -> Svg {
    svg()
        .path(name.path())
        .size(px(size))
        .flex_none()
        .text_color(color)
}

pub fn logo(size: f32, color: Hsla) -> Svg {
    svg()
        .path(LOGO)
        .size(px(size))
        .flex_none()
        .text_color(color)
}

/// Page or dialog title in the interface font.
pub fn heading(text: impl Into<SharedString>, size: f32, p: &Palette) -> Div {
    div()
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(size))
        .line_height(px(size * 1.25))
        .text_color(c(p.fg))
        .child(text.into())
}

/// Card title in the interface font (Figtree), matching the profile header.
pub fn card_title(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .text_size(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .line_height(px(18.))
        .text_color(c(p.fg))
        .child(text.into())
}

pub fn mono(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(11.))
        .text_color(c(p.muted))
        .child(text.into())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    leading: Option<IconName>,
    kind: Btn,
    small: bool,
    p: &Palette,
) -> Stateful<Div> {
    let (bg, fg, border) = match kind {
        Btn::Primary => (ca(p.primary, 1.0), c(p.primary_fg), ca(p.primary, 0.0)),
        Btn::Secondary => (c(p.surface), c(p.fg), c(p.line)),
        Btn::Ghost => (ca(p.surface, 0.0), c(p.muted), ca(p.line, 0.0)),
        Btn::Danger => (c(p.surface), c(p.bad), c(p.line)),
    };
    let hover_bg = match kind {
        Btn::Primary => mix(p.primary, if p.dark { 0xFFFFFF } else { 0x000000 }, 0.9),
        _ => mix(p.fg, p.surface, 0.06),
    };
    h_flex()
        .id(id)
        .flex_none()
        .items_center()
        .gap(px(7.))
        .h(px(if small { 28. } else { 34. }))
        .px(px(if small { 10. } else { 14. }))
        .rounded(px(9.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_color(fg)
        .text_size(px(if small { 12. } else { 12.5 }))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .when_some(leading, |this, i| this.child(icon(i, 14., fg)))
        .child(label.into())
}

pub fn icon_button(id: impl Into<ElementId>, name: IconName, p: &Palette) -> Stateful<Div> {
    let hover = mix(p.fg, p.surface, 0.06);
    div()
        .id(id)
        .flex_none()
        .size(px(34.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .border_1()
        .border_color(c(p.line))
        .bg(c(p.surface))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(name, 15., c(p.fg)))
}

pub fn toggle(id: impl Into<ElementId>, on: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .w(px(36.))
        .h(px(21.))
        .rounded_full()
        .bg(if on {
            c(p.primary)
        } else {
            mix(p.fg, p.surface, if p.dark { 0.22 } else { 0.16 })
        })
        .cursor_pointer()
        .relative()
        .child(
            div()
                .absolute()
                .top(px(3.))
                .left(px(if on { 18. } else { 3. }))
                .size(px(15.))
                .rounded_full()
                .bg(c(if on { p.primary_fg } else { p.surface }))
                .shadow_sm(),
        )
}

pub fn checkbox(id: impl Into<ElementId>, on: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .mt(px(1.))
        .size(px(17.))
        .rounded(px(5.))
        .border_1()
        .border_color(c(if on { p.primary } else { p.line }))
        .bg(c(if on { p.primary } else { p.surface }))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(on, |this| {
            this.child(icon(IconName::Check, 12., c(p.primary_fg)))
        })
}

#[derive(Clone, Copy)]
pub enum Tone {
    Ok,
    Warn,
    Off,
}

pub fn pill(text: impl Into<SharedString>, tone: Tone, p: &Palette) -> Div {
    let color = match tone {
        Tone::Ok => p.ok,
        Tone::Warn => p.warn,
        Tone::Off => p.muted,
    };
    h_flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .h(px(22.))
        .px(px(9.))
        .rounded_full()
        .bg(ca(
            color,
            if matches!(tone, Tone::Off) {
                0.10
            } else {
                0.15
            },
        ))
        .text_color(c(color))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .child(div().size(px(6.)).rounded_full().bg(c(color)))
        .child(text.into())
}

pub fn status_tone(status: Status) -> Tone {
    match status {
        Status::Running { .. } => Tone::Ok,
        Status::Starting | Status::Restarting { .. } | Status::Crashed => Tone::Warn,
        Status::Stopped => Tone::Off,
    }
}

#[derive(Clone, Copy)]
pub enum TileSize {
    Sm,
    Md,
    Xl,
}

/// Tinted profile tile: a pale wash of the profile color with the icon drawn in that color.
/// `ring` is the color behind the tile, used for the running dot's outline.
pub fn tile(profile: &Profile, size: TileSize, live: bool, ring: u32, p: &Palette) -> Div {
    let (box_px, icon_px, radius) = match size {
        TileSize::Sm => (28., 14., 8.),
        TileSize::Md => (34., 17., 10.),
        TileSize::Xl => (38., 19., 11.),
    };
    let pc = profile.color.rgb();
    div()
        .flex_none()
        .relative()
        .size(px(box_px))
        .rounded(px(radius))
        .bg(mix(pc, p.surface, if p.dark { 0.22 } else { 0.17 }))
        .border_1()
        .border_color(ca(pc, 0.25))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(profile_icon(profile.icon), icon_px, c(pc)))
        .when(live, |this| {
            this.child(
                div()
                    .absolute()
                    .right(px(-3.))
                    .bottom(px(-3.))
                    .size(px(12.))
                    .rounded_full()
                    .border_2()
                    .border_color(c(ring))
                    .bg(c(p.ok)),
            )
        })
}

pub fn kbd(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .rounded(px(5.))
        .border_1()
        .border_b_2()
        .border_color(c(p.line))
        .bg(c(p.kbd))
        .text_color(c(p.fg))
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .line_height(px(17.))
        .child(text.into())
}

/// "Ctrl+Alt+2" → three key caps.
pub fn keys(accel: &str, p: &Palette) -> Div {
    h_flex()
        .gap(px(3.))
        .items_center()
        .children(accel.split('+').map(|k| kbd(display_key(k), p)))
}

pub fn display_key(k: &str) -> String {
    match k {
        "Backquote" => "`".into(),
        "Control" => "Ctrl".into(),
        "Super" | "Win" => "Win".into(),
        other => other.into(),
    }
}

pub fn card(p: &Palette) -> Div {
    v_flex()
        .gap(px(12.))
        .p(px(16.))
        .pt(px(14.))
        .rounded(px(12.))
        .border_1()
        .border_color(c(p.line))
        .bg(c(p.surface))
        .min_w_0()
}

/// Title + description on the left, a control on the right, separated from the row above.
pub fn set_row(
    title: impl Into<SharedString>,
    desc: Option<AnyElement>,
    right: impl IntoElement,
    first: bool,
    p: &Palette,
) -> Div {
    h_flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .when(!first, |this| {
            this.border_t_1().border_color(c(p.line)).pt(px(11.))
        })
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.into()),
                )
                .when_some(desc, |this, d| {
                    this.child(div().text_size(px(11.5)).text_color(c(p.muted)).child(d))
                }),
        )
        .child(right)
}

pub fn note(text: impl Into<SharedString>, p: &Palette) -> Div {
    h_flex()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(8.))
        .bg(c(p.accent_soft))
        .text_size(px(11.5))
        .text_color(c(p.fg))
        .child(
            div()
                .flex_none()
                .size(px(7.))
                .rounded_full()
                .bg(c(p.accent)),
        )
        .child(div().flex_1().min_w_0().child(text.into()))
}

/// Minimize / maximize / close, hit-tested natively so snap layouts work.
pub fn caption_buttons(p: &Palette, window: &Window) -> Div {
    let hover = mix(p.fg, p.bg, 0.07);
    let btn = |id: &'static str, name: IconName, area: WindowControlArea, danger: bool| {
        div()
            .id(id)
            .w(px(44.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(c(p.muted))
            .window_control_area(area)
            .hover(move |s| {
                if danger {
                    s.bg(rgb(0xC42B1C)).text_color(rgb(0xFFFFFF))
                } else {
                    s.bg(hover)
                }
            })
            .child(svg().path(name.path()).size(px(14.)).text_color(c(p.muted)))
    };
    h_flex()
        .absolute()
        .top_0()
        .right_0()
        .h(px(34.))
        .child(btn(
            "cap-min",
            IconName::Minus,
            WindowControlArea::Min,
            false,
        ))
        .child(btn(
            "cap-max",
            if window.is_maximized() {
                IconName::Copy
            } else {
                IconName::Square
            },
            WindowControlArea::Max,
            false,
        ))
        .child(btn(
            "cap-close",
            IconName::X,
            WindowControlArea::Close,
            true,
        ))
}

/// Inline notice (bottom-right of a panel) for the model's latest message.
pub fn toast(text: SharedString, error: bool, p: &Palette) -> Div {
    h_flex()
        .absolute()
        .bottom(px(42.))
        .right(px(20.))
        .max_w(px(420.))
        .gap(px(9.))
        .items_center()
        .px(px(14.))
        .py(px(10.))
        .rounded(px(10.))
        .bg(c(p.surface))
        .border_1()
        .border_color(c(if error { p.bad } else { p.line }))
        .shadow_lg()
        .text_size(px(12.5))
        .text_color(c(p.fg))
        .child(icon(
            if error {
                IconName::TriangleAlert
            } else {
                IconName::CircleCheck
            },
            15.,
            c(if error { p.bad } else { p.ok }),
        ))
        .child(div().min_w_0().child(text))
}
