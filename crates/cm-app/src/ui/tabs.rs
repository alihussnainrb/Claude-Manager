//! The General, Claude Code and Data tabs of the main window.

use cm_core::{Profile, ProfileColor, ProfileIcon};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Editor, Input};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::main_window::{MainView, McpTarget, icon_option, shorten_path, swatch};
use super::widgets::*;
use crate::model::{Status, format_bytes, format_uptime};
use crate::theme::{Palette, c};

impl MainView {
    pub(super) fn render_general(
        &self,
        profile: &Profile,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = profile.id;
        let m = self.model.read(cx);
        let status = m.status(id);
        let running_for = match status {
            Status::Running { since } => format_uptime(since),
            _ => "—".into(),
        };
        let crashes = m.crashes_this_week(id);
        let size = m
            .folder_sizes
            .get(&id)
            .map(|b| format_bytes(*b))
            .unwrap_or_else(|| "…".into());

        let icons = ProfileIcon::ALL.into_iter().map(|i| {
            icon_option(
                SharedString::from(format!("icon-{}", i.label())),
                i,
                profile.icon == i,
                p,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |m, cx| {
                    m.update_profile(id, |p| p.icon = i);
                    cx.notify();
                })
            }))
        });
        let colors = ProfileColor::ALL.into_iter().map(|col| {
            swatch(
                SharedString::from(format!("color-{}", col.label())),
                col,
                profile.color == col,
                p,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |m, cx| {
                    m.update_profile(id, |p| p.color = col);
                    cx.notify();
                })
            }))
        });
        let shortcut: AnyElement = if self.recording {
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(
                    div()
                        .px(px(10.))
                        .py(px(5.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(c(p.accent))
                        .bg(c(p.accent_soft))
                        .text_size(px(11.5))
                        .text_color(c(p.accent_ink))
                        .child("Press keys… (Esc cancels, Backspace clears)"),
                )
                .into_any_element()
        } else {
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(match &profile.hotkey {
                    Some(k) => keys(k, p).into_any_element(),
                    None => div()
                        .text_size(px(11.5))
                        .text_color(c(p.muted))
                        .child("None")
                        .into_any_element(),
                })
                .child(
                    button("hotkey-change", "Change", None, Btn::Secondary, true, p).on_click(
                        cx.listener(|this, _, window, cx| {
                            this.recording = true;
                            this.focus.focus(window, cx);
                            cx.notify();
                        }),
                    ),
                )
                .into_any_element()
        };
        let label = |t: &'static str| {
            div()
                .text_size(px(11.5))
                .text_color(c(p.muted))
                .mb(px(6.))
                .child(t)
        };
        let stat = |v: String, k: &'static str| {
            v_flex()
                .flex_1()
                .min_w_0()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .bg(crate::theme::mix(p.fg, p.surface, 0.04))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::BOLD)
                        .child(v),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(c(p.muted))
                        .whitespace_nowrap()
                        .child(k),
                )
        };
        h_flex()
            .gap(px(14.))
            .items_start()
            .pb(px(8.))
            .child(
                card(p)
                    .flex_1()
                    .child(card_title("Appearance", p))
                    .child(
                        v_flex()
                            .child(label("Name"))
                            .child(Input::new(&self.name_input)),
                    )
                    .child(
                        v_flex()
                            .child(label("Icon"))
                            .child(h_flex().gap(px(6.)).flex_wrap().children(icons)),
                    )
                    .child(
                        v_flex()
                            .child(label("Color"))
                            .child(h_flex().gap(px(6.)).children(colors)),
                    ),
            )
            .child(
                card(p)
                    .w(px(296.))
                    .flex_none()
                    .child(card_title("Behavior", p))
                    .child(set_row(
                        "Shortcut",
                        Some(
                            div()
                                .child(format!("Shows {} from any app", profile.name))
                                .into_any_element(),
                        ),
                        shortcut,
                        true,
                        p,
                    ))
                    .child(set_row(
                        "Open at startup",
                        Some(div().child("When Windows starts").into_any_element()),
                        toggle("autostart", profile.autostart, p).on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.model.update(cx, |m, cx| {
                                    m.update_profile(id, |p| p.autostart = !p.autostart);
                                    let any = m.profiles.iter().any(|p| p.autostart);
                                    if any && !m.settings.start_with_windows {
                                        m.update_settings(|s| s.start_with_windows = true);
                                        crate::app::apply_start_with_windows(m);
                                    }
                                    cx.notify();
                                })
                            },
                        )),
                        false,
                        p,
                    ))
                    .child(set_row(
                        "Restart after a crash",
                        Some(div().child("Up to 3 times in 5 minutes").into_any_element()),
                        toggle("auto-restart", profile.auto_restart, p).on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.model.update(cx, |m, cx| {
                                    m.update_profile(id, |p| p.auto_restart = !p.auto_restart);
                                    cx.notify();
                                })
                            },
                        )),
                        false,
                        p,
                    ))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(stat(running_for, "Running for"))
                            .child(stat(crashes.to_string(), "Recent crashes"))
                            .child(stat(size, "Data folder")),
                    ),
            )
    }

    pub(super) fn render_code(
        &self,
        profile: &Profile,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = profile.id;
        let m = self.model.read(cx);
        let config_dir = if profile.claude_code.enabled {
            profile.claude_code.config_dir.clone()
        } else {
            m.paths.default_code_config_dir()
        };
        let shown_dir = shorten_path(&config_dir, &m.paths);
        let seed_line = self.seed_results.get(&id).cloned();
        let is_default = profile.is_default();
        let file = self.mcp_file(profile, cx);
        let file_name = file
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        let issues = self.mcp_issues.clone();
        let target = self.mcp_target;
        let seg = |id: &'static str, label: &'static str, t: McpTarget| {
            let on = target == t;
            div()
                .id(id)
                .px(px(11.))
                .py(px(3.))
                .rounded_full()
                .text_size(px(12.))
                .cursor_pointer()
                .when(on, |this| {
                    this.bg(c(p.surface))
                        .font_weight(FontWeight::BOLD)
                        .shadow_xs()
                })
                .when(!on, |this| this.text_color(c(p.muted)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.mcp_target = t;
                    if let Some(profile) = this.model.read(cx).selected_profile().cloned() {
                        this.load_mcp(&profile, window, cx);
                    }
                    cx.notify();
                }))
                .child(label)
        };
        let footer_status: AnyElement = if issues.is_empty() {
            let count = cm_core::mcp::parse_servers(&self.mcp_editor.read(cx).value())
                .map(|s| s.len())
                .unwrap_or(0);
            h_flex()
                .gap(px(5.))
                .items_center()
                .text_color(c(p.ok))
                .font_weight(FontWeight::BOLD)
                .child(icon(IconName::CircleCheck, 14., c(p.ok)))
                .child(format!(
                    "Valid · {count} server{}",
                    if count == 1 { "" } else { "s" }
                ))
                .into_any_element()
        } else {
            let first = &issues[0];
            let text = if first.server.is_empty() {
                first.message.clone()
            } else {
                format!("{}: {}", first.server, first.message)
            };
            h_flex()
                .gap(px(5.))
                .items_center()
                .min_w_0()
                .text_color(c(p.bad))
                .font_weight(FontWeight::SEMIBOLD)
                .child(icon(IconName::TriangleAlert, 14., c(p.bad)))
                .child(div().truncate().child(if issues.len() > 1 {
                    format!("{text} (+{} more)", issues.len() - 1)
                } else {
                    text
                }))
                .into_any_element()
        };
        let can_save = self.mcp_dirty && issues.is_empty();
        let title = profile.name.clone();

        v_flex()
            .gap(px(14.))
            .pb(px(8.))
            .child(
                card(p)
                    .gap(px(11.))
                    .child(set_row(
                        "Separate Claude Code settings",
                        Some(
                            div()
                                .child(if is_default {
                                    "Default uses your main Claude Code folder.".to_string()
                                } else {
                                    format!("Used by this profile's Code tab and terminals. {shown_dir}")
                                })
                                .into_any_element(),
                        ),
                        if is_default {
                            div().into_any_element()
                        } else {
                            toggle("code-isolate", profile.claude_code.enabled, p)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.mcp_loaded_for = None;
                                    this.model.update(cx, |m, cx| {
                                        m.update_profile(id, |p| p.claude_code.enabled = !p.claude_code.enabled);
                                        m.notify_info("Restart this profile's Claude to apply.");
                                        cx.notify();
                                    })
                                }))
                                .into_any_element()
                        },
                        true,
                        p,
                    ))
                    .when(!is_default && profile.claude_code.enabled, |this| {
                        this.child(set_row(
                            "Copy setup from your main Claude Code",
                            Some(match seed_line {
                                Some(line) => h_flex()
                                    .gap(px(6.))
                                    .items_center()
                                    .child(icon(IconName::CircleCheck, 13., c(p.ok)))
                                    .child(line)
                                    .into_any_element(),
                                None => div()
                                    .child("Settings, skills, agents and MCP servers. Never your login or history.")
                                    .into_any_element(),
                            }),
                            button("seed", "Copy", Some(IconName::RefreshCw), Btn::Secondary, true, p).on_click(
                                cx.listener(move |this, _, _, cx| this.copy_setup(id, cx)),
                            ),
                            false,
                            p,
                        ))
                    })
                    .child(set_row(
                        "Terminal",
                        Some(div().child("Opens Windows Terminal with this profile's settings, then runs claude.").into_any_element()),
                        button("terminal", "Open terminal", Some(IconName::SquareTerminal), Btn::Primary, true, p).on_click(
                            cx.listener(move |this, _, _, cx| {
                                let home = this.model.read(cx).paths.home.clone();
                                if let Err(e) = cm_win::shell::open_claude_terminal(&format!("Claude Code · {title}"), &config_dir, &home) {
                                    this.model.update(cx, |m, cx| {
                                        m.notify_error(format!("Couldn't open a terminal: {e}"));
                                        cx.notify();
                                    });
                                }
                            }),
                        ),
                        false,
                        p,
                    )),
            )
            .child(
                v_flex()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(c(p.line))
                    .bg(c(p.surface))
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .items_center()
                            .px(px(12.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(c(p.line))
                            .child(card_title("MCP servers", p))
                            .child(
                                h_flex()
                                    .p(px(2.))
                                    .gap(px(2.))
                                    .rounded_full()
                                    .bg(crate::theme::mix(p.fg, p.surface, 0.06))
                                    .child(seg("mcp-code", "Claude Code", McpTarget::Code))
                                    .child(seg("mcp-desktop", "Claude Desktop", McpTarget::Desktop)),
                            )
                            .child(div().flex_1())
                            .child(
                                button("mcp-file", file_name, Some(IconName::Folder), Btn::Ghost, true, p).on_click(
                                    move |_, _, _| {
                                        if let Some(dir) = file.parent() {
                                            let _ = std::fs::create_dir_all(dir);
                                            let _ = cm_win::shell::open_path(dir);
                                        }
                                    },
                                ),
                            ),
                    )
                    .child(div().h(px(220.)).bg(c(p.code)).child(Editor::new(&self.mcp_editor).h_full().appearance(false)))
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .items_center()
                            .px(px(12.))
                            .py(px(8.))
                            .border_t_1()
                            .border_color(c(p.line))
                            .text_size(px(12.))
                            .child(footer_status)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(c(p.muted))
                                    .child("A backup is saved before each change. Restart Claude to apply."),
                            )
                            .child(button("mcp-discard", "Discard", None, Btn::Ghost, true, p).on_click(cx.listener(
                                |this, _, window, cx| {
                                    if let Some(profile) = this.model.read(cx).selected_profile().cloned() {
                                        this.load_mcp(&profile, window, cx);
                                    }
                                    cx.notify();
                                },
                            )))
                            .child(
                                button("mcp-save", "Save", None, if can_save { Btn::Primary } else { Btn::Secondary }, true, p)
                                    .when(!can_save, |this| this.opacity(0.55))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !can_save {
                                            return;
                                        }
                                        let text = this.mcp_editor.read(cx).value().to_string();
                                        let Some(profile) = this.model.read(cx).selected_profile().cloned() else { return };
                                        let file = this.mcp_file(&profile, cx);
                                        let result = cm_core::mcp::parse_servers(&text)
                                            .map_err(|_| "fix the errors first".to_string())
                                            .and_then(|s| cm_core::mcp::write_servers(&file, s).map_err(|e| e.to_string()));
                                        this.model.update(cx, |m, cx| {
                                            match result {
                                                Ok(()) => m.notify_info("Saved. Restart this profile's Claude to apply."),
                                                Err(e) => m.notify_error(format!("Couldn't save: {e}")),
                                            }
                                            cx.notify();
                                        });
                                        this.mcp_dirty = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }

    pub(super) fn render_data(
        &self,
        profile: &Profile,
        p: &Palette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = profile.id;
        let m = self.model.read(cx);
        let data_dir = m.data_dir(profile);
        let shown_data = shorten_path(&data_dir, &m.paths);
        let size = m
            .folder_sizes
            .get(&id)
            .map(|b| format!(" · {}", format_bytes(*b)))
            .unwrap_or_default();
        let code_dir = profile
            .claude_code
            .enabled
            .then(|| profile.claude_code.config_dir.clone());
        let cowork_ok = m.paths.is_cowork_compatible(&data_dir);
        let running = m.status(id).is_running();
        let confirm = self.confirm_remove;
        let open_btn = |key: &'static str, dir: std::path::PathBuf| {
            button(key, "Open", Some(IconName::Folder), Btn::Secondary, true, p).on_click(
                move |_, _, _| {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = cm_win::shell::open_path(&dir);
                },
            )
        };
        v_flex()
            .gap(px(14.))
            .pb(px(8.))
            .child(
                card(p)
                    .gap(px(11.))
                    .child(card_title("Folders", p))
                    .child(set_row(
                        "Claude data",
                        Some(div().child(format!("Sign-in, settings and connectors · {shown_data}{size}")).into_any_element()),
                        open_btn("open-data", data_dir.clone()),
                        true,
                        p,
                    ))
                    .when_some(code_dir.clone(), |this, dir| {
                        let shown = shorten_path(&dir, &m.paths);
                        this.child(set_row(
                            "Claude Code",
                            Some(div().child(shown).into_any_element()),
                            open_btn("open-code", dir),
                            false,
                            p,
                        ))
                    })
                    .when(!profile.is_default(), |this| {
                        this.child(set_row(
                            "Use a different folder",
                            Some(div().child("Point this profile at an existing folder to get an old session back.").into_any_element()),
                            button("relink", "Choose folder…", Some(IconName::Link), Btn::Secondary, true, p).on_click(
                                cx.listener(move |this, _, _, cx| this.choose_folder(id, cx)),
                            ),
                            false,
                            p,
                        ))
                    }),
            )
            .child(note(
                if cowork_ok {
                    "Only one profile at a time can run a Cowork workspace. This is a Windows limit, not a setting."
                } else {
                    "This folder isn't directly inside %APPDATA%, so Cowork won't work for this profile."
                },
                p,
            ))
            .when(!profile.is_default(), |this| {
                this.child(card(p).child(match confirm {
                    None => set_row(
                        "Remove profile",
                        Some(div().child("Removes it from Claude Manager. Its folders stay on disk, so you can add it back later.").into_any_element()),
                        button("remove", "Remove…", None, Btn::Danger, true, p).on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_remove = Some(false);
                            cx.notify();
                        })),
                        true,
                        p,
                    )
                    .into_any_element(),
                    Some(recycle) => v_flex()
                        .gap(px(12.))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Remove {} from Claude Manager?", profile.name)),
                        )
                        .when(running, |this| {
                            this.child(div().text_size(px(11.5)).text_color(c(p.warn)).child("Quit its Claude first."))
                        })
                        .child(
                            h_flex()
                                .id("recycle")
                                .gap(px(9.))
                                .items_center()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.confirm_remove = this.confirm_remove.map(|r| !r);
                                    cx.notify();
                                }))
                                .child(checkbox("recycle-box", recycle, p))
                                .child(div().text_size(px(12.)).child("Also move both folders to the Recycle Bin")),
                        )
                        .child(
                            h_flex()
                                .gap(px(8.))
                                .justify_end()
                                .child(button("remove-cancel", "Cancel", None, Btn::Secondary, true, p).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.confirm_remove = None;
                                        cx.notify();
                                    }),
                                ))
                                .child(button("remove-yes", "Remove", None, Btn::Danger, true, p).on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.confirm_remove = None;
                                        this.model.update(cx, |m, cx| {
                                            m.remove_profile(id, recycle);
                                            cx.notify();
                                        });
                                        cx.notify();
                                    },
                                ))),
                        )
                        .into_any_element(),
                }))
            })
    }

    fn choose_folder(&mut self, id: cm_core::ProfileId, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Use this folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.model.update(cx, |m, cx| {
                    let taken = m
                        .profiles
                        .iter()
                        .any(|p| p.id != id && cm_core::paths::same_path(&m.data_dir(p), &dir));
                    if taken {
                        m.notify_error("Another profile already uses that folder.");
                    } else {
                        let cowork = m.paths.is_cowork_compatible(&dir);
                        m.update_profile(id, |p| p.user_data_dir = Some(dir));
                        if cowork {
                            m.notify_info("Folder changed. Launch the profile to use it.");
                        } else {
                            m.notify_info(
                                "Folder changed. Cowork needs a folder directly inside %APPDATA%.",
                            );
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }
}
