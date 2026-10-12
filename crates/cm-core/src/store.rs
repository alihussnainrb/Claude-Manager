//! Persistence of profiles and settings as versioned JSON, written atomically.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::now_millis;
use crate::paths::Paths;
use crate::profile::{ClaudeCodeConfig, Profile, ProfileColor, ProfileIcon, ProfileId};
use crate::settings::Settings;
use crate::slug::unique_slug;

pub const PROFILES_VERSION: u32 = 1;
pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error(
        "{path} was written by a newer Claude Manager (format v{found}, this build reads up to v{supported})"
    )]
    TooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilesFile {
    pub version: u32,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsFile {
    pub version: u32,
    #[serde(flatten)]
    pub settings: Settings,
}

/// The built-in profile that maps to the stock `%APPDATA%\Claude` + `~\.claude` folders.
pub fn default_profile(paths: &Paths) -> Profile {
    Profile {
        id: ProfileId::DEFAULT,
        name: "Default".into(),
        slug: "default".into(),
        icon: ProfileIcon::Sparkle,
        color: ProfileColor::Coral,
        hotkey: None,
        autostart: false,
        auto_restart: true,
        user_data_dir: None,
        claude_code: ClaudeCodeConfig {
            enabled: false,
            config_dir: paths.default_code_config_dir(),
        },
        extra_args: Vec::new(),
        created_at: now_millis(),
        last_launched_at: None,
        cached_account_uuid: None,
    }
}

/// A fresh profile with its own Desktop and Claude Code folders.
pub fn new_profile(paths: &Paths, name: &str, existing: &[Profile]) -> Profile {
    let slug = unique_slug(name, |s| {
        existing.iter().any(|p| p.slug == s)
            || paths.profile_user_data_dir(s).exists()
            || paths.profile_code_config_dir(s).exists()
    });
    let used_colors: Vec<_> = existing.iter().map(|p| p.color).collect();
    let color = ProfileColor::ALL
        .into_iter()
        .find(|c| !used_colors.contains(c))
        .unwrap_or(ProfileColor::ALL[existing.len() % ProfileColor::ALL.len()]);
    Profile {
        id: ProfileId::new(),
        name: name.trim().to_string(),
        user_data_dir: Some(paths.profile_user_data_dir(&slug)),
        claude_code: ClaudeCodeConfig {
            enabled: true,
            config_dir: paths.profile_code_config_dir(&slug),
        },
        slug,
        icon: ProfileIcon::ALL[existing.len() % ProfileIcon::ALL.len()],
        color,
        hotkey: None,
        autostart: false,
        auto_restart: true,
        extra_args: Vec::new(),
        created_at: now_millis(),
        last_launched_at: None,
        cached_account_uuid: None,
    }
}

/// Load profiles; a missing file yields just the Default profile.
pub fn load_profiles(paths: &Paths) -> Result<Vec<Profile>, StoreError> {
    let path = paths.profiles_file();
    let Some(file) = read_json::<ProfilesFile>(&path)? else {
        return Ok(vec![default_profile(paths)]);
    };
    if file.version > PROFILES_VERSION {
        return Err(StoreError::TooNew {
            path,
            found: file.version,
            supported: PROFILES_VERSION,
        });
    }
    let mut profiles = file.profiles;
    if !profiles.iter().any(Profile::is_default) {
        profiles.insert(0, default_profile(paths));
    }
    Ok(profiles)
}

pub fn save_profiles(paths: &Paths, profiles: &[Profile]) -> Result<(), StoreError> {
    let file = ProfilesFile {
        version: PROFILES_VERSION,
        profiles: profiles.to_vec(),
    };
    write_json_atomic(&paths.profiles_file(), &file)
}

pub fn load_settings(paths: &Paths) -> Result<Settings, StoreError> {
    let path = paths.settings_file();
    match read_json::<SettingsFile>(&path)? {
        None => Ok(Settings::default()),
        Some(f) if f.version > SETTINGS_VERSION => Err(StoreError::TooNew {
            path,
            found: f.version,
            supported: SETTINGS_VERSION,
        }),
        Some(f) => Ok(f.settings),
    }
}

pub fn save_settings(paths: &Paths, settings: &Settings) -> Result<(), StoreError> {
    let file = SettingsFile {
        version: SETTINGS_VERSION,
        settings: settings.clone(),
    };
    write_json_atomic(&paths.settings_file(), &file)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StoreError> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(StoreError::Io {
                path: path.into(),
                source,
            });
        }
    };
    // Tolerate a UTF-8 BOM (files touched by Windows PowerShell 5.1 have one).
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|source| StoreError::Json {
            path: path.into(),
            source,
        })
}

/// Write to `<path>.tmp`, flush, then rename over `path` so readers never see a torn file.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let io = |source| StoreError::Io {
        path: path.into(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    let mut json = serde_json::to_vec_pretty(value).expect("serializable");
    json.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(io)?;
        f.write_all(&json).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    fs::rename(&tmp, path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("Roaming"), dir.path().join("home"));
        (dir, paths)
    }

    #[test]
    fn missing_file_gives_default() {
        let (_d, paths) = tmp_paths();
        let profiles = load_profiles(&paths).unwrap();
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].is_default());
        assert_eq!(profiles[0].user_data_dir, None);
    }

    #[test]
    fn roundtrip() {
        let (_d, paths) = tmp_paths();
        let mut profiles = load_profiles(&paths).unwrap();
        let work = new_profile(&paths, "Work", &profiles);
        assert_eq!(work.slug, "work");
        assert_eq!(
            work.user_data_dir.as_deref(),
            Some(paths.profile_user_data_dir("work").as_path())
        );
        assert_ne!(work.color, profiles[0].color);
        profiles.push(work);
        save_profiles(&paths, &profiles).unwrap();
        assert_eq!(load_profiles(&paths).unwrap(), profiles);
    }

    #[test]
    fn slug_avoids_existing_folders() {
        let (_d, paths) = tmp_paths();
        fs::create_dir_all(paths.profile_user_data_dir("work")).unwrap();
        let p = new_profile(&paths, "Work", &[]);
        assert_eq!(p.slug, "work-2");
    }

    #[test]
    fn rejects_newer_format() {
        let (_d, paths) = tmp_paths();
        write_json_atomic(
            &paths.profiles_file(),
            &serde_json::json!({"version": 99, "profiles": []}),
        )
        .unwrap();
        assert!(matches!(
            load_profiles(&paths),
            Err(StoreError::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn settings_defaults_fill_missing_fields() {
        let (_d, paths) = tmp_paths();
        write_json_atomic(
            &paths.settings_file(),
            &serde_json::json!({"version": 1, "close_to_tray": false}),
        )
        .unwrap();
        let s = load_settings(&paths).unwrap();
        assert!(!s.close_to_tray);
        assert!(s.deep_link_broker);
    }

    #[test]
    fn bom_tolerated() {
        let (_d, paths) = tmp_paths();
        fs::create_dir_all(paths.manager_dir()).unwrap();
        fs::write(paths.settings_file(), b"\xEF\xBB\xBF{\"version\":1}").unwrap();
        assert!(load_settings(&paths).is_ok());
    }
}
