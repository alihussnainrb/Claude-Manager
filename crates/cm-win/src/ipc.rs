//! Single-instance enforcement and a named pipe for forwarding commands to the running manager.
//!
//! A second `cm.exe` (launched by a shortcut, the tray relaunch command, or Windows for a
//! `claude://` URL) writes one UTF-8 message to the pipe and exits.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::io::FromRawHandle;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

use crate::wide;

const MAX_MESSAGE: u64 = 64 * 1024;

/// Held by the primary instance for its lifetime.
pub struct PrimaryGuard(HANDLE);
unsafe impl Send for PrimaryGuard {}
impl Drop for PrimaryGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub enum Instance {
    Primary(PrimaryGuard),
    Secondary,
}

fn session_id() -> u32 {
    let mut sid = 0u32;
    unsafe {
        let _ = ProcessIdToSessionId(std::process::id(), &mut sid);
    }
    sid
}

fn user_tag() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    let clean: String = user.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    format!("{clean}-{}", session_id())
}

pub fn pipe_name() -> String {
    format!(r"\\.\pipe\ClaudeManager-{}", user_tag())
}

/// Claim the single-instance mutex (scoped to this logon session).
pub fn acquire() -> io::Result<Instance> {
    let name = wide(&format!(
        r"Local\ClaudeManager.SingleInstance.{}",
        user_tag()
    ));
    unsafe {
        let h = CreateMutexW(None, true, PCWSTR(name.as_ptr())).map_err(io::Error::other)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            Ok(Instance::Secondary)
        } else {
            Ok(Instance::Primary(PrimaryGuard(h)))
        }
    }
}

/// Send one message to the primary instance, retrying briefly while it starts up.
pub fn send(message: &str) -> io::Result<()> {
    let name = pipe_name();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match OpenOptions::new().write(true).open(&name) {
            Ok(mut f) => {
                f.write_all(message.as_bytes())?;
                return f.flush();
            }
            Err(e)
                if Instant::now() < deadline
                    && (e.kind() == io::ErrorKind::NotFound
                        || e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32)) =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Serve the pipe on a background thread, calling `on_message` for every message received.
pub fn serve(on_message: impl Fn(String) + Send + 'static) -> io::Result<JoinHandle<()>> {
    let name = wide(&pipe_name());
    // Create the first instance up front so a squatter (or a second manager) fails loudly.
    // HANDLE isn't Send; carry the raw value across the thread boundary.
    let first = create_instance(&name, true)?.0 as isize;
    std::thread::Builder::new()
        .name("cm-ipc".into())
        .spawn(move || {
            let mut next = Some(first);
            loop {
                let h = match next.take() {
                    Some(raw) => HANDLE(raw as *mut _),
                    None => match create_instance(&name, false) {
                        Ok(h) => h,
                        Err(e) => {
                            tracing::error!("pipe create failed: {e}");
                            std::thread::sleep(Duration::from_secs(1));
                            continue;
                        }
                    },
                };
                let connected = unsafe { ConnectNamedPipe(h, None) };
                if connected.is_err() && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                    unsafe {
                        let _ = CloseHandle(h);
                    }
                    continue;
                }
                // SAFETY: we own `h`; File closes it on drop.
                let file = unsafe { File::from_raw_handle(h.0) };
                let mut buf = String::new();
                match file.take(MAX_MESSAGE).read_to_string(&mut buf) {
                    Ok(_) if !buf.is_empty() => on_message(buf),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("pipe read failed: {e}"),
                }
            }
        })
}

fn create_instance(name: &[u16], first: bool) -> io::Result<HANDLE> {
    let mut mode = PIPE_ACCESS_INBOUND;
    if first {
        mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let h = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            mode,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            0,
            MAX_MESSAGE as u32,
            0,
            None,
        )
    };
    if h.is_invalid() {
        return Err(io::Error::last_os_error());
    }
    Ok(h)
}
