//! Finding and focusing top-level windows.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GW_OWNER, GetClassNameW, GetForegroundWindow, GetWindow,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    PostMessageW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow, WM_CLOSE,
};
use windows::core::BOOL;

/// A window handle that can cross threads. Only meaningful while the window exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(pub isize);

impl WindowHandle {
    pub fn hwnd(self) -> HWND {
        HWND(self.0 as *mut _)
    }
    pub fn from_hwnd(h: HWND) -> Self {
        Self(h.0 as isize)
    }
    pub fn is_alive(self) -> bool {
        unsafe { IsWindow(Some(self.hwnd())).as_bool() }
    }
}

/// Class name Electron/Chromium uses for its top-level windows.
pub const ELECTRON_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";

pub fn class_name(h: WindowHandle) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h.hwnd(), &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

pub fn process_id(h: WindowHandle) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h.hwnd(), Some(&mut pid)) };
    pid
}

/// Unowned top-level windows of `pid` with the Electron window class. Includes hidden ones
/// (Claude hides its main window when closed to tray); visible windows come first.
pub fn app_windows(pid: u32) -> Vec<WindowHandle> {
    struct Ctx {
        pid: u32,
        found: Vec<(bool, WindowHandle)>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
        let h = WindowHandle::from_hwnd(hwnd);
        if process_id(h) == ctx.pid
            && unsafe { GetWindow(hwnd, GW_OWNER) }.map_or(true, |o| o.is_invalid())
            && class_name(h) == ELECTRON_WINDOW_CLASS
            && unsafe { GetWindowTextLengthW(hwnd) } > 0
        {
            ctx.found
                .push((unsafe { IsWindowVisible(hwnd) }.as_bool(), h));
        }
        true.into()
    }
    let mut ctx = Ctx {
        pid,
        found: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.found.sort_by_key(|(visible, _)| !visible);
    ctx.found.into_iter().map(|(_, h)| h).collect()
}

pub fn foreground() -> Option<WindowHandle> {
    let h = unsafe { GetForegroundWindow() };
    (!h.is_invalid()).then(|| WindowHandle::from_hwnd(h))
}

/// Restore and bring a window to the foreground, working around the foreground lock.
pub fn focus(h: WindowHandle) -> bool {
    let hwnd = h.hwnd();
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = BringWindowToTop(hwnd);
        if SetForegroundWindow(hwnd).as_bool() {
            return true;
        }
        // Windows only lets the process that received the last input event steal focus.
        // A synthetic Alt press makes us that process.
        let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
        SendInput(&inputs, size_of::<INPUT>() as i32);
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// Ask a window to close (same as clicking its X button).
pub fn request_close(h: WindowHandle) {
    unsafe {
        let _ = PostMessageW(Some(h.hwnd()), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}
