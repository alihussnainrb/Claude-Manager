//! Background scanner: which Claude Desktop instances are running, from which data folder.

use std::time::{Duration, Instant};

use cm_core::cmdline::parse_claude_cmdline;
use cm_win::appx::{ClaudeInstall, find_claude};

use crate::events::{AppEvent, EventTx, Instance, ScanResult};

const INTERVAL: Duration = Duration::from_millis(1500);
const INSTALL_REFRESH: Duration = Duration::from_secs(60);

pub fn spawn(tx: EventTx) {
    std::thread::Builder::new()
        .name("cm-scan".into())
        .spawn(move || {
            let mut install: Option<ClaudeInstall> = None;
            let mut install_checked = Instant::now() - INSTALL_REFRESH;
            loop {
                // Re-resolve periodically and whenever the exe vanished (Store update).
                let stale = install.as_ref().is_none_or(|i| !i.exe.exists());
                if stale || install_checked.elapsed() >= INSTALL_REFRESH {
                    install = find_claude();
                    install_checked = Instant::now();
                }
                let result = ScanResult {
                    instances: scan(install.as_ref()),
                    install: install.clone(),
                };
                if tx.send_blocking(AppEvent::Scan(result)).is_err() {
                    return;
                }
                std::thread::sleep(INTERVAL);
            }
        })
        .expect("spawn scanner");
}

pub fn scan(install: Option<&ClaudeInstall>) -> Vec<Instance> {
    cm_win::process::list_by_name("Claude.exe")
        .into_iter()
        .filter(|p| match (&p.image_path, install) {
            (Some(img), Some(i)) => i.owns_image(img),
            (Some(img), None) => {
                let s = img.to_string_lossy().to_ascii_lowercase();
                s.contains(r"\windowsapps\claude_") || s.contains(r"\anthropicclaude\")
            }
            _ => false,
        })
        .filter_map(|p| {
            let info = parse_claude_cmdline(p.cmdline.as_deref()?);
            info.is_main.then(|| {
                let dir = info.user_data_dir.clone();
                let account =
                    cm_core::account::read_account_uuid(&dir.clone().unwrap_or_else(default_dir));
                Instance {
                    pid: p.pid,
                    windows: cm_win::window::app_windows(p.pid),
                    started_at: cm_win::process::start_time_millis(p.pid),
                    user_data_dir: dir,
                    account,
                }
            })
        })
        .collect()
}

fn default_dir() -> std::path::PathBuf {
    cm_win::known_paths()
        .map(|p| p.default_user_data_dir())
        .unwrap_or_default()
}

/// Total size of a folder tree, computed off the UI thread.
pub fn spawn_folder_size(tx: EventTx, id: cm_core::ProfileId, dir: std::path::PathBuf) {
    std::thread::spawn(move || {
        let bytes = dir_size(&dir);
        let _ = tx.send_blocking(AppEvent::FolderSize { id, bytes });
    });
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}
