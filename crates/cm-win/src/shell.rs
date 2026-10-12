//! Explorer, Recycle Bin and terminal helpers.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use windows::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW,
    SHFileOperationW, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::wide_path;

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Open a folder (or file) with its default handler.
pub fn open_path(path: &Path) -> io::Result<()> {
    let p = wide_path(path);
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(p.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute returns a pseudo-HINSTANCE > 32 on success.
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "ShellExecute failed ({})",
            r.0 as isize
        )))
    }
}

/// Move a file or folder to the Recycle Bin. Never deletes permanently.
pub fn recycle(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    // pFrom must be double-NUL terminated.
    let mut from = wide_path(path);
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    let rc = unsafe { SHFileOperationW(&mut op) };
    if rc != 0 || op.fAnyOperationsAborted.as_bool() {
        return Err(io::Error::other(format!(
            "moving {} to the Recycle Bin failed (code {rc})",
            path.display()
        )));
    }
    Ok(())
}

/// Open a PowerShell terminal whose environment points Claude Code at `config_dir`, then
/// start `claude`. Uses Windows Terminal when installed.
///
/// The script is passed with `-EncodedCommand` so paths and names need no quoting and
/// Windows Terminal's `;` command separator can't split it.
pub fn open_claude_terminal(title: &str, config_dir: &Path, cwd: &Path) -> io::Result<()> {
    let script = format!(
        "$env:CLAUDE_CONFIG_DIR = '{dir}'\n\
         $Host.UI.RawUI.WindowTitle = '{title}'\n\
         Write-Host \"Claude Code profile: {title}\" -ForegroundColor Cyan\n\
         Write-Host \"CLAUDE_CONFIG_DIR = $env:CLAUDE_CONFIG_DIR\" -ForegroundColor DarkGray\n\
         if (Get-Command claude -ErrorAction SilentlyContinue) {{ claude }} else {{ Write-Host 'claude is not on PATH. Install Claude Code: https://claude.com/code' -ForegroundColor Yellow }}\n",
        dir = ps_quote(&config_dir.to_string_lossy()),
        title = ps_quote(title),
    );
    let encoded = base64_utf16le(&script);
    let ps_args = [
        "-NoLogo",
        "-NoExit",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encoded,
    ];

    let wt = Command::new("wt.exe")
        .args(["-w", "new", "new-tab", "--title", title, "-d"])
        .arg(cwd)
        .arg("powershell.exe")
        .args(ps_args)
        .spawn();
    if wt.is_ok() {
        return Ok(());
    }
    Command::new("powershell.exe")
        .args(ps_args)
        .current_dir(cwd)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(|_| ())
}

/// Escape for a single-quoted PowerShell string.
fn ps_quote(s: &str) -> String {
    s.replace('\'', "''")
}

fn base64_utf16le(s: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64() {
        // "hi" as UTF-16LE = 68 00 69 00
        assert_eq!(base64_utf16le("hi"), "aABpAA==");
        assert_eq!(base64_utf16le(""), "");
        assert_eq!(base64_utf16le("a"), "YQA=");
    }

    #[test]
    fn quoting() {
        assert_eq!(ps_quote("it's"), "it''s");
    }
}
