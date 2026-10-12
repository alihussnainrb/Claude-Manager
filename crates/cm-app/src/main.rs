#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod broker;
mod events;
mod icons;
mod model;
mod platform;
mod scan;
mod taskbar;
mod theme;
mod ui;
mod updater;

use cm_win::ipc::{self, Instance};
use gpui_kit::*;

use crate::events::{AppEvent, Command};
use crate::model::{AppModel, ModelHandle};
use crate::platform::Platform;

struct Args {
    background: bool,
    after_update: Option<u32>,
    uninstall: bool,
    command: Option<Command>,
    dev_view: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args {
        background: false,
        after_update: None,
        uninstall: false,
        command: None,
        dev_view: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--background" => args.background = true,
            "--after-update" => args.after_update = it.next().and_then(|p| p.parse().ok()),
            "--uninstall" => args.uninstall = true,
            "--dev-view" => args.dev_view = it.next(),
            "--launch" => args.command = it.next().map(|id| Command::Launch { id }),
            "--deeplink" => args.command = it.next().map(|url| Command::DeepLink { url }),
            // Windows may hand us the bare URL when the handler omits the flag.
            url if url.to_ascii_lowercase().starts_with("claude://") => {
                args.command = Some(Command::DeepLink {
                    url: url.to_string(),
                })
            }
            _ => {}
        }
    }
    args
}

fn init_logging(
    paths: &cm_core::paths::Paths,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,gpui=warn"));
    let dir = paths.logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let appender = tracing_appender::rolling::daily(dir, "claude-manager.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Some(guard)
}

fn main() {
    let args = parse_args();
    let Some(paths) = cm_win::known_paths() else {
        eprintln!("APPDATA/USERPROFILE are not set");
        return;
    };
    let _log_guard = init_logging(&paths);

    // Run by the uninstaller: undo what we changed in Windows, then exit.
    if args.uninstall {
        uninstall(&paths);
        return;
    }

    // Started by our own updater: let the old version exit first so we can take the lock.
    if let Some(pid) = args.after_update {
        updater::wait_for_exit(pid);
    }
    std::thread::spawn(updater::cleanup);

    // A second copy forwards its request to the running one and exits.
    let guard = match ipc::acquire() {
        Ok(Instance::Primary(g)) => g,
        Ok(Instance::Secondary) => {
            let cmd = args.command.clone().unwrap_or(Command::Show);
            if let Err(e) = ipc::send(&serde_json::to_string(&cmd).expect("serializable")) {
                tracing::error!("couldn't reach the running Claude Manager: {e}");
            }
            return;
        }
        Err(e) => {
            tracing::error!("single-instance check failed: {e}");
            return;
        }
    };

    let (tx, rx) = async_channel::unbounded::<AppEvent>();
    let ipc_tx = tx.clone();
    if let Err(e) = ipc::serve(move |msg| match serde_json::from_str::<Command>(&msg) {
        Ok(cmd) => {
            let _ = ipc_tx.send_blocking(AppEvent::Command(cmd));
        }
        Err(e) => tracing::warn!("bad pipe message: {e}"),
    }) {
        tracing::error!("command pipe unavailable: {e}");
    }
    scan::spawn(tx.clone());
    broker::spawn_watcher(tx.clone());
    if let Some(cmd) = args.command.clone() {
        let _ = tx.send_blocking(AppEvent::Command(cmd));
    }

    // Hold the single-instance mutex for the life of the process. (GPUI drops the launch
    // closure once it has run, so it can't live in there.)
    std::mem::forget(guard);

    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            theme::init(cx);
            cx.set_quit_mode(QuitMode::Explicit);

            let model = cx.new(|_| AppModel::load(paths, tx.clone()));
            cx.set_global(ModelHandle(model.clone()));
            app::startup(cx).background = args.background;
            cx.set_global(Platform::start(tx.clone()));
            model.update(cx, |m, _| {
                broker::ensure(m);
                if args.after_update.is_some() {
                    m.notify_info(format!(
                        "Updated to Claude Manager {}",
                        env!("CARGO_PKG_VERSION")
                    ));
                }
            });

            if !args.background {
                ui::open_main(cx);
            }
            #[cfg(debug_assertions)]
            if let Some(view) = &args.dev_view {
                ui::dev_view(view, cx);
            }

            cx.spawn(async move |cx| {
                while let Ok(ev) = rx.recv().await {
                    cx.update(|cx| app::handle(ev, cx));
                }
            })
            .detach();
            app::sync_platform(cx);
            app::start_update_loop(cx);
        });
}

/// Give `claude://` back to Claude Desktop and remove the startup entry.
fn uninstall(paths: &cm_core::paths::Paths) {
    let settings = cm_core::store::load_settings(paths).unwrap_or_default();
    let fallback = cm_win::appx::find_claude().map(|i| format!("\"{}\" \"%1\"", i.exe.display()));
    let ours = cm_win::registry::protocol_handler()
        .ok()
        .flatten()
        .is_some_and(|h| h.contains("--deeplink"));
    if ours
        && let Some(cmd) = settings.saved_protocol_handler.or(fallback)
        && let Err(e) = cm_win::registry::set_protocol_handler(&cmd)
    {
        tracing::error!("couldn't restore the claude:// handler: {e}");
    }
    if let Err(e) = cm_win::registry::set_run_entry(None) {
        tracing::error!("couldn't remove the startup entry: {e}");
    }
    tracing::info!("uninstall cleanup done");
}
