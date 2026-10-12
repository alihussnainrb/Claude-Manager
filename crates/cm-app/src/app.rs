//! Event dispatch on the UI thread, and keeping the tray/hotkeys in step with the model.

use std::time::Duration;

use cm_core::ProfileId;
use gpui_kit::*;

use crate::events::{AppEvent, Command, HotkeyTarget, TrayAction};
use crate::model::{AppModel, model};
use crate::platform::{Platform, TrayProfile};
use crate::{broker, ui};

/// Startup state that matters until the first scan arrives.
#[derive(Default)]
pub struct Startup {
    pub background: bool,
    pub autostart_done: bool,
    /// Deep links that arrived before we knew what's running.
    pub queued_links: Vec<String>,
    /// Last tray/hotkey state pushed to the platform thread.
    platform_sig: String,
}
impl Global for Startup {}

pub fn handle(ev: AppEvent, cx: &mut App) {
    let model = model(cx);
    match ev {
        AppEvent::Scan(scan) => {
            let first = !model.read(cx).scanned_once;
            model.update(cx, |m, cx| {
                m.apply_scan(scan);
                cx.notify();
            });
            if first {
                on_first_scan(cx);
            }
        }
        AppEvent::Exited { id, pid, code } => {
            let relaunch = model.update(cx, |m, cx| {
                cx.notify();
                m.on_exit(id, pid, code)
            });
            if let Some(id) = relaunch {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(1500))
                        .await;
                    cx.update(|cx| {
                        crate::model::model(cx).update(cx, |m, cx| {
                            m.relaunch_after_crash(id);
                            cx.notify();
                        })
                    });
                })
                .detach();
            }
        }
        AppEvent::Command(cmd) => handle_command(cmd, cx),
        AppEvent::Tray(action) => match action {
            TrayAction::OpenManager => ui::open_main(cx),
            TrayAction::Profile(id) => show(id, cx),
            TrayAction::Switcher => ui::open_switcher(cx),
            TrayAction::Settings => ui::open_settings(cx),
            TrayAction::ToggleRouting => model.update(cx, |m, cx| {
                m.update_settings(|s| s.deep_link_broker = !s.deep_link_broker);
                if m.settings.deep_link_broker {
                    broker::ensure(m);
                } else {
                    broker::restore(m);
                }
                cx.notify();
            }),
            TrayAction::Quit => {
                if cx.has_global::<Platform>() {
                    cx.global::<Platform>().shutdown();
                }
                cx.quit();
            }
        },
        AppEvent::Hotkey(HotkeyTarget::Switcher) => ui::open_switcher(cx),
        AppEvent::Hotkey(HotkeyTarget::Profile(id)) => show(id, cx),
        AppEvent::Foreground { pid } => model.update(cx, |m, _| {
            if let Some(id) = m.profile_for_pid(pid) {
                m.note_focus(id);
            }
        }),
        AppEvent::ProtocolChanged => model.update(cx, |m, cx| {
            if m.settings.deep_link_broker && broker::ensure(m) {
                m.protocol_reclaims += 1;
                cx.notify();
            }
        }),
        AppEvent::FolderSize { id, bytes } => model.update(cx, |m, cx| {
            m.folder_sizes.insert(id, bytes);
            cx.notify();
        }),
        AppEvent::HotkeyConflict(keys) => model.update(cx, |m, cx| {
            m.notify_error(format!(
                "{} is already used by another app. Pick a different shortcut.",
                keys.join(", ")
            ));
            cx.notify();
        }),
    }
    sync_platform(cx);
}

fn show(id: ProfileId, cx: &mut App) {
    model(cx).update(cx, |m, cx| {
        m.show(id);
        cx.notify();
    });
}

fn handle_command(cmd: Command, cx: &mut App) {
    match cmd {
        Command::Show => ui::open_main(cx),
        Command::Launch { id } => match id.parse::<ProfileId>() {
            Ok(id) => show(id, cx),
            Err(_) => tracing::warn!("launch request for unknown profile {id}"),
        },
        Command::DeepLink { url } => {
            if !model(cx).read(cx).scanned_once {
                startup(cx).queued_links.push(url);
            } else {
                route_link(url, cx);
            }
        }
    }
}

fn route_link(url: String, cx: &mut App) {
    if !url.to_ascii_lowercase().starts_with("claude://") {
        tracing::warn!("ignoring non-claude link");
        return;
    }
    let ask = model(cx).update(cx, |m, cx| {
        cx.notify();
        broker::route(m, &url)
    });
    if let Some(candidates) = ask {
        ui::open_router(url, candidates, cx);
    }
}

fn on_first_scan(cx: &mut App) {
    let links = std::mem::take(&mut startup(cx).queued_links);
    for url in links {
        route_link(url, cx);
    }
    let st = startup(cx);
    if st.background && !st.autostart_done {
        st.autostart_done = true;
        model(cx).update(cx, |m, cx| {
            let ids: Vec<_> = m
                .profiles
                .iter()
                .filter(|p| p.autostart)
                .map(|p| p.id)
                .collect();
            for id in ids {
                if !m.status(id).is_running() {
                    m.launch(id);
                }
            }
            cx.notify();
        });
    }
}

pub fn startup(cx: &mut App) -> &mut Startup {
    if !cx.has_global::<Startup>() {
        cx.set_global(Startup::default());
    }
    cx.global_mut::<Startup>()
}

/// Push tray menu and hotkey changes to the platform thread, only when they changed.
pub fn sync_platform(cx: &mut App) {
    if !cx.has_global::<Platform>() {
        return;
    }
    let m = model(cx).read(cx);
    let tray: Vec<TrayProfile> = m
        .profiles
        .iter()
        .map(|p| TrayProfile {
            id: p.id,
            name: p.name.clone(),
            running: m.status(p.id).is_running(),
        })
        .collect();
    let mut hotkeys: Vec<(String, HotkeyTarget)> = m
        .profiles
        .iter()
        .filter_map(|p| Some((p.hotkey.clone()?, HotkeyTarget::Profile(p.id))))
        .collect();
    if let Some(k) = &m.settings.switcher_hotkey {
        hotkeys.push((k.clone(), HotkeyTarget::Switcher));
    }
    let routing = m.settings.deep_link_broker;
    let sig = format!(
        "{:?}|{:?}|{routing}",
        tray.iter()
            .map(|t| (t.id, &t.name, t.running))
            .collect::<Vec<_>>(),
        hotkeys.iter().map(|(k, t)| (k, *t)).collect::<Vec<_>>()
    );
    if startup(cx).platform_sig == sig {
        return;
    }
    startup(cx).platform_sig = sig;
    let platform = cx.global::<Platform>().clone();
    platform.update_tray(tray, routing);
    platform.set_hotkeys(hotkeys);
}

/// Add or remove the HKCU Run entry to match the setting.
pub fn apply_start_with_windows(m: &mut AppModel) {
    let command = std::env::current_exe()
        .ok()
        .map(|exe| format!("\"{}\" --background", exe.display()));
    let result = cm_win::registry::set_run_entry(if m.settings.start_with_windows {
        command.as_deref()
    } else {
        None
    });
    if let Err(e) = result {
        m.notify_error(format!("Couldn't change the startup setting: {e}"));
    }
}
