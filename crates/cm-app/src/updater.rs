//! Updates from GitHub Releases: check, download + verify, swap the exe in place, restart.
//!
//! Each release carries `claude-manager.exe` and `claude-manager.exe.sha256`. Claude windows
//! are separate processes, so restarting the manager never interrupts them.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub const REPO: &str = "alihussnainrb/Claude-Manager";
const ASSET: &str = "claude-manager.exe";
const CHECKSUM: &str = "claude-manager.exe.sha256";
/// Refuse absurd downloads (the exe is ~15 MB).
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub page_url: String,
    exe_url: String,
    checksum_url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Downloading(Release),
    /// Downloaded and verified; installs on restart.
    Ready(Release, PathBuf),
    Failed(String),
}

impl UpdateState {
    pub fn ready(&self) -> Option<&Release> {
        match self {
            UpdateState::Ready(r, _) => Some(r),
            _ => None,
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(format!("ClaudeManager/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The latest published release, if it's newer than this build. Blocking.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = match agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        // No release published yet.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(format!("couldn't reach GitHub: {e}")),
    };
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    parse_release(&body, env!("CARGO_PKG_VERSION"))
}

fn parse_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let tag = v["tag_name"].as_str().ok_or("release has no tag")?;
    let version = tag.trim_start_matches('v').to_string();
    if !is_newer(&version, current) {
        return Ok(None);
    }
    let asset_url = |name: &str| {
        v["assets"].as_array().and_then(|a| {
            a.iter()
                .find(|x| x["name"].as_str() == Some(name))
                .and_then(|x| x["browser_download_url"].as_str())
                .map(str::to_owned)
        })
    };
    let (Some(exe_url), Some(checksum_url)) = (asset_url(ASSET), asset_url(CHECKSUM)) else {
        return Err(format!("release {tag} is missing {ASSET} or its checksum"));
    };
    Ok(Some(Release {
        version,
        page_url: v["html_url"].as_str().unwrap_or_default().to_string(),
        exe_url,
        checksum_url,
    }))
}

/// Numeric dotted-version comparison; anything after `-` or `+` is ignored.
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (mut r, mut l) = (parse(remote), parse(local));
    let n = r.len().max(l.len());
    r.resize(n, 0);
    l.resize(n, 0);
    r > l
}

fn updates_dir() -> Option<PathBuf> {
    Some(
        cm_win::local_appdata()?
            .join("ClaudeManager")
            .join("updates"),
    )
}

/// Download a release and verify it against its published SHA-256. Blocking.
pub fn download(release: &Release) -> Result<PathBuf, String> {
    let agent = agent();
    let checksum_text = agent
        .get(&release.checksum_url)
        .call()
        .map_err(|e| format!("checksum download failed: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let expected = checksum_text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if expected.len() != 64 {
        return Err("the release checksum is malformed".into());
    }

    let dir = updates_dir().ok_or("LOCALAPPDATA is not set")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("claude-manager-{}.exe", release.version));
    let partial = target.with_extension("exe.part");

    let mut resp = agent
        .get(&release.exe_url)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);

    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != expected {
        let _ = std::fs::remove_file(&partial);
        return Err("the download didn't match the release checksum, so it was discarded".into());
    }
    std::fs::rename(&partial, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Put `new_exe` where the running exe is and start it. The caller should quit right after.
///
/// A running exe can't be overwritten on Windows but it can be renamed, so the current one
/// moves aside to `.old` (deleted by the next start) and the new one takes its place.
pub fn install_and_restart(new_exe: &Path, background: bool) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = old_path(&current);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&current, &old)
        .map_err(|e| format!("couldn't move the old version aside: {e}"))?;
    if let Err(e) = std::fs::copy(new_exe, &current) {
        let _ = std::fs::rename(&old, &current);
        return Err(format!("couldn't install the new version: {e}"));
    }
    let mut args = vec!["--after-update".to_string(), std::process::id().to_string()];
    if background {
        args.push("--background".into());
    }
    cm_win::process::spawn(&current, &args, &[])
        .map_err(|e| format!("couldn't start the new version: {e}"))?;
    Ok(())
}

fn old_path(current: &Path) -> PathBuf {
    current.with_extension("exe.old")
}

/// Remove the previous version left behind by an update, and stale downloads.
pub fn cleanup() {
    if let Ok(current) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&current));
    }
    if let Some(dir) = updates_dir()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Wait for the previous instance to exit so the single-instance lock is free.
pub fn wait_for_exit(pid: u32) {
    for _ in 0..150 {
        if !cm_win::process::is_running(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.2.0-beta.1", "0.2.0"));
    }

    #[test]
    fn parses_release() {
        let json = r#"{
            "tag_name": "v0.2.0", "html_url": "https://github.com/x/y/releases/tag/v0.2.0",
            "draft": false, "prerelease": false,
            "assets": [
                {"name": "claude-manager.exe", "browser_download_url": "https://dl/exe"},
                {"name": "claude-manager.exe.sha256", "browser_download_url": "https://dl/sha"}
            ]
        }"#;
        let r = parse_release(json, "0.1.0").unwrap().unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.exe_url, "https://dl/exe");
        assert_eq!(parse_release(json, "0.2.0").unwrap(), None);
    }

    #[test]
    fn rejects_release_without_checksum() {
        let json = r#"{"tag_name":"v9.0.0","assets":[{"name":"claude-manager.exe","browser_download_url":"x"}]}"#;
        assert!(parse_release(json, "0.1.0").is_err());
    }

    #[test]
    fn skips_prereleases() {
        let json = r#"{"tag_name":"v9.0.0","prerelease":true,"assets":[]}"#;
        assert_eq!(parse_release(json, "0.1.0").unwrap(), None);
    }
}
