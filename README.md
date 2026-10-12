# Claude Manager

Run several Claude Desktop accounts side by side on Windows: work, personal, clients. Each profile is the official Claude Desktop app with its own sign-in, chats, settings and connectors. Switch between them from the tray, a global shortcut or the quick switcher.

Free and open source (MIT). Not affiliated with Anthropic.

## Features

- **Profiles**: each one gets its own Claude data folder (`%APPDATA%\Claude-<name>`), so accounts never mix. Your existing Claude shows up as **Default**.
- **Sign-in routing**: Claude Manager takes over `claude://` links and sends each browser sign-in (and connector sign-in) to the profile that asked for it. When that's unclear, it asks.
- **Quick switcher** (`Ctrl + \``), per-profile global shortcuts, and a tray menu.
- **Claude Code per profile**: a separate `CLAUDE_CONFIG_DIR`, a one-click terminal, an MCP server editor, and copying your main setup without your login.
- **Crash restart**, a warning when two profiles share an account, adopting Claude windows started outside the manager, start with Windows, import and export.

## Requirements

- Windows 10 (19041+) or 11.
- Claude Desktop from the Microsoft Store or claude.com.

## Good to know

- Cowork workspaces need the profile's folder directly inside `%APPDATA%` (the default), and only one profile at a time can run Cowork. This is a Windows limit.
- If a sign-in still lands in the wrong profile, choose "Continue with email" on the Claude sign-in page and type the 6-digit code.
- To give `claude://` back to Claude: Settings → Sign-in routing → Restore.

## Building

Rust (stable, MSVC) and the Visual Studio Build Tools with the Windows SDK.

```bash
cargo build --release -p cm-app
```

The binary is `target/release/claude-manager.exe`.

```bash
cargo test --workspace
```

### Layout

| Crate | What it does |
|---|---|
| `crates/cm-core` | Platform-independent model: profiles, storage, sign-in routing policy, Claude Code setup copying, MCP config, import/export. |
| `crates/cm-win` | Win32/WinRT wrappers: finding the Store install, processes, windows, registry, single instance, message thread. |
| `crates/cm-app` | The GPUI app: model, scanner, tray and hotkeys, sign-in broker, and the UI. |

Debug builds accept `--dev-view code|data|dialog|settings|switcher|router` and `CM_THEME=light|dark` for checking screens.

## Credits

UI built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) and [GPUI Kit](https://github.com/longbridge/gpui-kit). Font: [Figtree](https://github.com/erikdkennedy/figtree), under the SIL Open Font License (see `assets/fonts`). Icons: [Lucide](https://lucide.dev).
