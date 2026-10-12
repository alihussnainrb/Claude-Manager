//! Tray icon, global hotkeys and foreground tracking, hosted on a dedicated Win32 message
//! thread so they don't depend on GPUI's event loop.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cm_core::ProfileId;
use cm_win::msgthread::{MessageThread, WinEvent, WinEventHooks, install_win_event_hooks};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui_kit::Global;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::events::{AppEvent, EventTx, HotkeyTarget, TrayAction};

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    static HOTKEYS: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
    static REGISTERED: RefCell<Vec<HotKey>> = const { RefCell::new(Vec::new()) };
    static HOOKS: RefCell<Option<WinEventHooks>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone)]
pub struct TrayProfile {
    pub id: ProfileId,
    pub name: String,
    pub running: bool,
}

#[derive(Clone)]
pub struct Platform {
    thread: MessageThread,
    bindings: Arc<Mutex<HashMap<u32, HotkeyTarget>>>,
    tx: EventTx,
}

impl Global for Platform {}

impl Platform {
    pub fn start(tx: EventTx) -> Self {
        let hook_tx = tx.clone();
        let (thread, _join) = MessageThread::spawn(move || {
            let icon = Icon::from_rgba(crate::icons::logo_rgba(32, 0xE2725B), 32, 32).ok();
            let mut builder = TrayIconBuilder::new()
                .with_tooltip("Claude Manager")
                .with_menu(Box::new(Menu::new()))
                .with_menu_on_left_click(false);
            if let Some(icon) = icon {
                builder = builder.with_icon(icon);
            }
            match builder.build() {
                Ok(tray) => TRAY.with(|t| *t.borrow_mut() = Some(tray)),
                Err(e) => tracing::error!("tray icon failed: {e}"),
            }
            match GlobalHotKeyManager::new() {
                Ok(m) => HOTKEYS.with(|h| *h.borrow_mut() = Some(m)),
                Err(e) => tracing::error!("global hotkeys unavailable: {e}"),
            }
            let hooks = install_win_event_hooks(move |ev| {
                if let WinEvent::Foreground { pid, .. } = ev {
                    let _ = hook_tx.try_send(AppEvent::Foreground { pid });
                }
            });
            cm_win::taskbar::com_init();
            HOOKS.with(|h| *h.borrow_mut() = Some(hooks));
        });

        let click_tx = tx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                let _ = click_tx.try_send(AppEvent::Tray(TrayAction::OpenManager));
            }
        }));
        let menu_tx = tx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Some(action) = parse_menu_id(e.id.as_ref()) {
                let _ = menu_tx.try_send(AppEvent::Tray(action));
            }
        }));
        let bindings: Arc<Mutex<HashMap<u32, HotkeyTarget>>> = Arc::default();
        let hk_bindings = bindings.clone();
        let hk_tx = tx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state == HotKeyState::Pressed
                && let Some(target) = hk_bindings.lock().ok().and_then(|b| b.get(&e.id).copied())
            {
                let _ = hk_tx.try_send(AppEvent::Hotkey(target));
            }
        }));
        Self {
            thread,
            bindings,
            tx,
        }
    }

    /// Rebuild the tray menu.
    pub fn update_tray(
        &self,
        profiles: Vec<TrayProfile>,
        routing_on: bool,
        update: Option<String>,
    ) {
        self.thread.run(move || {
            let menu = Menu::new();
            let _ = menu.append(&MenuItem::with_id("header", "Claude Manager", false, None));
            if let Some(v) = &update {
                let _ = menu.append(&MenuItem::with_id(
                    "update",
                    format!("Restart to update to {v}"),
                    true,
                    None,
                ));
                let _ = menu.append(&PredefinedMenuItem::separator());
            }
            for p in &profiles {
                let label = format!(
                    "{}    {}",
                    p.name,
                    if p.running { "● Show" } else { "Launch" }
                );
                let _ = menu.append(&MenuItem::with_id(
                    format!("profile:{}", p.id.0),
                    label,
                    true,
                    None,
                ));
            }
            let _ = menu.append(&PredefinedMenuItem::separator());
            let _ = menu.append(&MenuItem::with_id(
                "open",
                "Open Claude Manager",
                true,
                None,
            ));
            let _ = menu.append(&MenuItem::with_id("switcher", "Quick switcher", true, None));
            let routing = if routing_on {
                "Pause sign-in routing"
            } else {
                "Resume sign-in routing"
            };
            let _ = menu.append(&MenuItem::with_id("routing", routing, true, None));
            let _ = menu.append(&MenuItem::with_id("settings", "Settings", true, None));
            let _ = menu.append(&PredefinedMenuItem::separator());
            let _ = menu.append(&MenuItem::with_id(
                "quit",
                "Quit Claude Manager (Claude stays open)",
                true,
                None,
            ));
            TRAY.with(|t| {
                if let Some(tray) = t.borrow().as_ref() {
                    tray.set_menu(Some(Box::new(menu)));
                }
            });
        });
    }

    /// Replace all global hotkeys. Accelerators that fail to register are reported back.
    pub fn set_hotkeys(&self, wanted: Vec<(String, HotkeyTarget)>) {
        let bindings = self.bindings.clone();
        let tx = self.tx.clone();
        self.thread.run(move || {
            HOTKEYS.with(|m| {
                let m = m.borrow();
                let Some(manager) = m.as_ref() else { return };
                REGISTERED.with(|r| {
                    let mut r = r.borrow_mut();
                    let _ = manager.unregister_all(&r);
                    r.clear();
                    let mut map = HashMap::new();
                    let mut failed = Vec::new();
                    for (accel, target) in wanted {
                        match accel.parse::<HotKey>() {
                            Ok(hk) => match manager.register(hk) {
                                Ok(()) => {
                                    map.insert(hk.id(), target);
                                    r.push(hk);
                                }
                                Err(e) => {
                                    tracing::warn!("hotkey {accel} unavailable: {e}");
                                    failed.push(accel);
                                }
                            },
                            Err(e) => tracing::warn!("bad hotkey {accel}: {e}"),
                        }
                    }
                    if let Ok(mut b) = bindings.lock() {
                        *b = map;
                    }
                    if !failed.is_empty() {
                        let _ = tx.try_send(AppEvent::HotkeyConflict(failed));
                    }
                });
            });
        });
    }

    /// Apply per-profile taskbar identities (COM work, so it runs on the message thread).
    pub fn tag_windows(&self, plan: crate::taskbar::Plan) {
        self.thread.run(move || {
            let crate::taskbar::Plan { tag, clear, titles } = plan;
            for r in &tag {
                let identity = cm_win::taskbar::Identity {
                    app_id: &r.app_id,
                    icon: &r.icon,
                };
                if let Err(e) = cm_win::taskbar::apply(r.hwnd, &identity) {
                    tracing::warn!("taskbar tag failed: {e}");
                }
            }
            for h in clear {
                let _ = cm_win::taskbar::clear(h);
            }
            for (h, title) in titles {
                cm_win::window::set_title(h, &title);
            }
        });
    }

    pub fn shutdown(&self) {
        self.thread.run(|| {
            TRAY.with(|t| t.borrow_mut().take());
            HOOKS.with(|h| h.borrow_mut().take());
        });
        self.thread.quit();
    }
}

fn parse_menu_id(id: &str) -> Option<TrayAction> {
    Some(match id {
        "open" => TrayAction::OpenManager,
        "switcher" => TrayAction::Switcher,
        "routing" => TrayAction::ToggleRouting,
        "settings" => TrayAction::Settings,
        "update" => TrayAction::InstallUpdate,
        "quit" => TrayAction::Quit,
        other => TrayAction::Profile(other.strip_prefix("profile:")?.parse().ok()?),
    })
}
