//! A8: Settings — General, Shortcuts, Sign-in routing, Backup, About.

use gpui_kit::assets::IconName;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::widgets::*;
use crate::model::{AppModel, model};
use crate::theme::{self, Palette, c, mix};

pub fn open(cx: &mut App) -> Option<AnyWindowHandle> {
    let options = super::chrome_options(980., 620., Some((720., 480.)), cx);
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| SettingsView::new(window, cx))
    }) {
        Ok((h, _)) => Some(h),
        Err(e) => {
            tracing::error!("settings failed to open: {e}");
            None
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Shortcuts,
    Routing,
    Backup,
    About,
}

struct SettingsView {
    model: Entity<AppModel>,
    section: Section,
    recording: bool,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = model(cx);
        let subs = vec![
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| theme::sync(window, cx)),
        ];
        Self {
            model,
            section: Section::Routing,
            recording: false,
            focus: cx.focus_handle(),
            _subs: subs,
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }
        cx.stop_propagation();
        let ks = &ev.keystroke;
        let accel = match ks.key.as_str() {
            "escape" => {
                self.recording = false;
                cx.notify();
                return;
            }
            "backspace" | "delete" => None,
            "control" | "alt" | "shift" | "platform" => return,
            k => {
                let key = if k == "`" {
                    "`".to_string()
                } else if k.len() == 1 {
                    k.to_ascii_uppercase()
                } else {
                    return;
                };
                let m = &ks.modifiers;
                if !(m.control || m.alt || m.platform) {
                    return;
                }
                let mut parts: Vec<&str> = Vec::new();
                if m.control {
                    parts.push("Ctrl");
                }
                if m.alt {
                    parts.push("Alt");
                }
                if m.shift {
                    parts.push("Shift");
                }
                if m.platform {
                    parts.push("Super");
                }
                Some(format!("{}+{key}", parts.join("+")))
            }
        };
        self.recording = false;
        self.model.update(cx, |m, cx| {
            m.update_settings(|s| s.switcher_hotkey = accel);
            cx.notify();
        });
    }

    fn nav(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |key: &'static str, label: &'static str, name: IconName, s: Section| {
            let on = self.section == s;
            h_flex()
                .id(key)
                .gap(px(10.))
                .items_center()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(9.))
                .text_size(px(12.5))
                .cursor_pointer()
                .when(on, |this| {
                    this.bg(c(p.side_sel))
                        .text_color(c(p.side_fg))
                        .font_weight(FontWeight::BOLD)
                        .border_1()
                        .border_color(c(p.side_line))
                })
                .when(!on, |this| {
                    this.text_color(c(p.side_muted))
                        .hover(|st| st.bg(mix(p.side_fg, p.side, 0.05)))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.section = s;
                    this.recording = false;
                    cx.notify();
                }))
                .child(icon(
                    name,
                    16.,
                    c(if on { p.side_fg } else { p.side_muted }),
                ))
                .child(label)
        };
        v_flex()
            .w(px(216.))
            .flex_none()
            .px(px(10.))
            .pt(px(14.))
            .gap(px(2.))
            .child(
                h_flex()
                    .gap(px(9.))
                    .items_center()
                    .px(px(8.))
                    .pb(px(14.))
                    .h(px(36.))
                    .window_control_area(WindowControlArea::Drag)
                    .child(logo(20., c(p.accent)))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(13.5))
                            .text_color(c(p.side_fg))
                            .child("Settings"),
                    ),
            )
            .child(item(
                "s-general",
                "General",
                IconName::AppWindow,
                Section::General,
            ))
            .child(item(
                "s-shortcuts",
                "Shortcuts",
                IconName::Keyboard,
                Section::Shortcuts,
            ))
            .child(item(
                "s-routing",
                "Sign-in routing",
                IconName::Link,
                Section::Routing,
            ))
            .child(item(
                "s-backup",
                "Backup",
                IconName::Package,
                Section::Backup,
            ))
            .child(item("s-about", "About", IconName::Info, Section::About))
    }

    fn general(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let s = &self.model.read(cx).settings;
        page("General", "How Claude Manager starts and closes.", p)
            .child(
                card(p)
                    .gap(px(11.))
                    .child(set_row(
                        "Open when Windows starts",
                        Some(div().child("Starts in the tray, then opens profiles marked \"Open at startup\".").into_any_element()),
                        toggle("start-with-windows", s.start_with_windows, p).on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.update_settings(|s| s.start_with_windows = !s.start_with_windows);
                                crate::app::apply_start_with_windows(m);
                                cx.notify();
                            })
                        })),
                        true,
                        p,
                    ))
                    .child(set_row(
                        "Closing the window hides it to the tray",
                        Some(div().child("Profiles keep running either way. Quit from the tray menu.").into_any_element()),
                        toggle("close-to-tray", s.close_to_tray, p).on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.update_settings(|s| s.close_to_tray = !s.close_to_tray);
                                cx.notify();
                            })
                        })),
                        false,
                        p,
                    ))
                    .child(set_row(
                        "Give each profile its own taskbar button",
                        Some(div().child("Shows the profile's colored icon instead of Claude's, and pins relaunch that profile.").into_any_element()),
                        toggle("taskbar-icons", s.taskbar_icons, p).on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.update_settings(|s| s.taskbar_icons = !s.taskbar_icons);
                                cx.notify();
                            });
                            crate::app::sync_taskbar(cx);
                        })),
                        false,
                        p,
                    ))
                    .child(set_row(
                        "Tell me about new versions",
                        Some(div().child("Shows a notice when a newer release is on GitHub. Nothing installs automatically.").into_any_element()),
                        toggle("check-updates", s.check_for_updates, p).on_click(cx.listener(|this, _, _, cx| {
                            this.model.update(cx, |m, cx| {
                                m.update_settings(|s| s.check_for_updates = !s.check_for_updates);
                                cx.notify();
                            })
                        })),
                        false,
                        p,
                    )),
            )
    }

    fn shortcuts(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let m = self.model.read(cx);
        let switcher: AnyElement = if self.recording {
            div()
                .px(px(10.))
                .py(px(5.))
                .rounded(px(7.))
                .border_1()
                .border_color(c(p.accent))
                .bg(c(p.accent_soft))
                .text_size(px(11.5))
                .text_color(c(p.accent_ink))
                .child("Press keys… (Esc cancels, Backspace clears)")
                .into_any_element()
        } else {
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(match &m.settings.switcher_hotkey {
                    Some(k) => keys(&k.replace("Backquote", "`"), p).into_any_element(),
                    None => div()
                        .text_color(c(p.muted))
                        .text_size(px(11.5))
                        .child("None")
                        .into_any_element(),
                })
                .child(
                    button("switcher-change", "Change", None, Btn::Secondary, true, p).on_click(
                        cx.listener(|this, _, window, cx| {
                            this.recording = true;
                            this.focus.focus(window, cx);
                            cx.notify();
                        }),
                    ),
                )
                .into_any_element()
        };
        let per_profile: Vec<AnyElement> = m
            .profiles
            .iter()
            .map(|pr| {
                set_row(
                    pr.name.clone(),
                    None,
                    match &pr.hotkey {
                        Some(k) => keys(k, p).into_any_element(),
                        None => div()
                            .text_color(c(p.muted))
                            .text_size(px(11.5))
                            .child("None")
                            .into_any_element(),
                    },
                    false,
                    p,
                )
                .into_any_element()
            })
            .collect();
        page(
            "Shortcuts",
            "Global shortcuts work from any app. Change a profile's shortcut on its General tab.",
            p,
        )
        .child(
            card(p)
                .gap(px(11.))
                .child(set_row(
                    "Quick switcher",
                    Some(div().child("Find and show any profile").into_any_element()),
                    switcher,
                    true,
                    p,
                ))
                .children(per_profile),
        )
    }

    fn routing(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        let m = self.model.read(cx);
        let on = m.settings.deep_link_broker;
        let ours = crate::broker::handler_is_ours();
        let reclaims = m.protocol_reclaims;
        let status = if on && ours {
            let extra = if reclaims > 0 {
                format!(
                    " Claude tried to take them back {reclaims} time{} this session; they were reclaimed.",
                    if reclaims == 1 { "" } else { "s" }
                )
            } else {
                String::new()
            };
            Some((
                true,
                format!("Claude Manager is handling sign-in links.{extra}"),
            ))
        } else if on {
            Some((
                false,
                "Sign-in links aren't reaching Claude Manager yet. Reopen the app to retry."
                    .to_string(),
            ))
        } else {
            None
        };
        page(
            "Sign-in routing",
            "Claude Manager receives claude:// sign-in links from your browser and passes each one to the profile that asked for it. Without this, Windows may sign the wrong profile in.",
            p,
        )
        .child(
            card(p)
                .gap(px(11.))
                .child(set_row(
                    "Route sign-ins to the right profile",
                    None,
                    toggle("routing", on, p).on_click(cx.listener(|this, _, _, cx| {
                        this.model.update(cx, |m, cx| {
                            m.update_settings(|s| s.deep_link_broker = !s.deep_link_broker);
                            if m.settings.deep_link_broker {
                                crate::broker::ensure(m);
                            } else {
                                crate::broker::restore(m);
                            }
                            cx.notify();
                        })
                    })),
                    true,
                    p,
                ))
                .when_some(status, |this, (good, text)| {
                    let color = if good { p.ok } else { p.warn };
                    this.child(
                        h_flex()
                            .gap(px(9.))
                            .items_center()
                            .px(px(12.))
                            .py(px(10.))
                            .rounded(px(9.))
                            .bg(crate::theme::ca(color, 0.11))
                            .text_size(px(12.))
                            .child(icon(if good { IconName::CircleCheck } else { IconName::TriangleAlert }, 15., c(color)))
                            .child(text),
                    )
                })
                .child(set_row(
                    "Ask me when it's unclear",
                    Some(div().child("When off, unclear sign-ins go to the window you used last.").into_any_element()),
                    toggle("ask", m.settings.ask_when_unclear, p).on_click(cx.listener(|this, _, _, cx| {
                        this.model.update(cx, |m, cx| {
                            m.update_settings(|s| s.ask_when_unclear = !s.ask_when_unclear);
                            cx.notify();
                        })
                    })),
                    false,
                    p,
                ))
                .child(set_row(
                    "Restore Claude's own handler",
                    Some(div().child("Turns routing off and puts Windows back the way it was.").into_any_element()),
                    button("restore", "Restore", None, Btn::Secondary, true, p).on_click(cx.listener(|this, _, _, cx| {
                        this.model.update(cx, |m, cx| {
                            m.update_settings(|s| s.deep_link_broker = false);
                            crate::broker::restore(m);
                            cx.notify();
                        })
                    })),
                    false,
                    p,
                )),
        )
        .child(note(
            "Still landing in the wrong place? On the Claude sign-in page choose \"Continue with email\" and type the 6-digit code instead of clicking the link.",
            p,
        ))
    }

    fn backup(&self, p: &Palette, cx: &mut Context<Self>) -> Div {
        page("Backup", "Save your profiles and settings to one file, or bring them back. Logins are never included.", p).child(
            card(p)
                .gap(px(11.))
                .child(set_row(
                    "Export profiles",
                    Some(div().child("Names, icons, colors, shortcuts and folder locations.").into_any_element()),
                    button("export", "Export…", None, Btn::Secondary, true, p).on_click(cx.listener(|this, _, _, cx| this.export(cx))),
                    true,
                    p,
                ))
                .child(set_row(
                    "Import profiles",
                    Some(div().child("Adds profiles from an export. Profiles with the same ID are updated.").into_any_element()),
                    button("import", "Import…", None, Btn::Secondary, true, p).on_click(cx.listener(|this, _, _, cx| this.import(cx))),
                    false,
                    p,
                )),
        )
    }

    fn about(&self, p: &Palette) -> Div {
        page("About", "Run several Claude Desktop accounts side by side.", p).child(
            card(p)
                .gap(px(8.))
                .text_size(px(12.5))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("Claude Manager {}", env!("CARGO_PKG_VERSION"))))
                .child(div().text_color(c(p.muted)).child("Free and open source under the MIT license. Font: Figtree (SIL Open Font License)."))
                .child(div().text_color(c(p.muted)).child("Claude Manager is an independent project, not affiliated with Anthropic. Each profile runs the official Claude Desktop app.")),
        )
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let m = self.model.read(cx);
        let bundle = cm_core::export::export(&m.profiles, &m.settings);
        let rx = cx.prompt_for_new_path(&m.paths.home, Some("claude-manager-profiles.json"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            let result = cm_core::store::write_json_atomic(&path, &bundle);
            let _ = this.update(cx, |this, cx| {
                this.model.update(cx, |m, cx| {
                    match result {
                        Ok(()) => m.notify_info(format!("Exported to {}", path.display())),
                        Err(e) => m.notify_error(format!("Export failed: {e}")),
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn import(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string());
            let _ = this.update(cx, |this, cx| {
                this.model.update(cx, |m, cx| {
                    match text.and_then(|t| cm_core::export::parse(&t).map_err(|e| e.to_string())) {
                        Ok(bundle) => {
                            let n = bundle.profiles.len();
                            m.profiles =
                                cm_core::export::merge(&m.paths, &m.profiles, bundle.profiles);
                            m.save_profiles();
                            m.notify_info(format!(
                                "Imported {n} profile{}",
                                if n == 1 { "" } else { "s" }
                            ));
                        }
                        Err(e) => m.notify_error(format!("Import failed: {e}")),
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }
}

fn page(title: &'static str, lede: &'static str, p: &Palette) -> Div {
    v_flex().gap(px(16.)).child(heading(title, 22., p)).child(
        div()
            .mt(px(-8.))
            .max_w(px(560.))
            .text_size(px(12.5))
            .text_color(c(p.muted))
            .child(lede),
    )
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::palette(cx);
        let body = match self.section {
            Section::General => self.general(p, cx),
            Section::Shortcuts => self.shortcuts(p, cx),
            Section::Routing => self.routing(p, cx),
            Section::Backup => self.backup(p, cx),
            Section::About => self.about(p),
        };
        div()
            .flex()
            .flex_row()
            .id("settings-root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .bg(c(p.side))
            .font_family(theme::FONT_UI)
            .text_size(px(13.))
            .text_color(c(p.fg))
            .child(self.nav(p, cx))
            .child(
                v_flex()
                    .id("settings-panel")
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .m(px(6.))
                    .ml_0()
                    .rounded(px(10.))
                    .bg(c(p.bg))
                    .border_1()
                    .border_color(c(p.side_line))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right(px(140.))
                            .h(px(34.))
                            .window_control_area(WindowControlArea::Drag),
                    )
                    .child(
                        div()
                            .id("settings-scroll")
                            .flex_1()
                            .overflow_y_scroll()
                            .px(px(30.))
                            .pt(px(44.))
                            .pb(px(20.))
                            .child(body),
                    )
                    .child(caption_buttons(p, window)),
            )
    }
}
