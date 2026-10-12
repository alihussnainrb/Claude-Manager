//! Reading which account a Claude Desktop data folder is signed into.
//!
//! Only `lastKnownAccountUuid` is read from `config.json`. Token caches in the same file are
//! never touched.

use std::path::Path;

use crate::profile::{Profile, ProfileId};

const CONFIG_FILE: &str = "config.json";
const ACCOUNT_KEY: &str = "lastKnownAccountUuid";

/// The account UUID recorded in `<user_data_dir>\config.json`, if any.
pub fn read_account_uuid(user_data_dir: &Path) -> Option<String> {
    let bytes = std::fs::read(user_data_dir.join(CONFIG_FILE)).ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    value
        .get(ACCOUNT_KEY)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Groups of profiles that share a signed-in account (each group has 2+ members).
pub fn duplicate_accounts(profiles: &[Profile]) -> Vec<Vec<ProfileId>> {
    let mut groups: Vec<(String, Vec<ProfileId>)> = Vec::new();
    for p in profiles {
        let Some(uuid) = &p.cached_account_uuid else {
            continue;
        };
        match groups.iter_mut().find(|(u, _)| u == uuid) {
            Some((_, ids)) => ids.push(p.id),
            None => groups.push((uuid.clone(), vec![p.id])),
        }
    }
    groups
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(_, ids)| ids)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::store::{default_profile, new_profile};

    #[test]
    fn reads_only_account() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(CONFIG_FILE),
            r#"{"oauth:tokenCache":"secret","lastKnownAccountUuid":"abc-123"}"#,
        )
        .unwrap();
        assert_eq!(read_account_uuid(dir.path()).as_deref(), Some("abc-123"));
    }

    #[test]
    fn missing_or_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_account_uuid(dir.path()), None);
        std::fs::write(
            dir.path().join(CONFIG_FILE),
            r#"{"lastKnownAccountUuid":""}"#,
        )
        .unwrap();
        assert_eq!(read_account_uuid(dir.path()), None);
    }

    #[test]
    fn finds_duplicates() {
        let paths = Paths::new(r"C:\r", r"C:\h");
        let mut a = default_profile(&paths);
        let mut b = new_profile(&paths, "B", &[a.clone()]);
        let mut c = new_profile(&paths, "C", &[a.clone(), b.clone()]);
        a.cached_account_uuid = Some("x".into());
        b.cached_account_uuid = Some("y".into());
        c.cached_account_uuid = Some("x".into());
        assert_eq!(
            duplicate_accounts(&[a.clone(), b, c.clone()]),
            vec![vec![a.id, c.id]]
        );
    }
}
