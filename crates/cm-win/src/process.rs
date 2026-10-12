//! Launching and inspecting processes.

use std::ffi::c_void;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use windows::Wdk::System::Threading::{NtQueryInformationProcess, PROCESSINFOCLASS};
use windows::Win32::Foundation::{CloseHandle, HANDLE, STILL_ACTIVE, UNICODE_STRING};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
};
use windows::core::PWSTR;

/// `ProcessCommandLineInformation` (Windows 8.1+); readable with only
/// `PROCESS_QUERY_LIMITED_INFORMATION`, unlike walking the PEB.
const PROCESS_COMMAND_LINE_INFORMATION: PROCESSINFOCLASS = PROCESSINFOCLASS(60);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub image_path: Option<PathBuf>,
    pub cmdline: Option<String>,
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// All processes whose executable file name equals `exe_name` (case-insensitive), with image
/// path and command line filled in where access allows.
pub fn list_by_name(exe_name: &str) -> Vec<ProcInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let snap = OwnedHandle(snap);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap.0, &mut entry).is_ok();
        while ok {
            let name = crate::from_wide(&entry.szExeFile);
            if name.eq_ignore_ascii_case(exe_name) {
                let pid = entry.th32ProcessID;
                let (image_path, cmdline) =
                    match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                        Ok(h) => {
                            let h = OwnedHandle(h);
                            (image_path(h.0), command_line(h.0))
                        }
                        Err(_) => (None, None),
                    };
                out.push(ProcInfo {
                    pid,
                    parent_pid: entry.th32ParentProcessID,
                    image_path,
                    cmdline,
                });
            }
            ok = Process32NextW(snap.0, &mut entry).is_ok();
        }
    }
    out
}

unsafe fn image_path(h: HANDLE) -> Option<PathBuf> {
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).ok()?
    };
    Some(PathBuf::from(String::from_utf16_lossy(
        &buf[..len as usize],
    )))
}

unsafe fn command_line(h: HANDLE) -> Option<String> {
    let mut needed = 0u32;
    // First call sizes the buffer (returns STATUS_INFO_LENGTH_MISMATCH).
    let _ = unsafe {
        NtQueryInformationProcess(
            h,
            PROCESS_COMMAND_LINE_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    if needed == 0 {
        return None;
    }
    // u64 backing keeps the UNICODE_STRING header aligned.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let status = unsafe {
        NtQueryInformationProcess(
            h,
            PROCESS_COMMAND_LINE_INFORMATION,
            buf.as_mut_ptr() as *mut c_void,
            needed,
            &mut needed,
        )
    };
    if status.is_err() {
        return None;
    }
    let us = unsafe { &*(buf.as_ptr() as *const UNICODE_STRING) };
    if us.Buffer.is_null() || us.Length == 0 {
        return Some(String::new());
    }
    let chars = unsafe { std::slice::from_raw_parts(us.Buffer.0, us.Length as usize / 2) };
    Some(String::from_utf16_lossy(chars))
}

/// Exit code of `pid` if it has exited, `None` while running (or if it can't be opened).
pub fn exit_code(pid: u32) -> Option<u32> {
    unsafe {
        let h = OwnedHandle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut code = 0u32;
        GetExitCodeProcess(h.0, &mut code).ok()?;
        (code != STILL_ACTIVE.0 as u32).then_some(code)
    }
}

/// When `pid` started, in milliseconds since the Unix epoch.
pub fn start_time_millis(pid: u32) -> Option<i64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetProcessTimes;
    unsafe {
        let h = OwnedHandle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(h.0, &mut created, &mut exited, &mut kernel, &mut user).ok()?;
        let ticks = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        // FILETIME counts 100ns intervals since 1601-01-01.
        Some((ticks.checked_sub(116_444_736_000_000_000)? / 10_000) as i64)
    }
}

pub fn is_running(pid: u32) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let h = OwnedHandle(h);
        let mut code = 0u32;
        GetExitCodeProcess(h.0, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32
    }
}

pub fn terminate(pid: u32, exit_code: u32) -> io::Result<()> {
    unsafe {
        let h = OwnedHandle(OpenProcess(PROCESS_TERMINATE, false, pid).map_err(io::Error::other)?);
        TerminateProcess(h.0, exit_code).map_err(io::Error::other)
    }
}

/// Start a GUI program detached from our console, with extra environment variables.
pub fn spawn(exe: &Path, args: &[String], env: &[(String, String)]) -> io::Result<Child> {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.spawn()
}

/// Start a console helper without flashing a window.
pub fn spawn_hidden(exe: &Path, args: &[String]) -> io::Result<Child> {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_own_process_cmdline() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_string();
        let procs = list_by_name(&name);
        let mine = procs
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("own process listed");
        assert!(mine.cmdline.as_deref().unwrap_or_default().contains(&name));
        assert!(is_running(mine.pid));
        assert_eq!(exit_code(mine.pid), None);
    }
}
