//! Import/export of profiles and settings as a single JSON file. Contains no credentials:
//! logins live in each profile's data folder, which is never exported.

use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::profile::Profile;
use crate::settings::Settings;
use crate::{UnixMillis, now_millis};

pub const FORMAT: &str = "claude-manager-export";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportBundle {
    pub format: String,
    pub version: u32,
    pub exported_at: UnixMillis,
    pub profiles: Vec<Profile>,
    pub settings: Settings,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("not a Claude Manager export file")]
    WrongFormat,
    #[error("export format v{0} is newer than this build supports")]
    TooNew(u32),
    #[error("invalid JSON: {0}")]
    Json(String),
}

pub fn export(profiles: &[Profile], settings: &Settings) -> ExportBundle {
    let mut settings = settings.clone();
    // Machine-specific; restoring someone else's handler path would break deep links.
    settings.saved_protocol_handler = None;
    let profiles = profiles
        .iter()
        .cloned()
        .map(|mut p| {
            p.cached_account_uuid = None;
            p
        })
        .collect();
    ExportBundle {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now_millis(),
        profiles,
        settings,
    }
}

pub fn parse(text: &str) -> Result<ExportBundle, ImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let raw: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ImportError::Json(e.to_string()))?;
    if raw.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(ImportError::WrongFormat);
    }
    let version = raw.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > VERSION {
        return Err(ImportError::TooNew(version));
    }
    serde_json::from_value(raw).map_err(|e| ImportError::Json(e.to_string()))
}

/// Merge imported profiles into `existing`. Profiles with the same id are replaced; others are
/// appended. Paths from another machine (or user) are remapped to this machine's layout.
pub fn merge(paths: &Paths, existing: &[Profile], imported: Vec<Profile>) -> Vec<Profile> {
    let mut out = existing.to_vec();
    for mut p in imported {
        remap_paths(paths, &mut p);
        let keep_runtime = out
            .iter()
            .find(|e| e.id == p.id)
            .map(|e| e.cached_account_uuid.clone());
        if let Some(acc) = keep_runtime {
            p.cached_account_uuid = acc;
        }
        match out.iter_mut().find(|e| e.id == p.id) {
            Some(slot) => *slot = p,
            None => {
                if out.iter().any(|e| e.slug == p.slug) {
                    p.slug = crate::slug::unique_slug(&p.slug, |s| out.iter().any(|e| e.slug == s));
                    p.user_data_dir = Some(paths.profile_user_data_dir(&p.slug));
                    p.claude_code.config_dir = paths.profile_code_config_dir(&p.slug);
                }
                out.push(p);
            }
        }
    }
    out
}

fn remap_paths(paths: &Paths, p: &mut Profile) {
    if p.is_default() {
        p.user_data_dir = None;
        p.claude_code.config_dir = paths.default_code_config_dir();
        return;
    }
    if let Some(dir) = &p.user_data_dir
        && !dir.starts_with(&paths.appdata)
        && !dir.exists()
    {
        p.user_data_dir = Some(paths.profile_user_data_dir(&p.slug));
    }
    if !p.claude_code.config_dir.starts_with(&paths.home) && !p.claude_code.config_dir.exists() {
        p.claude_code.config_dir = paths.profile_code_config_dir(&p.slug);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{default_profile, new_profile};

    #[test]
    fn roundtrip_strips_machine_state() {
        let paths = Paths::new(r"C:\Users\a\AppData\Roaming", r"C:\Users\a");
        let mut d = default_profile(&paths);
        d.cached_account_uuid = Some("acc".into());
        let settings = Settings {
            saved_protocol_handler: Some("x".into()),
            ..Default::default()
        };
        let bundle = export(&[d], &settings);
        let text = serde_json::to_string(&bundle).unwrap();
        let parsed = parse(&text).unwrap();
        assert_eq!(parsed.settings.saved_protocol_handler, None);
        assert_eq!(parsed.profiles[0].cached_account_uuid, None);
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(parse(r#"{"version":1}"#), Err(ImportError::WrongFormat));
        assert_eq!(
            parse(r#"{"format":"claude-manager-export","version":7}"#),
            Err(ImportError::TooNew(7))
        );
        assert!(matches!(parse("nope"), Err(ImportError::Json(_))));
    }

    #[test]
    fn merge_remaps_foreign_paths() {
        let theirs = Paths::new(r"C:\Users\bob\AppData\Roaming", r"C:\Users\bob");
        let ours = Paths::new(r"C:\Users\alice\AppData\Roaming", r"C:\Users\alice");
        let work = new_profile(&theirs, "Work", &[]);
        let merged = merge(&ours, &[default_profile(&ours)], vec![work.clone()]);
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged[1].user_data_dir,
            Some(ours.profile_user_data_dir("work"))
        );
        assert_eq!(
            merged[1].claude_code.config_dir,
            ours.profile_code_config_dir("work")
        );
    }

    #[test]
    fn merge_replaces_same_id_and_dedupes_slug() {
        let paths = Paths::new(r"C:\r", r"C:\h");
        let existing = vec![default_profile(&paths), new_profile(&paths, "Work", &[])];
        let mut renamed = existing[1].clone();
        renamed.name = "Work (renamed)".into();
        let other_work = new_profile(&paths, "Work", &[]);
        let merged = merge(&paths, &existing, vec![renamed, other_work]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[1].name, "Work (renamed)");
        assert_eq!(merged[2].slug, "work-2");
    }
}
