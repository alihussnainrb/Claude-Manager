//! Where everything lives on disk.

use std::path::{Path, PathBuf};

/// Root folders the rest of the layout is derived from. Resolved by the platform layer
/// (`%APPDATA%`, `%USERPROFILE%`) so this module stays pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `%APPDATA%` (Roaming).
    pub appdata: PathBuf,
    /// `%USERPROFILE%`.
    pub home: PathBuf,
}

/// Name of the stock Claude Desktop data folder under `%APPDATA%`.
pub const DEFAULT_USER_DATA_DIR_NAME: &str = "Claude";
const MANAGER_DIR_NAME: &str = "ClaudeManager";

impl Paths {
    pub fn new(appdata: impl Into<PathBuf>, home: impl Into<PathBuf>) -> Self {
        Self {
            appdata: appdata.into(),
            home: home.into(),
        }
    }

    pub fn manager_dir(&self) -> PathBuf {
        self.appdata.join(MANAGER_DIR_NAME)
    }

    pub fn profiles_file(&self) -> PathBuf {
        self.manager_dir().join("profiles.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.manager_dir().join("settings.json")
    }

    pub fn state_file(&self) -> PathBuf {
        self.manager_dir().join("state.json")
    }

    pub fn icons_dir(&self) -> PathBuf {
        self.manager_dir().join("icons")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.manager_dir().join("logs")
    }

    /// `%APPDATA%\Claude` — what Claude Desktop uses when launched without `--user-data-dir`.
    pub fn default_user_data_dir(&self) -> PathBuf {
        self.appdata.join(DEFAULT_USER_DATA_DIR_NAME)
    }

    /// `%APPDATA%\Claude-<slug>`. Must sit directly under `%APPDATA%`: the Cowork VM service
    /// looks for `vm_bundles` there and ignores `--user-data-dir` otherwise.
    pub fn profile_user_data_dir(&self, slug: &str) -> PathBuf {
        self.appdata
            .join(format!("{DEFAULT_USER_DATA_DIR_NAME}-{slug}"))
    }

    /// `~\.claude` — Claude Code's stock config dir.
    pub fn default_code_config_dir(&self) -> PathBuf {
        self.home.join(".claude")
    }

    /// `~\.claude.json` — Claude Code's stock global state file (holds `mcpServers`).
    pub fn default_code_state_file(&self) -> PathBuf {
        self.home.join(".claude.json")
    }

    /// `~\.claude-<slug>`.
    pub fn profile_code_config_dir(&self, slug: &str) -> PathBuf {
        self.home.join(format!(".claude-{slug}"))
    }

    /// Resolve a profile's effective user-data-dir (`None` = stock folder).
    pub fn effective_user_data_dir(&self, dir: Option<&Path>) -> PathBuf {
        dir.map(Path::to_path_buf)
            .unwrap_or_else(|| self.default_user_data_dir())
    }

    /// True when `dir` is a direct child of `%APPDATA%` (required for Cowork to work).
    pub fn is_cowork_compatible(&self, dir: &Path) -> bool {
        dir.parent().is_some_and(|p| same_path(p, &self.appdata))
    }
}

/// Case-insensitive, separator-insensitive comparison suitable for Windows paths.
pub fn same_path(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

/// Lowercased, `/`→`\`, no trailing separator, no `\\?\` prefix.
pub fn normalize(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        Paths::new(r"C:\Users\me\AppData\Roaming", r"C:\Users\me")
    }

    #[test]
    fn layout() {
        let p = paths();
        assert_eq!(
            p.profile_user_data_dir("work"),
            PathBuf::from(r"C:\Users\me\AppData\Roaming\Claude-work")
        );
        assert_eq!(
            p.profile_code_config_dir("work"),
            PathBuf::from(r"C:\Users\me\.claude-work")
        );
        assert_eq!(
            p.profiles_file(),
            PathBuf::from(r"C:\Users\me\AppData\Roaming\ClaudeManager\profiles.json")
        );
    }

    #[test]
    fn cowork_compat() {
        let p = paths();
        assert!(p.is_cowork_compatible(&p.profile_user_data_dir("x")));
        assert!(p.is_cowork_compatible(Path::new(r"c:/users/ME/appdata/roaming/Other")));
        assert!(!p.is_cowork_compatible(Path::new(r"D:\Claude\x")));
        assert!(!p.is_cowork_compatible(Path::new(r"C:\Users\me\AppData\Roaming\a\b")));
    }

    #[test]
    fn normalize_variants() {
        assert!(same_path(Path::new(r"\\?\C:\A\B\"), Path::new("c:/a/b")));
    }
}
