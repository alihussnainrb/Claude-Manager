//! Folder-safe slugs derived from profile names.

const MAX_LEN: usize = 32;

/// Names Windows refuses as path components, regardless of extension.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Lowercase ASCII letters/digits separated by single dashes. Never empty.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
        if out.len() >= MAX_LEN {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() || RESERVED.contains(&out.as_str()) {
        format!(
            "profile{}",
            if out.is_empty() {
                String::new()
            } else {
                format!("-{out}")
            }
        )
    } else {
        out
    }
}

/// `slugify(name)`, suffixed with `-2`, `-3`, … until `taken` returns false.
pub fn unique_slug(name: &str, mut taken: impl FnMut(&str) -> bool) -> String {
    let base = slugify(name);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken(candidate))
        .expect("infinite iterator")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        assert_eq!(slugify("Work"), "work");
        assert_eq!(slugify("  Client: ACME Corp!! "), "client-acme-corp");
        assert_eq!(slugify("Ünïcode ✨ name"), "n-code-name");
    }

    #[test]
    fn empty_and_reserved() {
        assert_eq!(slugify("✨✨"), "profile");
        assert_eq!(slugify("CON"), "profile-con");
    }

    #[test]
    fn truncates() {
        let s = slugify(&"a".repeat(100));
        assert_eq!(s.len(), MAX_LEN);
    }

    #[test]
    fn unique() {
        let taken = ["work", "work-2"];
        assert_eq!(unique_slug("Work", |s| taken.contains(&s)), "work-3");
        assert_eq!(unique_slug("Home", |s| taken.contains(&s)), "home");
    }
}
