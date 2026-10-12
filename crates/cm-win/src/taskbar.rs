//! Give each profile's Claude window its own taskbar button: a separate AppUserModelID (so
//! windows group per profile and pins relaunch the right one) plus the profile's icon.
//!
//! Must run on a thread with COM initialized ([`com_init`]).

use std::path::Path;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
    PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemAlloc};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, HICON, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_LOADFROMFILE, LoadImageW,
    SM_CXICON, SM_CXSMICON, SendMessageW, WM_SETICON,
};
use windows::core::{PCWSTR, PWSTR, Result};

use crate::window::WindowHandle;
use crate::{wide, wide_path};

pub struct Identity<'a> {
    pub app_id: &'a str,
    pub icon: &'a Path,
}

pub fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// A `VT_LPWSTR` variant (what the AppUserModel keys require; `VT_BSTR` is rejected).
fn lpwstr(s: &str) -> PROPVARIANT {
    let w = wide(s);
    unsafe {
        let buf = CoTaskMemAlloc(w.len() * 2) as *mut u16;
        std::ptr::copy_nonoverlapping(w.as_ptr(), buf, w.len());
        let mut pv = PROPVARIANT::default();
        let inner = &mut *pv.Anonymous.Anonymous;
        inner.vt = VT_LPWSTR;
        inner.Anonymous.pwszVal = PWSTR(buf);
        pv
    }
}

/// Tag a window with a profile's taskbar identity and icon.
pub fn apply(h: WindowHandle, id: &Identity) -> Result<()> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(h.hwnd())?;
        // Only the ID. Relaunch properties (for pinning) make the taskbar ignore the window
        // icon on Store-installed Claude and show a blank button, and that result is cached
        // per ID. Clear any left from older builds.
        let empty = PROPVARIANT::default();
        store.SetValue(&PKEY_AppUserModel_RelaunchCommand, &empty)?;
        store.SetValue(&PKEY_AppUserModel_RelaunchDisplayNameResource, &empty)?;
        store.SetValue(&PKEY_AppUserModel_RelaunchIconResource, &empty)?;
        store.SetValue(&PKEY_AppUserModel_ID, &lpwstr(id.app_id))?;
        store.Commit()?;
    }
    set_icon(h, id.icon);
    Ok(())
}

/// Remove our identity so the window groups with Claude again.
pub fn clear(h: WindowHandle) -> Result<()> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(h.hwnd())?;
        let empty = PROPVARIANT::default();
        store.SetValue(&PKEY_AppUserModel_ID, &empty)?;
        store.SetValue(&PKEY_AppUserModel_RelaunchCommand, &empty)?;
        store.SetValue(&PKEY_AppUserModel_RelaunchDisplayNameResource, &empty)?;
        store.SetValue(&PKEY_AppUserModel_RelaunchIconResource, &empty)?;
        store.Commit()?;
        // A null icon falls back to the window class icon (Claude's own).
        SendMessageW(
            h.hwnd(),
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(0)),
        );
        SendMessageW(
            h.hwnd(),
            WM_SETICON,
            Some(WPARAM(ICON_SMALL as usize)),
            Some(LPARAM(0)),
        );
    }
    Ok(())
}

/// Set a window's title-bar and taskbar icon from an .ico file. The icons are intentionally
/// leaked: the window keeps using them for as long as it lives.
pub fn set_icon(h: WindowHandle, ico: &Path) {
    let path = wide_path(ico);
    let load = |metric| unsafe {
        let size = GetSystemMetrics(metric);
        LoadImageW(
            None,
            PCWSTR(path.as_ptr()),
            IMAGE_ICON,
            size,
            size,
            LR_LOADFROMFILE,
        )
        .ok()
        .map(|h| HICON(h.0))
    };
    unsafe {
        if let Some(big) = load(SM_CXICON) {
            SendMessageW(
                h.hwnd(),
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(big.0 as isize)),
            );
        }
        if let Some(small) = load(SM_CXSMICON) {
            SendMessageW(
                h.hwnd(),
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(small.0 as isize)),
            );
        }
    }
}
