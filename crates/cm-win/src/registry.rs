//! HKCU registry helpers: the `claude://` protocol handler, the Run key, and change watching.

use std::io;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, KEY_SET_VALUE, REG_NOTIFY_CHANGE_LAST_SET,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW,
    RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::PCWSTR;

use crate::wide;

pub const PROTOCOL_KEY: &str = r"Software\Classes\claude";
pub const PROTOCOL_COMMAND_KEY: &str = r"Software\Classes\claude\shell\open\command";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "ClaudeManager";

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn err(code: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(code.0 as i32)
}

fn open(subkey: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> io::Result<Key> {
    let mut h = HKEY::default();
    let sub = wide(subkey);
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            Some(0),
            access,
            &mut h,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(err(rc));
    }
    Ok(Key(h))
}

fn create(subkey: &str) -> io::Result<Key> {
    let mut h = HKEY::default();
    let sub = wide(subkey);
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_READ,
            None,
            &mut h,
            None,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(err(rc));
    }
    Ok(Key(h))
}

/// Read a string value from HKCU (`value = ""` reads the default value).
pub fn get_string(subkey: &str, value: &str) -> io::Result<Option<String>> {
    let key = match open(subkey, KEY_READ) {
        Ok(k) => k,
        Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND.0 as i32) => return Ok(None),
        Err(e) => return Err(e),
    };
    let name = wide(value);
    let mut ty = REG_VALUE_TYPE::default();
    let mut len = 0u32;
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut len),
        )
    };
    if rc == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if rc != ERROR_SUCCESS {
        return Err(err(rc));
    }
    let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut len),
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(err(rc));
    }
    Ok(Some(crate::from_wide(&buf)))
}

/// Write a `REG_SZ` value under HKCU, creating the key if needed.
pub fn set_string(subkey: &str, value: &str, data: &str) -> io::Result<()> {
    let key = create(subkey)?;
    let name = wide(value);
    let data = wide(data);
    let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };
    let rc = unsafe { RegSetValueExW(key.0, PCWSTR(name.as_ptr()), Some(0), REG_SZ, Some(bytes)) };
    if rc != ERROR_SUCCESS {
        return Err(err(rc));
    }
    Ok(())
}

pub fn delete_value(subkey: &str, value: &str) -> io::Result<()> {
    let key = match open(subkey, KEY_SET_VALUE) {
        Ok(k) => k,
        Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND.0 as i32) => return Ok(()),
        Err(e) => return Err(e),
    };
    let name = wide(value);
    let rc = unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) };
    if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
        return Err(err(rc));
    }
    Ok(())
}

// ---- claude:// protocol handler -------------------------------------------------------------

/// The command currently registered for `claude://` in HKCU.
pub fn protocol_handler() -> io::Result<Option<String>> {
    get_string(PROTOCOL_COMMAND_KEY, "")
}

/// Point `claude://` at `command` (which must contain `"%1"`).
pub fn set_protocol_handler(command: &str) -> io::Result<()> {
    set_string(PROTOCOL_KEY, "", "URL:claude")?;
    set_string(PROTOCOL_KEY, "URL Protocol", "")?;
    set_string(PROTOCOL_COMMAND_KEY, "", command)
}

// ---- Start with Windows ---------------------------------------------------------------------

pub fn run_entry() -> io::Result<Option<String>> {
    get_string(RUN_KEY, RUN_VALUE)
}

pub fn set_run_entry(command: Option<&str>) -> io::Result<()> {
    match command {
        Some(c) => set_string(RUN_KEY, RUN_VALUE, c),
        None => delete_value(RUN_KEY, RUN_VALUE),
    }
}

// ---- Watching -------------------------------------------------------------------------------

/// Block the calling thread until a value under `subkey` (or its subkeys) changes.
/// Returns an error if the key can't be opened (e.g. it was deleted).
pub fn wait_for_change(subkey: &str) -> io::Result<()> {
    let key = open(subkey, KEY_NOTIFY)?;
    let rc =
        unsafe { RegNotifyChangeKeyValue(key.0, true, REG_NOTIFY_CHANGE_LAST_SET, None, false) };
    if rc != ERROR_SUCCESS {
        return Err(err(rc));
    }
    Ok(())
}

// ---- Registered URL handler -----------------------------------------------------------------
//
// Store-installed Claude claims claude:// through its package manifest, and Windows prefers
// that over `Software\Classes\claude`. The supported way in is to register as a handler
// (ProgID + Capabilities + RegisteredApplications), then let the user pick us once in
// Windows' Default apps; their choice lands in UserChoice, which wins over everything.

pub const URL_PROGID: &str = "ClaudeManager.claude";
pub const APP_NAME: &str = "Claude Manager";
const CAPABILITIES_KEY: &str = r"Software\ClaudeManager\Capabilities";
const REGISTERED_APPS_KEY: &str = r"Software\RegisteredApplications";
const USER_CHOICE_KEY: &str =
    r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\claude\UserChoice";
/// Newer Windows 11 builds store the choice here (value `ProgId`) instead.
const USER_CHOICE_LATEST_KEY: &str =
    r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\claude\UserChoiceLatest\ProgId";

/// Register `command` (which must contain `"%1"`) as a handler Windows can offer for claude://.
pub fn register_url_handler(command: &str, icon: &str) -> io::Result<()> {
    let progid = format!(r"Software\Classes\{URL_PROGID}");
    set_string(&progid, "", "Claude sign-in link")?;
    set_string(&progid, "URL Protocol", "")?;
    set_string(&format!(r"{progid}\DefaultIcon"), "", icon)?;
    set_string(&format!(r"{progid}\shell\open\command"), "", command)?;
    set_string(
        &format!(r"{progid}\Application"),
        "ApplicationName",
        APP_NAME,
    )?;
    set_string(CAPABILITIES_KEY, "ApplicationName", APP_NAME)?;
    set_string(
        CAPABILITIES_KEY,
        "ApplicationDescription",
        "Sends each Claude sign-in to the profile that asked for it.",
    )?;
    set_string(
        &format!(r"{CAPABILITIES_KEY}\UrlAssociations"),
        "claude",
        URL_PROGID,
    )?;
    set_string(REGISTERED_APPS_KEY, APP_NAME, CAPABILITIES_KEY)
}

pub fn unregister_url_handler() -> io::Result<()> {
    delete_value(REGISTERED_APPS_KEY, APP_NAME)?;
    delete_tree(r"Software\ClaudeManager")?;
    delete_tree(&format!(r"Software\Classes\{URL_PROGID}"))
}

/// The ProgID the user picked for claude:// in Windows, if any.
pub fn url_user_choice() -> Option<String> {
    get_string(USER_CHOICE_LATEST_KEY, "ProgId")
        .ok()
        .flatten()
        .or_else(|| get_string(USER_CHOICE_KEY, "ProgId").ok().flatten())
}

/// True when Windows will hand claude:// links to us.
pub fn url_handler_is_ours() -> bool {
    url_user_choice().as_deref() == Some(URL_PROGID)
}

fn delete_tree(subkey: &str) -> io::Result<()> {
    use windows::Win32::System::Registry::RegDeleteTreeW;
    let sub = wide(subkey);
    let rc = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr())) };
    if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
        return Err(err(rc));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = r"Software\ClaudeManager-Test";

    #[test]
    fn string_roundtrip() {
        set_string(TEST_KEY, "v", "hello wörld").unwrap();
        assert_eq!(
            get_string(TEST_KEY, "v").unwrap().as_deref(),
            Some("hello wörld")
        );
        delete_value(TEST_KEY, "v").unwrap();
        assert_eq!(get_string(TEST_KEY, "v").unwrap(), None);
        assert_eq!(
            get_string(r"Software\ClaudeManager-Missing", "v").unwrap(),
            None
        );
        unsafe {
            let sub = wide(TEST_KEY);
            let _ = windows::Win32::System::Registry::RegDeleteKeyW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
            );
        }
    }
}
