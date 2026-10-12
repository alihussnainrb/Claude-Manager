//! A6: "Which profile were you signing in to?" — shown only when routing is ambiguous.

use cm_core::ProfileId;
use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::widgets::*;
use crate::broker::{LinkKind, link_kind};
use crate::model::{AppModel, format_uptime, model};
use crate::theme::{self, c, mix};

pub fn open(url: String, candidates: Vec<ProfileId>, cx: &mut App) -> Option<AnyWindowHandle> {
    let height = 250. + 54. * candidates.len().min(6) as f32;
    let mut options = super::chrome_options(450., height, None, cx);
    options.kind = WindowKind::PopUp;
    options.is_resizable = false;
    options.is_minimizable = false;
    options.focus = true;
    match gpui_kit::open_window(options, cx, |_, cx| {
        cx.new(|cx| Router::new(url, candidates, cx))
    }) {
        Ok((h, _)) => Some(h),
        Err(e) => {
            tracing::error!("router failed to open: {e}");
            None
        }
    }
}

struct Router {
    model: Entity<AppModel>,
    url: String,
    candidates: Vec<ProfileId>,
    choice: usize,
    _sub: Subscription,
}

impl Router {
    fn new(url: String, candidates: Vec<ProfileId>, cx: &mut Context<Self>) -> Self {
        let model = model(cx);
        // Prefer a profile that's still waiting for a sign-in.
        let choice = {
            let m = model.read(cx);
            candidates
                .iter()
                .position(|id| {
                    m.profile(*id)
                        .is_some_and(|p| p.cached_account_uuid.is_none())
                })
                .unwrap_or(0)
        };
        let sub = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            url,
            candidates,
            choice,
            _sub: sub,
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.candidates.get(self.choice).copied() else {
            return;
        };
        let url = self.url.clone();
        self.model.update(cx, |m, cx| {
            m.handoff(id, Some(&url));
            m.note_focus(id);
            cx.notify();
        });
        window.remove_window();
    }
}

impl Render for Router {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let m = self.model.read(cx);
        let kind = link_kind(&self.url);
        let (kind_icon, kind_label) = match kind {
            LinkKind::Account => (IconName::User, "Account sign-in"),
            LinkKind::Connector => (IconName::Plug, "Connector sign-in"),
        };
        let running = m.running_count();
        let lede = if running == 0 {
            "Your browser sent a sign-in back to Claude, but no profile is open. Pick one to open it with this sign-in.".to_string()
        } else {
            format!(
                "Your browser sent a sign-in back to Claude and {running} profile{} open. Pick one so the account lands in the right place.",
                if running == 1 { " is" } else { "s are" }
            )
        };
        let choices: Vec<AnyElement> = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let profile = m.profile(*id)?;
                let on = i == self.choice;
                let hint: String = if profile.cached_account_uuid.is_none() {
                    "Not signed in yet".into()
                } else if let Some((_, at)) = m.focus_history.iter().rev().find(|(p, _)| p == id) {
                    format!("Used {} ago", format_uptime(*at))
                } else if m.status(*id).is_running() {
                    "Running".into()
                } else {
                    "Not running".into()
                };
                Some(
                    h_flex()
                        .id(SharedString::from(format!("choice-{id}")))
                        .gap(px(10.))
                        .items_center()
                        .px(px(10.))
                        .py(px(8.))
                        .rounded(px(11.))
                        .border_1()
                        .border_color(c(if on { p.accent } else { p.line }))
                        .when(on, |this| this.bg(c(p.accent_soft)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choice = i;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .size(px(16.))
                                .rounded_full()
                                .border_color(c(if on { p.primary } else { p.line }))
                                .when(on, |d| d.border(px(5.)))
                                .when(!on, |d| d.border_1())
                                .bg(c(p.surface)),
                        )
                        .child(tile(profile, TileSize::Sm, false, p.surface, p))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .truncate()
                                        .child(profile.name.clone()),
                                )
                                .child(
                                    div().text_size(px(11.5)).text_color(c(p.muted)).child(hint),
                                ),
                        )
                        .when(on && i == 0, |this| {
                            this.child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(c(p.accent_ink))
                                    .child("Most likely"),
                            )
                        })
                        .into_any_element(),
                )
            })
            .collect();
        let target = self
            .candidates
            .get(self.choice)
            .and_then(|id| m.profile(*id))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        v_flex()
            .size_full()
            .bg(c(p.surface))
            .font_family(theme::FONT_UI)
            .text_size(px(13.))
            .text_color(c(p.fg))
            .relative()
            .child(
                h_flex()
                    .h(px(34.))
                    .pl(px(14.))
                    .gap(px(8.))
                    .items_center()
                    .text_size(px(12.))
                    .text_color(c(p.muted))
                    .window_control_area(WindowControlArea::Drag)
                    .child(logo(16., c(p.accent)))
                    .child("Claude Manager"),
            )
            .child(
                div()
                    .id("router-close")
                    .absolute()
                    .top_0()
                    .right_0()
                    .w(px(44.))
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .window_control_area(WindowControlArea::Close)
                    .hover(|s| s.bg(rgb(0xC42B1C)))
                    .child(icon(IconName::X, 14., c(p.muted))),
            )
            .child(
                v_flex()
                    .px(px(20.))
                    .pb(px(18.))
                    .gap(px(12.))
                    .flex_1()
                    .child(heading("Which profile were you signing in to?", 17., p))
                    .child(div().text_size(px(12.5)).text_color(c(p.muted)).child(lede))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .px(px(10.))
                            .py(px(8.))
                            .rounded(px(9.))
                            .bg(c(p.code))
                            .text_size(px(12.))
                            .child(icon(kind_icon, 15., c(p.accent_ink)))
                            .child(div().font_weight(FontWeight::BOLD).child(kind_label))
                            .child(
                                mono(format!("{}?…", crate::broker::redact(&self.url)), p)
                                    .truncate(),
                            ),
                    )
                    .child(v_flex().gap(px(6.)).children(choices))
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .justify_end()
                            .child(
                                button("r-cancel", "Cancel", None, Btn::Secondary, false, p)
                                    .on_click(|_, window, _| window.remove_window()),
                            )
                            .child(
                                button(
                                    "r-send",
                                    format!("Send to {target}"),
                                    None,
                                    Btn::Primary,
                                    false,
                                    p,
                                )
                                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                            ),
                    ),
            )
            .border_1()
            .border_color(mix(p.fg, p.surface, 0.12))
    }
}
