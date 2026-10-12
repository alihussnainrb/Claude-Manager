//! Everything that reaches the UI thread from elsewhere: background threads, the tray, hotkeys,
//! window hooks and other `claude-manager.exe` invocations.

use std::path::PathBuf;

use cm_core::ProfileId;
use cm_win::appx::ClaudeInstall;
use cm_win::window::WindowHandle;
use serde::{Deserialize, Serialize};

/// Sent over the named pipe by a second `claude-manager.exe`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Bring the manager window forward.
    Show,
    /// Launch or focus a profile (taskbar relaunch command, shortcuts).
    Launch { id: String },
    /// A `claude://` URL handed to us by Windows.
    DeepLink { url: String },
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub pid: u32,
    /// `None` = launched without `--user-data-dir` (the stock folder).
    pub user_data_dir: Option<PathBuf>,
    pub windows: Vec<WindowHandle>,
    pub started_at: Option<i64>,
    pub account: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub install: Option<ClaudeInstall>,
    pub instances: Vec<Instance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    OpenManager,
    Profile(ProfileId),
    Switcher,
    ToggleRouting,
    Settings,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyTarget {
    Switcher,
    Profile(ProfileId),
}

#[derive(Debug)]
pub enum AppEvent {
    Scan(ScanResult),
    Exited {
        id: ProfileId,
        pid: u32,
        code: Option<i32>,
    },
    Command(Command),
    Tray(TrayAction),
    Hotkey(HotkeyTarget),
    Foreground {
        pid: u32,
    },
    ProtocolChanged,
    FolderSize {
        id: ProfileId,
        bytes: u64,
    },
    /// Accelerators another app already owns.
    HotkeyConflict(Vec<String>),
}

pub type EventTx = async_channel::Sender<AppEvent>;
