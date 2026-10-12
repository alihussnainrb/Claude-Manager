//! A1–A3, A7: the manager window — profile sidebar, profile header and its tabs.

use std::collections::HashMap;

use cm_core::{Profile, ProfileColor, ProfileIcon, ProfileId, now_millis};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{EditorState, Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::widgets::*;
use crate::model::{AppModel, Status, format_uptime, model};
use crate::theme::{self, Palette, c, mix};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Code,
    Data,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum McpTarget {
    Code,
    Desktop,
}

pub(super) struct NewProfile {
    pub name: Entity<InputState>,
    pub icon: ProfileIcon,
    pub color: ProfileColor,
    pub isolate: bool,
    pub seed: bool,
    pub error: Option<SharedString>,
}

pub struct MainView {
    pub(super) model: Entity<AppModel>,
    pub(super) tab: Tab,
    pub(super) name_input: Entity<InputState>,
    pub(super) name_for: Option<ProfileId>,
    pub(super) dialog: Option<NewProfile>,
    pub(super) recording: bool,
    pub(super) focus: FocusHandle,
    pub(super) confirm_remove: Option<bool>,
    pub(super) menu_open: bool,
    pub(super) mcp_target: McpTarget,
    pub(super) mcp_editor: Entity<EditorState>,
    pub(super) mcp_loaded_for: Option<(ProfileId, McpTarget)>,
    pub(super) mcp_issues: Vec<cm_core::mcp::McpIssue>,
    pub(super) mcp_dirty: bool,
    pub(super) seed_results: HashMap<ProfileId, SharedString>,
    settings: Option<Entity<super::settings_window::SettingsView>>,
    _settings_sub: Option<Subscription>,
    toast_for: Option<i64>,
    _toast_timer: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl MainView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = model(cx);
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Profile name"));
        let mcp_editor = cx.new(|cx| EditorState::new(window, cx).language("json"));
        let mut subs = vec![
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| theme::sync(window, cx)),
        ];
        subs.push(
            cx.subscribe_in(&name_input, window, |this, state, ev, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    let name = state.read(cx).value().trim().to_string();
                    if let (Some(id), false) = (this.name_for, name.is_empty()) {
                        this.model.update(cx, |m, cx| {
                            m.update_profile(id, |p| p.name = name);
                            cx.notify();
                        });
                    }
                }
            }),
        );
        subs.push(
            cx.subscribe_in(&mcp_editor, window, |this, state, ev, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.mcp_dirty = true;
                    this.mcp_issues = cm_core::mcp::parse_servers(&state.read(cx).value())
                        .err()
                        .unwrap_or_default();
                    cx.notify();
                }
            }),
        );
        window.on_window_should_close(cx, |_, cx| {
            if !crate::model::model(cx).read(cx).settings.close_to_tray {
                cx.quit();
            }
            true
        });
        Self {
            model,
            tab: Tab::General,
            name_input,
            name_for: None,
            dialog: None,
            recording: false,
            focus: cx.focus_handle(),
            confirm_remove: None,
            menu_open: false,
            mcp_target: McpTarget::Code,
            mcp_editor,
            mcp_loaded_for: None,
            mcp_issues: Vec::new(),
            mcp_dirty: false,
            seed_results: HashMap::new(),
            settings: None,
            _settings_sub: None,
            toast_for: None,
            _toast_timer: None,
            _subs: subs,
        }
    }

    /// Show Settings as a dialog over this window.
    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            return;
        }
        self.dialog = None;
        self.menu_open = false;
        let view = cx.new(super::settings_window::SettingsView::new);
        self._settings_sub = Some(cx.subscribe(
            &view,
            |this, _, _: &super::settings_window::SettingsEvent, cx| {
                this.settings = None;
                this._settings_sub = None;
                cx.notify();
            },
        ));
        view.read(cx).focus_handle().focus(window, cx);
        self.settings = Some(view);
        cx.notify();
    }

    pub(super) fn select(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        self.confirm_remove = None;
        self.menu_open = false;
        self.recording = false;
        self.model.update(cx, |m, cx| {
            m.selected = id;
            m.refresh_folder_size(id);
            cx.notify();
        });
    }

    /// Keep per-profile editors in step with the selection.
    fn sync_selection(&mut self, profile: &Profile, window: &mut Window, cx: &mut Context<Self>) {
        if self.name_for != Some(profile.id) {
            self.name_for = Some(profile.id);
            let name = profile.name.clone();
            self.name_input
                .update(cx, |s, cx| s.set_value(name, window, cx));
            self.model.read(cx).refresh_folder_size(profile.id);
        }
        if self.tab == Tab::Code && self.mcp_loaded_for != Some((profile.id, self.mcp_target)) {
            self.load_mcp(profile, window, cx);
        }
    }

    pub(super) fn mcp_file(&self, profile: &Profile, cx: &App) -> std::path::PathBuf {
        let m = self.model.read(cx);
        match self.mcp_target {
            McpTarget::Code => {
                if profile.claude_code.enabled {
                    profile.claude_code.config_dir.join(".claude.json")
                } else {
                    m.paths.default_code_state_file()
                }
            }
            McpTarget::Desktop => m.data_dir(profile).join("claude_desktop_config.json"),
        }
    }

    pub(super) fn load_mcp(
        &mut self,
        profile: &Profile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let file = self.mcp_file(profile, cx);
        let text = match cm_core::mcp::read_servers(&file) {
            Ok(s) => cm_core::mcp::to_editor_text(&s),
            Err(e) => {
                self.model.update(cx, |m, _| m.notify_error(format!("{e}")));
                "{}".into()
            }
        };
        self.mcp_editor
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.mcp_loaded_for = Some((profile.id, self.mcp_target));
        self.mcp_issues.clear();
        self.mcp_dirty = false;
    }

    pub(super) fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let m = self.model.read(cx);
        let used_colors: Vec<_> = m.profiles.iter().map(|p| p.color).collect();
        let used_icons: Vec<_> = m.profiles.iter().map(|p| p.icon).collect();
        let color = ProfileColor::ALL
            .into_iter()
            .find(|c| !used_colors.contains(c))
            .unwrap_or_default();
        let icon = ProfileIcon::ALL
            .into_iter()
            .find(|i| !used_icons.contains(i))
            .unwrap_or_default();
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. Work, Personal, Client – Northwind")
        });
        name.update(cx, |s, cx| s.focus(window, cx));
        self._subs
            .push(cx.subscribe_in(&name, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.submit_dialog(false, window, cx);
                }
            }));
        self.dialog = Some(NewProfile {
            name,
            icon,
            color,
            isolate: true,
            seed: true,
            error: None,
        });
        cx.notify();
    }

    fn submit_dialog(&mut self, sign_in: bool, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.dialog.as_mut() else {
            return;
        };
        let name = d.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            d.error = Some("Enter a name for the profile.".into());
            cx.notify();
            return;
        }
        if self
            .model
            .read(cx)
            .profiles
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case(&name))
        {
            d.error = Some(format!("A profile named {name} already exists.").into());
            cx.notify();
            return;
        }
        let (icon, color, isolate, seed) = (d.icon, d.color, d.isolate, d.seed);
        self.dialog = None;
        self.tab = Tab::General;
        let id = self.model.update(cx, |m, cx| {
            let id = m.create_profile(&name, icon, color, isolate);
            if sign_in {
                m.mark_pending_sign_in(id);
                m.launch(id);
            }
            m.notify_info(format!("Created {name}"));
            cx.notify();
            id
        });
        if isolate && seed {
            self.copy_setup(id, cx);
        }
    }

    /// Copy the main Claude Code setup into a profile's config dir, off the UI thread.
    pub(super) fn copy_setup(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        let m = self.model.read(cx);
        let Some(p) = m.profile(id) else { return };
        let (src, state, dst) = (
            m.paths.default_code_config_dir(),
            m.paths.default_code_state_file(),
            p.claude_code.config_dir.clone(),
        );
        let task = cx.background_spawn(async move { cm_core::seed::seed(&src, &state, &dst) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let text: SharedString = match result {
                    Ok(r) if r.copied.is_empty() && r.mcp_servers_added.is_empty() => {
                        "Already up to date. Login not copied.".into()
                    }
                    Ok(r) => format!(
                        "Copied just now: {} files and {} MCP servers. Login not copied.",
                        r.copied.len(),
                        r.mcp_servers_added.len()
                    )
                    .into(),
                    Err(e) => format!("Copy failed: {e}").into(),
                };
                this.seed_results.insert(id, text);
                cx.notify();
            });
        })
        .detach();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if self.recording {
            cx.stop_propagation();
            let id = self.model.read(cx).selected;
            match ks.key.as_str() {
                "escape" => self.recording = false,
                "backspace" | "delete" => {
                    self.recording = false;
                    self.model.update(cx, |m, cx| {
                        m.update_profile(id, |p| p.hotkey = None);
                        cx.notify();
                    });
                }
                _ => {
                    if let Some(accel) = accelerator(ks) {
                        self.recording = false;
                        self.model.update(cx, |m, cx| {
                            let clash = m
                                .profiles
                                .iter()
                                .find(|p| p.id != id && p.hotkey.as_deref() == Some(&accel));
                            let switcher = m
                                .settings
                                .switcher_hotkey
                                .as_deref()
                                .is_some_and(|s| same_accel(s, &accel));
                            if let Some(other) = clash {
                                let msg =
                                    format!("{accel} is already the shortcut for {}.", other.name);
                                m.notify_error(msg);
                            } else if switcher {
                                m.notify_error(format!(
                                    "{accel} opens the quick switcher. Pick another."
                                ));
                            } else {
                                m.update_profile(id, |p| p.hotkey = Some(accel));
                            }
                            cx.notify();
                        });
                    }
                }
            }
            cx.notify();
            return;
        }
        if ks.modifiers.control && ks.key == "k" {
            super::open_switcher(cx);
        }
        if ks.modifiers.control && ks.key == "n" && self.dialog.is_none() {
            self.open_dialog(window, cx);
        }
        if ks.key == "escape"
            && (self.dialog.take().is_some() || self.menu_open || self.confirm_remove.is_some())
        {
            self.menu_open = false;
            self.confirm_remove = None;
            cx.notify();
        }
    }

    // ---- sidebar ----------------------------------------------------------------------------

    fn render_sidebar(&self, p: &Palette, cx: &Context<Self>) -> impl IntoElement + use<> {
        let m = self.model.read(cx);
        let rows: Vec<AnyElement> = m
            .profiles
            .iter()
            .map(|profile| self.render_row(profile, m, p, cx).into_any_element())
            .collect();
        let orphan = m.orphans.first().cloned();
        let more = m.orphans.len().saturating_sub(1);
        v_flex()
            .w(px(256.))
            .flex_none()
            .h_full()
            .px(px(10.))
            .pt(px(14.))
            .pb(px(10.))
            .gap(px(3.))
            .text_color(c(p.side_fg))
            .child(
                h_flex()
                    .gap(px(9.))
                    .items_center()
                    .px(px(8.))
                    .pb(px(12.))
                    .h(px(36.))
                    .window_control_area(WindowControlArea::Drag)
                    .child(logo(20., c(p.accent)))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(13.5))
                            .child("Claude Manager"),
                    ),
            )
            .child(
                h_flex()
                    .id("search")
                    .mx(px(2.))
                    .mb(px(8.))
                    .h(px(32.))
                    .px(px(10.))
                    .gap(px(8.))
                    .items_center()
                    .rounded(px(9.))
                    .bg(mix(p.side_fg, p.side, 0.06))
                    .text_color(c(p.side_muted))
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .on_click(|_, _, cx| super::open_switcher(cx))
                    .child(icon(IconName::Search, 14., c(p.side_muted)))
                    .child("Find a profile")
                    .child(div().ml_auto().text_size(px(10.5)).child("Ctrl K")),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px(px(10.))
                    .py(px(6.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(c(p.side_muted))
                    .child("PROFILES")
                    .child(
                        div()
                            .id("add-profile")
                            .rounded(px(5.))
                            .p(px(2.))
                            .cursor_pointer()
                            .hover(|s| s.bg(mix(p.side_fg, p.side, 0.08)))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_dialog(window, cx)),
                            )
                            .child(icon(IconName::Plus, 15., c(p.side_muted))),
                    ),
            )
            .child(
                v_flex()
                    .id("profile-list")
                    .gap(px(3.))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .when_some(orphan, |this, o| {
                let dir = o.user_data_dir.clone();
                let dir2 = o.user_data_dir.clone();
                let shown = shorten_path(&o.user_data_dir, &m.paths);
                this.child(
                    v_flex()
                        .mx(px(2.))
                        .mb(px(6.))
                        .p(px(10.))
                        .gap(px(7.))
                        .rounded(px(10.))
                        .border_1()
                        .border_dashed()
                        .border_color(mix(p.side_fg, p.side, 0.25))
                        .text_size(px(12.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(if more > 0 {
                            format!("{} Claude windows aren't in a profile", more + 1)
                        } else {
                            "1 Claude window isn't in a profile".into()
                        }))
                        .child(mono(shown, p).text_size(px(10.5)))
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .child(
                                    button("adopt", "Add as profile", None, Btn::Primary, true, p)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            let id = this.model.update(cx, |m, cx| {
                                                let id = m.adopt(&dir);
                                                cx.notify();
                                                id
                                            });
                                            this.select(id, cx);
                                        })),
                                )
                                .child(
                                    button("ignore", "Ignore", None, Btn::Secondary, true, p)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.model.update(cx, |m, cx| {
                                                m.ignore_orphan(&dir2);
                                                cx.notify();
                                            })
                                        })),
                                ),
                        ),
                )
            })
            .child(
                h_flex()
                    .id("open-settings")
                    .gap(px(10.))
                    .items_center()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(10.))
                    .text_size(px(12.5))
                    .text_color(c(p.side_muted))
                    .cursor_pointer()
                    .hover(|s| s.bg(mix(p.side_fg, p.side, 0.06)))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx)))
                    .child(icon(IconName::Settings, 16., c(p.side_muted)))
                    .child("Settings"),
            )
    }

    fn render_row(
        &self,
        profile: &Profile,
        m: &AppModel,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = profile.id;
        let selected = m.selected == id;
        let status = m.status(id);
        let (line, warn, alert) = status_line(profile, status, m);
        let ring = if selected { p.side_sel } else { p.side };
        h_flex()
            .id(SharedString::from(format!("row-{id}")))
            .relative()
            .gap(px(10.))
            .items_center()
            .px(px(8.))
            .py(px(7.))
            .rounded(px(10.))
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(c(p.side_sel))
                    .border_1()
                    .border_color(c(p.side_line))
                    .shadow_xs()
                    .child(
                        div()
                            .absolute()
                            .left(px(-11.))
                            .top(px(12.))
                            .bottom(px(12.))
                            .w(px(3.))
                            .rounded_r(px(3.))
                            .bg(c(p.accent)),
                    )
            })
            .when(!selected, |this| {
                this.border_1()
                    .border_color(c(p.side))
                    .hover(|s| s.bg(mix(p.side_fg, p.side, 0.04)))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(id, cx)))
            .child(tile(profile, TileSize::Md, status.is_running(), ring, p))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(profile.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(5.))
                            .items_center()
                            .text_size(px(11.5))
                            .text_color(c(if warn { p.warn } else { p.side_muted }))
                            .when(alert, |this| {
                                this.child(icon(IconName::TriangleAlert, 12., c(p.warn)))
                            })
                            .child(div().truncate().child(line)),
                    ),
            )
    }

    // ---- header -----------------------------------------------------------------------------

    fn render_hero(
        &self,
        profile: &Profile,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let m = self.model.read(cx);
        let id = profile.id;
        let status = m.status(id);
        let signed_in = profile.cached_account_uuid.is_some();
        let meta: SharedString = match (&profile.cached_account_uuid, status) {
            (_, Status::Restarting { .. } | Status::Crashed) => "Last crash just now".into(),
            (Some(acc), _) => format!("Signed in · account {}", short_account(acc)).into(),
            (None, Status::Running { .. }) => "Not signed in yet".into(),
            (None, _) => match profile.last_launched_at {
                Some(t) => format!("Last used {} ago", format_uptime(t)).into(),
                None => "Never launched".into(),
            },
        };
        let pill_text: SharedString = match status {
            Status::Running { .. } => "Running".into(),
            Status::Starting => "Starting…".into(),
            Status::Restarting { attempt } => format!("Restarting ({attempt} of 3)").into(),
            Status::Crashed => "Crashed".into(),
            Status::Stopped => "Not running".into(),
        };
        let primary: Option<Stateful<Div>> = match status {
            Status::Running { .. } if !signed_in => Some(
                button(
                    "primary",
                    "Sign in",
                    Some(IconName::User),
                    Btn::Primary,
                    true,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |m, cx| {
                        m.mark_pending_sign_in(id);
                        m.show(id);
                        cx.notify();
                    })
                })),
            ),
            Status::Running { .. } => Some(
                button(
                    "primary",
                    "Show window",
                    Some(IconName::ExternalLink),
                    Btn::Primary,
                    true,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |m, cx| {
                        m.show(id);
                        cx.notify();
                    })
                })),
            ),
            Status::Restarting { .. } => None,
            Status::Starting => None,
            Status::Crashed | Status::Stopped => Some(
                button(
                    "primary",
                    if status == Status::Crashed {
                        "Relaunch"
                    } else {
                        "Launch"
                    },
                    Some(IconName::Play),
                    Btn::Primary,
                    true,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |m, cx| {
                        if m.profile(id)
                            .is_some_and(|p| p.cached_account_uuid.is_none())
                        {
                            m.mark_pending_sign_in(id);
                        }
                        m.launch(id);
                        cx.notify();
                    })
                })),
            ),
        };
        let secondary: Option<Stateful<Div>> = match status {
            Status::Running { .. } => Some(
                button(
                    "quit",
                    "Quit",
                    Some(IconName::Square),
                    Btn::Secondary,
                    true,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |m, cx| {
                        m.quit(id);
                        cx.notify();
                    })
                })),
            ),
            Status::Restarting { .. } => Some(
                button(
                    "stop-restart",
                    "Stop restarting",
                    Some(IconName::Square),
                    Btn::Secondary,
                    true,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model.update(cx, |m, cx| {
                        m.stop_restarting(id);
                        cx.notify();
                    })
                })),
            ),
            _ => None,
        };
        h_flex()
            .relative()
            .gap(px(12.))
            .items_center()
            .px(px(28.))
            .pt(px(26.))
            .pb(px(12.))
            .child(tile(profile, TileSize::Xl, false, p.bg, p))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(px(22.))
                            .truncate()
                            .child(profile.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .items_center()
                            .text_size(px(12.))
                            .text_color(c(p.muted))
                            .child(pill(pill_text, status_tone(status), p))
                            .child(div().truncate().child(meta)),
                    ),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .self_end()
                    .flex_none()
                    .children(primary)
                    .children(secondary)
                    .child(
                        icon_button("more", IconName::Ellipsis, p)
                            .size(px(28.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.menu_open = !this.menu_open;
                                cx.notify();
                            })),
                    ),
            )
            .when(self.menu_open, |this| {
                this.child(self.render_more_menu(profile, p, cx))
            })
    }

    fn render_more_menu(
        &self,
        profile: &Profile,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = profile.id;
        let dir = self.model.read(cx).data_dir(profile);
        let item = |key: &'static str, label: &'static str, name: IconName, danger: bool| {
            h_flex()
                .id(key)
                .gap(px(10.))
                .items_center()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(7.))
                .text_size(px(12.5))
                .text_color(c(if danger { p.bad } else { p.fg }))
                .cursor_pointer()
                .hover(|s| s.bg(c(p.accent_soft)))
                .child(icon(name, 15., c(if danger { p.bad } else { p.muted })))
                .child(label)
        };
        v_flex()
            .absolute()
            .top(px(64.))
            .right(px(28.))
            .w(px(220.))
            .p(px(6.))
            .rounded(px(12.))
            .bg(c(p.surface))
            .border_1()
            .border_color(c(p.line))
            .shadow_lg()
            .occlude()
            .child(
                item("m-signin", "Sign in again", IconName::User, false).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.menu_open = false;
                        this.model.update(cx, |m, cx| {
                            m.mark_pending_sign_in(id);
                            m.show(id);
                            cx.notify();
                        });
                    },
                )),
            )
            .child(
                item("m-folder", "Open data folder", IconName::Folder, false).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.menu_open = false;
                        let _ = cm_win::shell::open_path(&dir);
                        cx.notify();
                    }),
                ),
            )
            .when(!profile.is_default(), |this| {
                this.child(
                    item("m-remove", "Remove…", IconName::TriangleAlert, true).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.menu_open = false;
                            this.tab = Tab::Data;
                            this.confirm_remove = Some(false);
                            cx.notify();
                        }),
                    ),
                )
            })
    }

    fn render_tabs(&self, p: &Palette, cx: &Context<Self>) -> impl IntoElement + use<> {
        let tab = |id: &'static str, label: &'static str, t: Tab| {
            let on = self.tab == t;
            div()
                .id(id)
                .px(px(13.))
                .py(px(6.))
                .rounded_full()
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |this| {
                    this.bg(c(p.accent_soft))
                        .text_color(c(p.accent_ink))
                        .font_weight(FontWeight::BOLD)
                })
                .when(!on, |this| {
                    this.text_color(c(p.muted))
                        .font_weight(FontWeight::MEDIUM)
                        .hover(|s| s.bg(mix(p.fg, p.bg, 0.05)))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = t;
                    this.confirm_remove = None;
                    cx.notify();
                }))
                .child(label)
        };
        h_flex()
            .gap(px(4.))
            .px(px(28.))
            .pt(px(4.))
            .pb(px(14.))
            .child(tab("tab-general", "General", Tab::General))
            .child(tab("tab-code", "Claude Code", Tab::Code))
            .child(tab("tab-data", "Data", Tab::Data))
    }

    fn render_status_bar(&self, p: &Palette, cx: &Context<Self>) -> impl IntoElement + use<> {
        let m = self.model.read(cx);
        let update_note: Option<(SharedString, bool)> = match &m.update {
            crate::updater::UpdateState::Downloading(r) => {
                Some((format!("Downloading {}…", r.version).into(), false))
            }
            crate::updater::UpdateState::Ready(r, _) => {
                Some((format!("Restart to update to {}", r.version).into(), true))
            }
            _ => None,
        };
        let (ok, install): (bool, SharedString) = match &m.install {
            Some(i) => (
                true,
                format!(
                    "Claude Desktop {} · {}",
                    i.version,
                    if matches!(i.kind, cm_win::appx::InstallKind::Msix { .. }) {
                        "Microsoft Store"
                    } else {
                        "Installer"
                    }
                )
                .into(),
            ),
            None if !m.scanned_once => (true, "Looking for Claude Desktop…".into()),
            None => (
                false,
                "Claude Desktop not found. Install it from the Microsoft Store.".into(),
            ),
        };
        let sep = || div().w(px(1.)).h(px(12.)).bg(c(p.line));
        h_flex()
            .flex_none()
            .h(px(30.))
            .mt(px(14.))
            .px(px(28.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(c(p.line))
            .text_size(px(11.5))
            .text_color(c(p.muted))
            .child(
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(c(if ok { p.ok } else { p.bad })),
            )
            .child(install)
            .child(sep())
            .child(if m.settings.deep_link_broker {
                "Sign-in routing on"
            } else {
                "Sign-in routing off"
            })
            .child(sep())
            .child(format!("{} running", m.running_count()))
            .child(div().flex_1())
            .when_some(update_note, |this, (text, ready)| {
                this.child(
                    h_flex()
                        .id("update-note")
                        .gap(px(6.))
                        .items_center()
                        .px(px(8.))
                        .py(px(2.))
                        .rounded(px(6.))
                        .text_color(c(p.accent_ink))
                        .font_weight(FontWeight::SEMIBOLD)
                        .when(ready, |this| {
                            this.cursor_pointer()
                                .bg(c(p.accent_soft))
                                .on_click(|_, _, cx| crate::app::install_update(cx))
                        })
                        .child(icon(IconName::RefreshCw, 12., c(p.accent_ink)))
                        .child(text),
                )
            })
    }

    // ---- new profile dialog -----------------------------------------------------------------

    fn render_dialog(
        &self,
        d: &NewProfile,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let m = self.model.read(cx);
        let typed = d.name.read(cx).value().trim().to_string();
        let slug =
            cm_core::slug::unique_slug(if typed.is_empty() { "profile" } else { &typed }, |s| {
                m.profiles.iter().any(|p| p.slug == s)
            });
        let data_dir = format!(r"%APPDATA%\Claude-{slug}");
        let code_dir = format!(r"%USERPROFILE%\.claude-{slug}");
        let icons = ProfileIcon::ALL.into_iter().map(|i| {
            let on = d.icon == i;
            icon_option(
                SharedString::from(format!("np-icon-{}", i.label())),
                i,
                on,
                p,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(d) = this.dialog.as_mut() {
                    d.icon = i;
                }
                cx.notify();
            }))
        });
        let colors = ProfileColor::ALL.into_iter().map(|col| {
            swatch(
                SharedString::from(format!("np-color-{}", col.label())),
                col,
                d.color == col,
                p,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(d) = this.dialog.as_mut() {
                    d.color = col;
                }
                cx.notify();
            }))
        });
        let check_row = |id: &'static str, on: bool, title: &'static str, hint: &'static str| {
            h_flex()
                .id(id)
                .gap(px(9.))
                .items_start()
                .cursor_pointer()
                .child(checkbox(SharedString::from(format!("{id}-box")), on, p))
                .child(
                    v_flex()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(div().text_size(px(11.5)).text_color(c(p.muted)).child(hint)),
                )
        };
        div()
            .absolute()
            .inset_0()
            .bg(gpui_kit::hsla(0.07, 0.3, 0.12, if p.dark { 0.55 } else { 0.34 }))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .child(
                v_flex()
                    .w(px(470.))
                    .rounded(px(16.))
                    .bg(c(p.surface))
                    .border_1()
                    .border_color(c(p.line))
                    .shadow_2xl()
                    .overflow_hidden()
                    .child(
                        v_flex()
                            .px(px(22.))
                            .pt(px(20.))
                            .pb(px(4.))
                            .gap(px(4.))
                            .child(heading("New profile", 18., p))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(c(p.muted))
                                    .child("A separate Claude with its own sign-in, chats, settings and connectors."),
                            ),
                    )
                    .child(
                        v_flex()
                            .px(px(22.))
                            .pt(px(14.))
                            .pb(px(18.))
                            .gap(px(14.))
                            .child(
                                v_flex()
                                    .gap(px(6.))
                                    .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child("Name"))
                                    .child(Input::new(&d.name))
                                    .when_some(d.error.clone(), |this, e| {
                                        this.child(div().text_size(px(11.5)).text_color(c(p.bad)).child(e))
                                    }),
                            )
                            .child(
                                h_flex()
                                    .gap(px(22.))
                                    .flex_wrap()
                                    .child(
                                        v_flex()
                                            .gap(px(6.))
                                            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child("Icon"))
                                            .child(h_flex().gap(px(6.)).children(icons)),
                                    )
                                    .child(
                                        v_flex()
                                            .gap(px(6.))
                                            .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child("Color"))
                                            .child(h_flex().h(px(32.)).items_center().gap(px(9.)).children(colors)),
                                    ),
                            )
                            .child(
                                check_row(
                                    "np-isolate",
                                    d.isolate,
                                    "Give Claude Code its own settings",
                                    "Separate memory, skills, plugins and MCP servers for this profile.",
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(d) = this.dialog.as_mut() {
                                        d.isolate = !d.isolate;
                                    }
                                    cx.notify();
                                })),
                            )
                            .when(d.isolate, |this| {
                                this.child(
                                    check_row(
                                        "np-seed",
                                        d.seed,
                                        "Copy my Claude Code setup from Default",
                                        "Settings, skills, agents and MCP servers. Never your login or chat history.",
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(d) = this.dialog.as_mut() {
                                            d.seed = !d.seed;
                                        }
                                        cx.notify();
                                    })),
                                )
                            })
                            .child(
                                v_flex()
                                    .gap(px(5.))
                                    .px(px(12.))
                                    .py(px(10.))
                                    .rounded(px(10.))
                                    .bg(c(p.code))
                                    .child(folder_line("Claude data", data_dir, p))
                                    .when(d.isolate, |this| this.child(folder_line("Claude Code", code_dir, p))),
                            ),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap(px(8.))
                            .px(px(22.))
                            .py(px(12.))
                            .border_t_1()
                            .border_color(c(p.line))
                            .bg(mix(p.fg, p.surface, 0.03))
                            .child(button("np-cancel", "Cancel", None, Btn::Secondary, false, p).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.dialog = None;
                                    cx.notify();
                                },
                            )))
                            .child(button("np-create", "Create", None, Btn::Secondary, false, p).on_click(cx.listener(
                                |this, _, window, cx| this.submit_dialog(false, window, cx),
                            )))
                            .child(
                                button("np-create-sign", "Create and sign in", Some(IconName::Play), Btn::Primary, false, p)
                                    .on_click(cx.listener(|this, _, window, cx| this.submit_dialog(true, window, cx))),
                            ),
                    ),
            )
    }

    fn render_toast(
        &mut self,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let notice = self.model.read(cx).notice.clone()?;
        const SHOW_MS: i64 = 6000;
        let age = now_millis() - notice.at;
        if age >= SHOW_MS {
            return None;
        }
        if self.toast_for != Some(notice.at) {
            self.toast_for = Some(notice.at);
            let wait = std::time::Duration::from_millis((SHOW_MS - age).max(0) as u64 + 50);
            self._toast_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }));
        }
        Some(toast(notice.text, notice.error, p))
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let profile = self.model.read(cx).selected_profile().cloned();
        if let Some(profile) = &profile {
            self.sync_selection(profile, window, cx);
        }
        let toast = self.render_toast(p, cx);
        let sidebar = self.render_sidebar(p, cx);
        let panel = v_flex()
            .relative()
            .flex_1()
            .min_w_0()
            // Flush with the window edges, rounded only where it meets the sidebar.
            .rounded_tl(px(12.))
            .rounded_bl(px(12.))
            .bg(c(p.bg))
            .border_l_1()
            .border_color(c(p.side_line))
            .overflow_hidden()
            .text_color(c(p.fg))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right(px(140.))
                    .h(px(34.))
                    .window_control_area(WindowControlArea::Drag),
            )
            .when_some(profile.clone(), |this, profile| {
                this.child(self.render_hero(&profile, p, cx))
                    .child(self.render_tabs(p, cx))
                    .child(
                        div()
                            .id("tab-body")
                            .flex_1()
                            .min_h_0()
                            .px(px(28.))
                            .overflow_y_scroll()
                            .child(match self.tab {
                                Tab::General => {
                                    self.render_general(&profile, p, cx).into_any_element()
                                }
                                Tab::Code => self.render_code(&profile, p, cx).into_any_element(),
                                Tab::Data => self.render_data(&profile, p, cx).into_any_element(),
                            }),
                    )
            })
            .child(self.render_status_bar(p, cx))
            .child(caption_buttons(p, window))
            .children(toast);
        let dialog = self.dialog.as_ref().map(|d| self.render_dialog(d, p, cx));
        div()
            .flex()
            .flex_row()
            .id("main-root")
            .key_context("MainView")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .relative()
            .size_full()
            .bg(c(p.side))
            .font_family(theme::FONT_UI)
            .text_size(px(13.))
            .child(sidebar)
            .child(panel)
            .children(dialog)
            .when_some(self.settings.clone(), |this, settings| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(gpui_kit::hsla(
                            0.07,
                            0.3,
                            0.12,
                            if p.dark { 0.55 } else { 0.34 },
                        ))
                        .flex()
                        .items_center()
                        .justify_center()
                        .occlude()
                        .child(settings),
                )
            })
    }
}

// ---- helpers shared with tabs -------------------------------------------------------------

pub(super) fn icon_option(
    id: SharedString,
    i: ProfileIcon,
    on: bool,
    p: &Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(c(if on { p.accent } else { p.line }))
        .bg(c(if on { p.accent_soft } else { p.surface }))
        .cursor_pointer()
        .hover(|s| s.border_color(c(p.accent)))
        .child(icon(
            crate::assets::profile_icon(i),
            16.,
            c(if on { p.accent_ink } else { p.muted }),
        ))
}

pub(super) fn swatch(id: SharedString, col: ProfileColor, on: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(on, |this| this.border_2().border_color(c(p.fg)))
        .child(div().size(px(22.)).rounded_full().bg(c(col.rgb())))
}

fn folder_line(label: &'static str, path: String, p: &Palette) -> impl IntoElement {
    h_flex()
        .gap(px(8.))
        .text_size(px(11.5))
        .child(
            div()
                .w(px(92.))
                .flex_none()
                .text_color(c(p.muted))
                .child(label),
        )
        .child(mono(path, p).text_color(c(p.fg)).truncate())
}

/// Sidebar status line: (text, warn color, alert icon).
pub(super) fn status_line(
    profile: &Profile,
    status: Status,
    m: &AppModel,
) -> (SharedString, bool, bool) {
    if let Some(other) = m.duplicate_of(profile.id) {
        return (format!("Same account as {}", other.name).into(), true, true);
    }
    match status {
        Status::Running { since } => (
            format!("Running · {}", format_uptime(since)).into(),
            false,
            false,
        ),
        Status::Starting => ("Starting…".into(), false, false),
        Status::Restarting { attempt } => (
            format!("Restarting after a crash ({attempt} of 3)").into(),
            true,
            false,
        ),
        Status::Crashed => ("Crashed. Click to relaunch.".into(), true, true),
        Status::Stopped => ("Not running".into(), false, false),
    }
}

pub(super) fn short_account(acc: &str) -> String {
    let clean: String = acc.chars().filter(|c| *c != '-').collect();
    if clean.len() <= 8 {
        clean
    } else {
        format!("{}…{}", &clean[..4], &clean[clean.len() - 4..])
    }
}

pub(super) fn shorten_path(path: &std::path::Path, paths: &cm_core::paths::Paths) -> String {
    let s = path.display().to_string();
    let appdata = paths.appdata.display().to_string();
    let home = paths.home.display().to_string();
    if let Some(rest) = s.strip_prefix(&appdata) {
        format!("%APPDATA%{rest}")
    } else if let Some(rest) = s.strip_prefix(&home) {
        format!("%USERPROFILE%{rest}")
    } else {
        s
    }
}

/// GPUI keystroke → accelerator understood by `global-hotkey`, e.g. "Ctrl+Alt+2".
/// Requires Ctrl, Alt or Win so shortcuts don't swallow ordinary typing.
fn accelerator(ks: &Keystroke) -> Option<String> {
    let key = match ks.key.as_str() {
        "control" | "alt" | "shift" | "platform" | "function" | "" => return None,
        "`" => "`".to_string(),
        "space" => "Space".to_string(),
        k if k.len() == 1 => k.to_ascii_uppercase(),
        k if k.starts_with('f') && k[1..].parse::<u8>().is_ok() => k.to_ascii_uppercase(),
        _ => return None,
    };
    let mods = &ks.modifiers;
    if !(mods.control || mods.alt || mods.platform) {
        return None;
    }
    let mut parts = Vec::new();
    if mods.control {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.platform {
        parts.push("Super");
    }
    let mut s = parts.join("+");
    s.push('+');
    s.push_str(&key);
    Some(s)
}

pub(super) fn same_accel(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.to_ascii_lowercase()
            .replace("backquote", "`")
            .replace("control", "ctrl")
    };
    norm(a) == norm(b)
}
