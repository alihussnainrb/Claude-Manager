//! A4: the quick switcher — a floating palette that shows or launches a profile.

use cm_core::{Profile, ProfileId};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::widgets::*;
use crate::model::{AppModel, model};
use crate::theme::{self, Palette, c};

const WIDTH: f32 = 520.;
const HEIGHT: f32 = 340.;

pub fn open(cx: &mut App) -> Option<AnyWindowHandle> {
    let display = cx.primary_display();
    let origin = display
        .map(|d| {
            let b = d.bounds();
            point(
                b.origin.x + (b.size.width - px(WIDTH)) / 2.,
                b.origin.y + b.size.height * 0.18,
            )
        })
        .unwrap_or_else(|| point(px(400.), px(160.)));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            origin,
            size(px(WIDTH), px(HEIGHT)),
        ))),
        titlebar: None,
        kind: WindowKind::PopUp,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        app_id: Some("ClaudeManager".into()),
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Switcher::new(window, cx))
    }) {
        Ok((h, _)) => Some(h),
        Err(e) => {
            tracing::error!("switcher failed to open: {e}");
            None
        }
    }
}

struct Switcher {
    model: Entity<AppModel>,
    query: Entity<InputState>,
    selected: usize,
    was_active: bool,
    _subs: Vec<Subscription>,
}

struct Match {
    id: ProfileId,
    /// Character indices of the name that matched the query.
    hits: Vec<usize>,
}

impl Switcher {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = model(cx);
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find a profile"));
        query.update(cx, |s, cx| s.focus(window, cx));
        let subs = vec![
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.subscribe_in(&query, window, |this, _, ev, window, cx| match ev {
                InputEvent::Change => {
                    this.selected = 0;
                    cx.notify();
                }
                InputEvent::PressEnter { secondary, .. } => this.confirm(*secondary, window, cx),
                _ => {}
            }),
            // Disappear as soon as focus moves elsewhere (once it has had focus at all).
            cx.observe_window_activation(window, |this: &mut Self, window, _| {
                if window.is_window_active() {
                    this.was_active = true;
                } else if this.was_active {
                    window.remove_window();
                }
            }),
        ];
        window.activate_window();
        Self {
            model,
            query,
            selected: 0,
            was_active: false,
            _subs: subs,
        }
    }

    fn matches(&self, cx: &App) -> Vec<Match> {
        let m = self.model.read(cx);
        let q = self.query.read(cx).value().to_lowercase();
        let recency = |id: ProfileId| {
            m.focus_history
                .iter()
                .rposition(|(p, _)| *p == id)
                .map_or(-1, |i| i as i64)
        };
        let mut out: Vec<(bool, i64, &Profile, Vec<usize>)> = m
            .profiles
            .iter()
            .filter_map(|p| {
                fuzzy(&p.name, &q).map(|hits| (m.status(p.id).is_running(), recency(p.id), p, hits))
            })
            .collect();
        out.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.cmp(&a.1))
                .then(a.2.name.cmp(&b.2.name))
        });
        out.into_iter()
            .map(|(_, _, p, hits)| Match { id: p.id, hits })
            .collect()
    }

    fn confirm(&mut self, open_manager: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.matches(cx).into_iter().nth(self.selected) else {
            return;
        };
        window.remove_window();
        if open_manager {
            super::open_main_at(m.id, cx);
        } else {
            self.model.update(cx, |model, cx| {
                model.show(m.id);
                cx.notify();
            });
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.matches(cx).len();
        match ev.keystroke.key.as_str() {
            "escape" => window.remove_window(),
            "down" if n > 0 => {
                self.selected = (self.selected + 1) % n;
                cx.stop_propagation();
            }
            "up" if n > 0 => {
                self.selected = (self.selected + n - 1) % n;
                cx.stop_propagation();
            }
            _ => return,
        }
        cx.notify();
    }
}

impl Render for Switcher {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let matches = self.matches(cx);
        self.selected = self.selected.min(matches.len().saturating_sub(1));
        let m = self.model.read(cx);
        let rows: Vec<AnyElement> = matches
            .iter()
            .take(5)
            .enumerate()
            .filter_map(|(i, mt)| {
                let profile = m.profile(mt.id)?;
                Some(row(profile, &mt.hits, i == self.selected, m, p, i, cx).into_any_element())
            })
            .collect();
        let empty = rows.is_empty();
        v_flex()
            .id("switcher")
            .capture_key_down(cx.listener(Self::on_key))
            .size_full()
            .bg(c(p.surface))
            .border_1()
            .border_color(c(p.line))
            .rounded(px(16.))
            .overflow_hidden()
            .font_family(theme::FONT_UI)
            .text_color(c(p.fg))
            .text_size(px(13.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .px(px(18.))
                    .py(px(10.))
                    .border_b_1()
                    .border_color(c(p.line))
                    .child(icon(IconName::Search, 18., c(p.muted)))
                    .child(
                        div().flex_1().child(
                            Input::new(&self.query)
                                .appearance(false)
                                .text_size(px(15.5)),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .p(px(6.))
                    .gap(px(2.))
                    .children(rows)
                    .when(empty, |this| {
                        this.child(
                            div()
                                .p(px(16.))
                                .text_color(c(p.muted))
                                .child("No profile matches. Press Esc to close."),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap(px(16.))
                    .px(px(18.))
                    .py(px(9.))
                    .border_t_1()
                    .border_color(c(p.line))
                    .bg(crate::theme::mix(p.fg, p.surface, 0.03))
                    .text_size(px(11.))
                    .text_color(c(p.muted))
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .items_center()
                            .child(kbd("↑", p))
                            .child(kbd("↓", p))
                            .child("move"),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .items_center()
                            .child(kbd("Enter", p))
                            .child("show or launch"),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .items_center()
                            .child(kbd("Ctrl Enter", p))
                            .child("open in manager"),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .items_center()
                            .child(kbd("Esc", p))
                            .child("close"),
                    ),
            )
    }
}

fn row(
    profile: &Profile,
    hits: &[usize],
    highlighted: bool,
    m: &AppModel,
    p: &Palette,
    index: usize,
    cx: &Context<Switcher>,
) -> impl IntoElement + use<> {
    let running = m.status(profile.id).is_running();
    let name: Vec<AnyElement> = profile
        .name
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let hit = hits.contains(&i);
            div()
                .when(hit, |d| {
                    d.text_color(c(p.accent_ink))
                        .font_weight(FontWeight::EXTRA_BOLD)
                })
                .child(ch.to_string())
                .into_any_element()
        })
        .collect();
    let action: AnyElement = match (&profile.hotkey, highlighted) {
        (_, true) => h_flex()
            .gap(px(8.))
            .items_center()
            .child(if running { "Show" } else { "Launch" })
            .child(kbd("Enter", p))
            .into_any_element(),
        (Some(k), false) => keys(k, p).into_any_element(),
        (None, false) => div()
            .child(if running { "Show" } else { "Launch" })
            .into_any_element(),
    };
    let id = profile.id;
    h_flex()
        .id(SharedString::from(format!("sw-{id}")))
        .gap(px(11.))
        .items_center()
        .px(px(10.))
        .py(px(8.))
        .rounded(px(10.))
        .cursor_pointer()
        .when(highlighted, |this| this.bg(c(p.accent_soft)))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.selected = index;
            this.confirm(false, window, cx);
        }))
        .child(tile(
            profile,
            TileSize::Sm,
            running,
            if highlighted {
                p.accent_soft
            } else {
                p.surface
            },
            p,
        ))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(h_flex().font_weight(FontWeight::SEMIBOLD).children(name))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(c(p.muted))
                        .child(if running { "Running" } else { "Not running" }),
                ),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(c(p.muted))
                .child(action),
        )
}

/// Case-insensitive subsequence match; returns matched character indices.
fn fuzzy(name: &str, query: &str) -> Option<Vec<usize>> {
    if query.is_empty() {
        return Some(Vec::new());
    }
    let mut hits = Vec::new();
    let mut q = query.chars().peekable();
    for (i, ch) in name.to_lowercase().chars().enumerate() {
        if q.peek() == Some(&ch) {
            hits.push(i);
            q.next();
        }
    }
    q.peek().is_none().then_some(hits)
}

#[cfg(test)]
mod tests {
    #[test]
    fn fuzzy_matches() {
        assert_eq!(super::fuzzy("Work", "wo"), Some(vec![0, 1]));
        assert_eq!(
            super::fuzzy("Client – Northwind", "cnw"),
            Some(vec![0, 4, 14])
        );
        assert_eq!(super::fuzzy("Default", "xyz"), None);
        assert_eq!(super::fuzzy("Default", ""), Some(vec![]));
    }
}
