//! Locating the installed Claude Desktop executable.
//!
//! The Microsoft Store (MSIX) build lives under a versioned `WindowsApps` folder that changes on
//! every update, so it is resolved again before each launch. The legacy Squirrel installer
//! lives under `%LOCALAPPDATA%\AnthropicClaude`.

use std::path::{Path, PathBuf};

use windows::Management::Deployment::PackageManager;
use windows::core::HSTRING;

/// Package family name of the Microsoft Store build.
pub const CLAUDE_PACKAGE_FAMILY: &str = "Claude_pzs8sxrjxfjjc";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    Msix { package_family: String },
    Squirrel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeInstall {
    pub exe: PathBuf,
    pub version: String,
    pub kind: InstallKind,
}

impl ClaudeInstall {
    /// True when `image` is an executable belonging to this install (e.g. a running process).
    pub fn owns_image(&self, image: &Path) -> bool {
        let Some(root) = self.exe.parent() else {
            return false;
        };
        let root = cm_core::paths::normalize(root);
        cm_core::paths::normalize(image).starts_with(&root)
    }
}

pub fn find_claude() -> Option<ClaudeInstall> {
    match find_msix() {
        Ok(Some(i)) => return Some(i),
        Ok(None) => {}
        Err(e) => tracing::warn!("MSIX lookup failed: {e}"),
    }
    find_squirrel()
}

fn find_msix() -> windows::core::Result<Option<ClaudeInstall>> {
    let pm = PackageManager::new()?;
    let user = HSTRING::new(); // current user
    let by_family = pm.FindPackagesByUserSecurityIdPackageFamilyName(
        &user,
        &HSTRING::from(CLAUDE_PACKAGE_FAMILY),
    )?;
    if let Some(i) = newest(by_family) {
        return Ok(Some(i));
    }
    // Family name changed (new signing publisher?) — fall back to a full scan by name.
    Ok(newest(pm.FindPackagesByUserSecurityId(&user)?))
}

fn newest(
    pkgs: impl IntoIterator<Item = windows::ApplicationModel::Package>,
) -> Option<ClaudeInstall> {
    pkgs.into_iter()
        .filter_map(|pkg| match inspect(&pkg) {
            Ok(found) => found,
            Err(e) => {
                tracing::debug!("skipping package: {e}");
                None
            }
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, install)| install)
}

/// `(version rank, install)` when `pkg` is Claude Desktop with its exe present.
fn inspect(
    pkg: &windows::ApplicationModel::Package,
) -> windows::core::Result<Option<(u64, ClaudeInstall)>> {
    let id = pkg.Id()?;
    if id.Name()?.to_string_lossy() != "Claude" {
        return Ok(None);
    }
    let v = id.Version()?;
    let rank = ((v.Major as u64) << 48)
        | ((v.Minor as u64) << 32)
        | ((v.Build as u64) << 16)
        | v.Revision as u64;
    let exe = PathBuf::from(pkg.InstalledPath()?.to_string_lossy())
        .join("app")
        .join("Claude.exe");
    if !exe.exists() {
        return Ok(None);
    }
    let install = ClaudeInstall {
        exe,
        version: format!("{}.{}.{}.{}", v.Major, v.Minor, v.Build, v.Revision),
        kind: InstallKind::Msix {
            package_family: id.FamilyName()?.to_string_lossy(),
        },
    };
    Ok(Some((rank, install)))
}

fn find_squirrel() -> Option<ClaudeInstall> {
    let root = crate::local_appdata()?.join("AnthropicClaude");
    let stub = root.join("claude.exe");
    if !stub.exists() {
        return None;
    }
    let version = std::fs::read_dir(&root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_prefix("app-")
                .map(str::to_owned)
        })
        .max_by(|a, b| compare_versions(a, b))
        .unwrap_or_else(|| "unknown".into());
    Some(ClaudeInstall {
        exe: stub,
        version,
        kind: InstallKind::Squirrel,
    })
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| {
        s.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    parse(a).cmp(&parse(b))
}
