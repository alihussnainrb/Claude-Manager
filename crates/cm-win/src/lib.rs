//! Thin, safe wrappers over the Win32/WinRT APIs Claude Manager needs.
//!
//! Every `unsafe` block in the project lives in this crate.

pub mod appx;
pub mod ipc;
pub mod msgthread;
pub mod process;
pub mod registry;
pub mod shell;
pub mod window;

use std::path::PathBuf;

/// `%APPDATA%` and `%USERPROFILE%`, resolved from the environment.
pub fn known_paths() -> Option<cm_core::paths::Paths> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from)?;
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    Some(cm_core::paths::Paths::new(appdata, home))
}

/// `%LOCALAPPDATA%`.
pub fn local_appdata() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn wide_path(p: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
