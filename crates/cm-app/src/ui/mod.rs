//! Windows: the manager, the quick switcher, the sign-in router and settings.

mod main_window;
mod router;
mod settings_window;
mod switcher;
mod tabs;
pub mod widgets;

use cm_core::ProfileId;
use gpui_kit::*;

pub use main_window::MainView;

/// Handles of the windows that are currently open.
#[derive(Default)]
pub struct OpenWindows {
    main: Option<AnyWindowHandle>,
    switcher: Option<AnyWindowHandle>,
    router: Option<AnyWindowHandle>,
    settings: Option<AnyWindowHandle>,
    main_view: Option<WeakEntity<MainView>>,
}
impl Global for OpenWindows {}

fn windows(cx: &mut App) -> &mut OpenWindows {
    if !cx.has_global::<OpenWindows>() {
        cx.set_global(OpenWindows::default());
    }
    cx.global_mut::<OpenWindows>()
}

/// Activate a window if it's still open. Returns false if it was closed.
fn activate(handle: Option<AnyWindowHandle>, cx: &mut App) -> bool {
    handle.is_some_and(|h| {
        h.update(cx, |_, window, _| window.activate_window())
            .is_ok()
    })
}

pub(crate) fn chrome_options(
    width: f32,
    height: f32,
    min: Option<(f32, f32)>,
    cx: &App,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(height)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Claude Manager".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_min_size: min.map(|(w, h)| size(px(w), px(h))),
        app_id: Some("ClaudeManager".into()),
        ..Default::default()
    }
}

pub fn open_main(cx: &mut App) {
    let existing = windows(cx).main;
    if activate(existing, cx) {
        return;
    }
    let options = chrome_options(980., 680., Some((760., 520.)), cx);
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| MainView::new(window, cx))
    }) {
        Ok((handle, view)) => {
            let w = windows(cx);
            w.main = Some(handle);
            w.main_view = Some(view.downgrade());
        }
        Err(e) => tracing::error!("main window failed to open: {e}"),
    }
}

/// Open the main window with a profile selected.
pub fn open_main_at(id: ProfileId, cx: &mut App) {
    crate::model::model(cx).update(cx, |m, cx| {
        m.selected = id;
        cx.notify();
    });
    open_main(cx);
}

pub fn open_switcher(cx: &mut App) {
    let existing = windows(cx).switcher.take();
    if let Some(h) = existing {
        // Toggle: a second press closes it.
        if h.update(cx, |_, window, _| window.remove_window()).is_ok() {
            return;
        }
    }
    windows(cx).switcher = switcher::open(cx);
}

pub fn open_router(url: String, candidates: Vec<ProfileId>, cx: &mut App) {
    if let Some(h) = windows(cx).router.take() {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
    windows(cx).router = router::open(url, candidates, cx);
}

pub fn open_settings(cx: &mut App) {
    let existing = windows(cx).settings;
    if activate(existing, cx) {
        return;
    }
    windows(cx).settings = settings_window::open(cx);
}

/// Debug builds only: open the app in a given state so each screen can be checked visually.
#[cfg(debug_assertions)]
pub fn dev_view(name: &str, cx: &mut App) {
    use main_window::Tab;
    let (Some(handle), Some(view)) = (
        windows(cx).main,
        windows(cx).main_view.clone().and_then(|v| v.upgrade()),
    ) else {
        return;
    };
    let name = name.to_string();
    match name.as_str() {
        "settings" => open_settings(cx),
        "switcher" => open_switcher(cx),
        "router" => {
            let ids = crate::model::model(cx)
                .read(cx)
                .profiles
                .iter()
                .map(|p| p.id)
                .collect();
            open_router("claude://login/callback?code=dev".into(), ids, cx);
        }
        _ => {
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |v, cx| {
                    match name.as_str() {
                        "code" => v.tab = Tab::Code,
                        "data" => v.tab = Tab::Data,
                        "dialog" => v.open_dialog(window, cx),
                        _ => {}
                    }
                    cx.notify();
                })
            });
        }
    }
}

/// Whether the manager window is currently open (not hidden to the tray).
pub fn is_main_open(cx: &mut App) -> bool {
    let h = windows(cx).main;
    h.is_some_and(|h| h.update(cx, |_, _, _| ()).is_ok())
}
