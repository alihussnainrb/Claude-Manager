//! Parsing of Claude Desktop process command lines.
//!
//! Electron spawns many helper processes (`--type=renderer`, `--type=gpu-process`, …). The
//! "browser" (main) process is the one without `--type=`, and it carries `--user-data-dir`
//! when we launched it for a profile.

use std::path::PathBuf;

/// Split a Windows command line the way `CommandLineToArgvW` does.
pub fn split_windows_args(cmdline: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut have_arg = false;
    let chars: Vec<char> = cmdline.chars().collect();
    let mut i = 0;

    // argv[0] is special: everything up to the closing quote (or first whitespace), no escapes.
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i < chars.len() {
        if chars[i] == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                cur.push(chars[i]);
                i += 1;
            }
            i += 1;
        } else {
            while i < chars.len() && !chars[i].is_whitespace() {
                cur.push(chars[i]);
                i += 1;
            }
        }
        args.push(std::mem::take(&mut cur));
    }

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                let start = i;
                while i < chars.len() && chars[i] == '\\' {
                    i += 1;
                }
                let n = i - start;
                if i < chars.len() && chars[i] == '"' {
                    cur.extend(std::iter::repeat_n('\\', n / 2));
                    if n % 2 == 1 {
                        cur.push('"');
                        i += 1;
                    }
                } else {
                    cur.extend(std::iter::repeat_n('\\', n));
                }
                have_arg = true;
                continue;
            }
            '"' => {
                if in_quotes && chars.get(i + 1) == Some(&'"') {
                    cur.push('"');
                    i += 1;
                } else {
                    in_quotes = !in_quotes;
                }
                have_arg = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if have_arg {
                    args.push(std::mem::take(&mut cur));
                    have_arg = false;
                }
            }
            c => {
                cur.push(c);
                have_arg = true;
            }
        }
        i += 1;
    }
    if have_arg {
        args.push(cur);
    }
    args
}

/// Quote a single argument so `CommandLineToArgvW` reproduces it exactly.
pub fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            c => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// Build a full command line from an exe path and arguments.
pub fn join_windows_args<'a>(exe: &str, args: impl IntoIterator<Item = &'a str>) -> String {
    let mut s = quote_windows_arg(exe);
    for a in args {
        s.push(' ');
        s.push_str(&quote_windows_arg(a));
    }
    s
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeProcessInfo {
    /// True for the Electron browser process (no `--type=`).
    pub is_main: bool,
    /// `--user-data-dir` value; `None` means the stock folder.
    pub user_data_dir: Option<PathBuf>,
}

pub fn parse_claude_cmdline(cmdline: &str) -> ClaudeProcessInfo {
    let args = split_windows_args(cmdline);
    let mut is_main = true;
    let mut user_data_dir = None;
    let mut iter = args.iter().skip(1).peekable();
    while let Some(a) = iter.next() {
        if a.starts_with("--type=") {
            is_main = false;
        } else if let Some(v) = a.strip_prefix("--user-data-dir=") {
            user_data_dir = Some(PathBuf::from(v));
        } else if a == "--user-data-dir"
            && let Some(v) = iter.next()
        {
            user_data_dir = Some(PathBuf::from(v));
        }
    }
    ClaudeProcessInfo {
        is_main,
        user_data_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_basic() {
        let a = split_windows_args(
            r#""C:\Program Files\Claude\Claude.exe" --user-data-dir="C:\A B\x" foo"#,
        );
        assert_eq!(
            a,
            vec![
                r"C:\Program Files\Claude\Claude.exe",
                r"--user-data-dir=C:\A B\x",
                "foo"
            ]
        );
    }

    #[test]
    fn split_escapes() {
        let a = split_windows_args(r#"x.exe "a\"b" c\\d "e\\" f"#);
        assert_eq!(a, vec!["x.exe", "a\"b", r"c\\d", r"e\", "f"]);
    }

    #[test]
    fn quote_roundtrip() {
        for s in [
            "simple",
            "with space",
            r"trail\",
            r#"q"uote"#,
            r"C:\A B\",
            "",
        ] {
            let line = format!("x.exe {}", quote_windows_arg(s));
            assert_eq!(split_windows_args(&line)[1], s, "line={line}");
        }
    }

    #[test]
    fn parse_main() {
        let i = parse_claude_cmdline(
            r#""C:\x\Claude.exe" "--user-data-dir=C:\Users\me\AppData\Roaming\Claude-work""#,
        );
        assert!(i.is_main);
        assert_eq!(
            i.user_data_dir,
            Some(PathBuf::from(r"C:\Users\me\AppData\Roaming\Claude-work"))
        );
    }

    #[test]
    fn parse_helper() {
        let i = parse_claude_cmdline(
            r#"C:\x\Claude.exe --type=renderer --user-data-dir=C:\d --lang=en"#,
        );
        assert!(!i.is_main);
    }

    #[test]
    fn parse_default() {
        let i = parse_claude_cmdline(r#""C:\x\Claude.exe""#);
        assert!(i.is_main);
        assert_eq!(i.user_data_dir, None);
    }
}
