//! Seeding a profile's Claude Code config dir from the stock `~\.claude`.
//!
//! Copies setup (settings, memory, agents, commands, skills, plugins, MCP servers) but never the
//! login, conversation history or caches. Existing files in the destination are never
//! overwritten, so seeding is safe to repeat.

use std::fs;
use std::path::{Path, PathBuf};

use crate::mcp;
use crate::store::StoreError;

/// Top-level entries of `~\.claude` that are copied.
pub const ALLOWLIST: &[&str] = &[
    "settings.json",
    "CLAUDE.md",
    "agents",
    "commands",
    "skills",
    "output-styles",
    "hooks",
    "plugins",
    "keybindings.json",
];

/// Entries that must never be copied even if the allowlist is changed. Checked for every path
/// component during the walk.
pub const DENYLIST: &[&str] = &[
    ".credentials.json",
    "projects",
    "history.jsonl",
    "sessions",
    "todos",
    "statsig",
    "shell-snapshots",
    "file-history",
    "ide",
    "debug",
    "telemetry",
    "cache",
];

/// Text files larger than this are copied verbatim instead of having paths rewritten.
const REWRITE_MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SeedReport {
    pub copied: Vec<PathBuf>,
    pub skipped_existing: Vec<PathBuf>,
    pub mcp_servers_added: Vec<String>,
}

/// `src_dir` = `~\.claude`, `src_state` = `~\.claude.json`, `dst_dir` = profile config dir.
pub fn seed(src_dir: &Path, src_state: &Path, dst_dir: &Path) -> Result<SeedReport, StoreError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| StoreError::Io { path, source }
    };
    let mut report = SeedReport::default();
    fs::create_dir_all(dst_dir).map_err(io(dst_dir))?;

    for name in ALLOWLIST {
        let src = src_dir.join(name);
        if src.exists() {
            copy_tree(&src, &dst_dir.join(name), src_dir, dst_dir, &mut report)?;
        }
    }

    // With CLAUDE_CONFIG_DIR set, Claude Code keeps its global state at `<dir>\.claude.json`.
    report.mcp_servers_added = mcp::merge_servers(src_state, &dst_dir.join(".claude.json"))?;
    Ok(report)
}

fn is_denied(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        DENYLIST.iter().any(|d| s.eq_ignore_ascii_case(d))
    })
}

fn copy_tree(
    src: &Path,
    dst: &Path,
    src_root: &Path,
    dst_root: &Path,
    report: &mut SeedReport,
) -> Result<(), StoreError> {
    let rel = src.strip_prefix(src_root).unwrap_or(src);
    if is_denied(rel) {
        return Ok(());
    }
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| StoreError::Io { path, source }
    };
    let meta = fs::symlink_metadata(src).map_err(io(src))?;
    if meta.file_type().is_symlink() {
        // Don't follow links out of the source tree.
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(dst).map_err(io(dst))?;
        for entry in fs::read_dir(src).map_err(io(src))? {
            let entry = entry.map_err(io(src))?;
            copy_tree(
                &entry.path(),
                &dst.join(entry.file_name()),
                src_root,
                dst_root,
                report,
            )?;
        }
        return Ok(());
    }
    if dst.exists() {
        report.skipped_existing.push(dst.to_path_buf());
        return Ok(());
    }
    let is_text = matches!(
        src.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("json" | "md" | "txt" | "toml" | "yaml" | "yml")
    );
    if is_text && meta.len() <= REWRITE_MAX_BYTES {
        let bytes = fs::read(src).map_err(io(src))?;
        match String::from_utf8(bytes) {
            Ok(text) => {
                let rewritten = rewrite_paths(&text, src_root, dst_root);
                fs::write(dst, rewritten).map_err(io(dst))?;
            }
            Err(e) => fs::write(dst, e.into_bytes()).map_err(io(dst))?,
        }
    } else {
        fs::copy(src, dst).map_err(io(dst))?;
    }
    report.copied.push(dst.to_path_buf());
    Ok(())
}

/// Replace absolute references to `from` with `to`, in the spellings that show up in config
/// files: raw (`C:\x`), JSON-escaped (`C:\\x`) and forward-slash (`C:/x`).
pub fn rewrite_paths(text: &str, from: &Path, to: &Path) -> String {
    let from = from
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    let to = to
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_string();
    if from.is_empty() {
        return text.to_string();
    }
    let variants = [
        (
            from.replace('/', "\\").replace('\\', "\\\\"),
            to.replace('/', "\\").replace('\\', "\\\\"),
        ),
        (from.replace('/', "\\"), to.replace('/', "\\")),
        (from.replace('\\', "/"), to.replace('\\', "/")),
    ];
    let mut out = text.to_string();
    for (f, t) in variants {
        out = replace_path_prefix(&out, &f, &t);
    }
    out
}

/// Case-insensitive replace of `from` only where it is followed by a path boundary, so
/// `C:\Users\me\.claude` doesn't match inside `C:\Users\me\.claude-work`.
fn replace_path_prefix(text: &str, from: &str, to: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle = from.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut search = 0;
    while let Some(pos) = lower[search..].find(&needle) {
        let start = search + pos;
        let end = start + needle.len();
        let boundary = text[end..].chars().next().is_none_or(|c| {
            matches!(
                c,
                '\\' | '/' | '"' | '\'' | ' ' | '\n' | '\r' | ',' | '}' | ']'
            )
        });
        if boundary {
            out.push_str(&text[last..start]);
            out.push_str(to);
            last = end;
        }
        search = end;
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn copies_allowlist_never_credentials() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join(".claude");
        let dst = root.path().join(".claude-work");
        let state = root.path().join(".claude.json");
        write(&src.join("settings.json"), r#"{"model":"opus"}"#);
        write(&src.join("CLAUDE.md"), "# memory");
        write(&src.join("skills/x/SKILL.md"), "skill");
        write(&src.join(".credentials.json"), "SECRET");
        write(&src.join("projects/p/session.jsonl"), "chat");
        write(&src.join("history.jsonl"), "history");
        write(&src.join("plugins/cache/big.bin"), "cached");
        write(
            &state,
            r#"{"oauthAccount":{"x":1},"mcpServers":{"fs":{"command":"npx"}}}"#,
        );

        let report = seed(&src, &state, &dst).unwrap();

        assert!(dst.join("settings.json").exists());
        assert!(dst.join("CLAUDE.md").exists());
        assert!(dst.join("skills/x/SKILL.md").exists());
        assert!(!dst.join(".credentials.json").exists());
        assert!(!dst.join("projects").exists());
        assert!(!dst.join("history.jsonl").exists());
        assert!(
            !dst.join("plugins/cache").exists(),
            "denylisted component inside an allowed dir"
        );
        assert_eq!(report.mcp_servers_added, vec!["fs".to_string()]);

        let new_state = fs::read_to_string(dst.join(".claude.json")).unwrap();
        assert!(
            !new_state.contains("oauthAccount"),
            "only mcpServers is carried over"
        );
    }

    #[test]
    fn idempotent_and_non_destructive() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join(".claude");
        let dst = root.path().join(".claude-work");
        write(&src.join("CLAUDE.md"), "from source");
        write(&dst.join("CLAUDE.md"), "edited in profile");
        let report = seed(&src, &root.path().join("missing.json"), &dst).unwrap();
        assert_eq!(
            fs::read_to_string(dst.join("CLAUDE.md")).unwrap(),
            "edited in profile"
        );
        assert_eq!(report.skipped_existing.len(), 1);
    }

    #[test]
    fn rewrites_paths() {
        let from = Path::new(r"C:\Users\me\.claude");
        let to = Path::new(r"C:\Users\me\.claude-work");
        let text = r#"{"a":"C:\\Users\\me\\.claude\\plugins\\x","b":"c:/users/me/.claude/skills","c":"C:\\Users\\me\\.claude-other\\y","d":"C:\\Users\\me\\.claude"}"#;
        let out = rewrite_paths(text, from, to);
        assert_eq!(
            out,
            r#"{"a":"C:\\Users\\me\\.claude-work\\plugins\\x","b":"C:/Users/me/.claude-work/skills","c":"C:\\Users\\me\\.claude-other\\y","d":"C:\\Users\\me\\.claude-work"}"#
        );
    }
}
