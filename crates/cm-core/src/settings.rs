use serde::{Deserialize, Serialize};

/// App-wide preferences, persisted to `settings.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Accelerator that opens the quick switcher.
    pub switcher_hotkey: Option<String>,
    /// Closing the manager window hides it to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Register `cm.exe --background` in HKCU Run.
    pub start_with_windows: bool,
    /// Own the `claude://` scheme and route callbacks to the right profile.
    pub deep_link_broker: bool,
    /// Original `claude://` handler command, restored when the broker is disabled.
    pub saved_protocol_handler: Option<String>,
    /// Ask which profile a sign-in belongs to when it's unclear; otherwise use the last-used one.
    pub ask_when_unclear: bool,
    /// Show a notice when a newer GitHub release exists.
    pub check_for_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            switcher_hotkey: Some("Ctrl+Backquote".into()),
            close_to_tray: true,
            start_with_windows: false,
            deep_link_broker: true,
            saved_protocol_handler: None,
            ask_when_unclear: true,
            check_for_updates: true,
        }
    }
}
