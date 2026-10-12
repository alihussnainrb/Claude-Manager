//! Platform-independent model and logic for Claude Manager.
//!
//! Nothing in this crate touches Win32; everything here is unit-testable on any host.

pub mod account;
pub mod cmdline;
pub mod export;
pub mod mcp;
pub mod paths;
pub mod profile;
pub mod routing;
pub mod seed;
pub mod settings;
pub mod slug;
pub mod store;

pub use profile::{ClaudeCodeConfig, Profile, ProfileColor, ProfileIcon, ProfileId};
pub use settings::Settings;

/// Milliseconds since the Unix epoch. Used for all persisted timestamps.
pub type UnixMillis = i64;

pub fn now_millis() -> UnixMillis {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
