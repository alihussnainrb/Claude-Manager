//! `mcpServers` handling for Claude Desktop (`claude_desktop_config.json`) and Claude Code
//! (`<config dir>\.claude.json`). Both files keep servers under a top-level `mcpServers` object;
//! every other key in those files is preserved untouched.

use std::path::Path;

use serde_json::{Map, Value};

use crate::store::{StoreError, read_json, write_json_atomic};

pub const KEY: &str = "mcpServers";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpIssue {
    /// Server name, or empty for problems with the document itself.
    pub server: String,
    pub message: String,
}

/// Parse editor text holding the `mcpServers` object and report every problem found.
pub fn parse_servers(text: &str) -> Result<Map<String, Value>, Vec<McpIssue>> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        vec![McpIssue {
            server: String::new(),
            message: format!("Invalid JSON: {e}"),
        }]
    })?;
    let Value::Object(servers) = value else {
        return Err(vec![issue(
            "",
            "Expected a JSON object of the form { \"name\": { ... } }",
        )]);
    };
    let issues: Vec<_> = servers
        .iter()
        .flat_map(|(name, v)| validate_server(name, v))
        .collect();
    if issues.is_empty() {
        Ok(servers)
    } else {
        Err(issues)
    }
}

fn validate_server(name: &str, v: &Value) -> Vec<McpIssue> {
    let mut out = Vec::new();
    if name.trim().is_empty() {
        out.push(issue(name, "Server name must not be empty"));
    }
    let Value::Object(obj) = v else {
        out.push(issue(name, "Server entry must be an object"));
        return out;
    };
    let kind = obj.get("type").and_then(Value::as_str);
    let has_command = obj
        .get("command")
        .is_some_and(|c| c.as_str().is_some_and(|s| !s.trim().is_empty()));
    let has_url = obj
        .get("url")
        .is_some_and(|c| c.as_str().is_some_and(|s| !s.trim().is_empty()));

    match kind {
        None | Some("stdio") => {
            if !has_command && !(kind.is_none() && has_url) {
                out.push(issue(
                    name,
                    "Needs a non-empty \"command\" (stdio) or a \"url\" (remote)",
                ));
            }
        }
        Some("http" | "sse" | "streamable-http") => {
            if !has_url {
                out.push(issue(name, "Remote servers need a non-empty \"url\""));
            }
        }
        Some(other) => out.push(issue(name, &format!("Unknown \"type\": {other:?}"))),
    }
    if let Some(u) = obj.get("url").and_then(Value::as_str)
        && has_url
        && !(u.starts_with("http://") || u.starts_with("https://"))
    {
        out.push(issue(name, "\"url\" must start with http:// or https://"));
    }
    if let Some(args) = obj.get("args")
        && !args
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        out.push(issue(name, "\"args\" must be an array of strings"));
    }
    for key in ["env", "headers"] {
        if let Some(env) = obj.get(key)
            && !env
                .as_object()
                .is_some_and(|m| m.values().all(Value::is_string))
        {
            out.push(issue(
                name,
                &format!("\"{key}\" must be an object of string values"),
            ));
        }
    }
    out
}

fn issue(server: &str, message: &str) -> McpIssue {
    McpIssue {
        server: server.to_string(),
        message: message.to_string(),
    }
}

/// The `mcpServers` object of a config file (empty when the file or key is missing).
pub fn read_servers(file: &Path) -> Result<Map<String, Value>, StoreError> {
    let doc: Option<Value> = read_json(file)?;
    Ok(doc
        .and_then(|d| d.get(KEY).and_then(Value::as_object).cloned())
        .unwrap_or_default())
}

/// Replace the `mcpServers` object in `file`, keeping every other key. Writes `<file>.bak` first.
pub fn write_servers(file: &Path, servers: Map<String, Value>) -> Result<(), StoreError> {
    let mut doc = match read_json::<Value>(file)? {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    if file.exists() {
        let bak = file.with_extension("json.bak");
        std::fs::copy(file, &bak).map_err(|source| StoreError::Io { path: bak, source })?;
    }
    doc.insert(KEY.into(), Value::Object(servers));
    write_json_atomic(file, &Value::Object(doc))
}

/// Copy servers from `src` into `dst` without overwriting entries `dst` already has.
/// Returns the names that were added.
pub fn merge_servers(src: &Path, dst: &Path) -> Result<Vec<String>, StoreError> {
    let from = read_servers(src)?;
    if from.is_empty() {
        return Ok(Vec::new());
    }
    let mut to = read_servers(dst)?;
    let mut added = Vec::new();
    for (name, server) in from {
        if !to.contains_key(&name) {
            to.insert(name.clone(), server);
            added.push(name);
        }
    }
    if !added.is_empty() {
        write_servers(dst, to)?;
    }
    Ok(added)
}

/// Pretty JSON for the editor.
pub fn to_editor_text(servers: &Map<String, Value>) -> String {
    serde_json::to_string_pretty(&Value::Object(servers.clone())).expect("serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_servers() {
        let ok = parse_servers(
            r#"{
              "fs": {"command": "npx", "args": ["-y", "server-fs"], "env": {"A": "1"}},
              "remote": {"type": "http", "url": "https://x.dev/mcp", "headers": {"Authorization": "Bearer t"}},
              "legacy-remote": {"url": "https://y.dev/sse"}
            }"#,
        );
        assert_eq!(ok.unwrap().len(), 3);
    }

    #[test]
    fn invalid_servers() {
        let errs = parse_servers(
            r#"{
              "a": {"args": ["x"]},
              "b": {"type": "http"},
              "c": {"command": "x", "args": "notarray"},
              "d": {"type": "carrier-pigeon"},
              "e": 5,
              "f": {"type": "sse", "url": "ftp://nope"}
            }"#,
        )
        .unwrap_err();
        let servers: Vec<_> = errs.iter().map(|i| i.server.as_str()).collect();
        assert_eq!(servers, ["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn bad_json() {
        let errs = parse_servers("{").unwrap_err();
        assert!(errs[0].message.starts_with("Invalid JSON"));
        assert!(parse_servers("[]").is_err());
    }

    #[test]
    fn write_preserves_other_keys_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("claude_desktop_config.json");
        std::fs::write(
            &f,
            r#"{"preferences":{"x":1},"mcpServers":{"old":{"command":"a"}}}"#,
        )
        .unwrap();
        let new = parse_servers(r#"{"new":{"command":"b"}}"#).unwrap();
        write_servers(&f, new).unwrap();
        let doc: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert_eq!(doc["preferences"]["x"], 1);
        assert!(doc["mcpServers"].get("old").is_none());
        assert_eq!(doc["mcpServers"]["new"]["command"], "b");
        assert!(f.with_extension("json.bak").exists());
    }

    #[test]
    fn merge_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.json");
        let dst = dir.path().join("dst.json");
        std::fs::write(
            &src,
            r#"{"mcpServers":{"a":{"command":"src-a"},"b":{"command":"src-b"}}}"#,
        )
        .unwrap();
        std::fs::write(&dst, r#"{"mcpServers":{"a":{"command":"dst-a"}}}"#).unwrap();
        assert_eq!(merge_servers(&src, &dst).unwrap(), vec!["b".to_string()]);
        let s = read_servers(&dst).unwrap();
        assert_eq!(s["a"]["command"], "dst-a");
        assert_eq!(s["b"]["command"], "src-b");
    }

    #[test]
    fn merge_into_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.json");
        std::fs::write(&src, r#"{"mcpServers":{"a":{"command":"x"}}}"#).unwrap();
        let dst = dir.path().join("sub").join("dst.json");
        assert_eq!(merge_servers(&src, &dst).unwrap(), vec!["a".to_string()]);
        assert_eq!(read_servers(&dst).unwrap().len(), 1);
    }
}
