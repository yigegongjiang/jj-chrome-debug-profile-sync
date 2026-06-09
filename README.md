# jj-chrome-debug-profile-sync

rsync the Chrome user profile to a target dir, then launch a debug-mode Chrome (remote debugging port) for CDP / external tooling. Bun single-file executable, macOS only; `install.sh` to install, `update` to self-update.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-chrome-debug-profile-sync/main/install.sh | bash
```

Installs to `$HOME/.local/bin` by default. Override with `VERSION` / `INSTALL_DIR` / `REPO`.

## Usage

The installed command is `jj-chrome-debug-profile-sync` (same as the repo / binary name). Run it with no arguments to sync the profile and launch debug Chrome; subcommands:

<!-- prettier-ignore -->
| Command | Alias | Description |
|---|---|---|
| `(none)` | — | Sync profile → launch debug Chrome (CDP) |
| `help` | `-h` / `--help` | Usage |
| `version` | `-v` / `--version` | Version |
| `update` | `upgrade` | Self-update (compiled binary only) |
| `uninstall` | — | Uninstall (compiled binary only) |

## With Chrome MCP

Launch debug Chrome via this tool, then point [chrome-devtools-mcp](https://github.com/ChromeDevTools/chrome-devtools-mcp) at `http://127.0.0.1:9222` — AI agents (Claude Code / Codex / etc.) can operate Chrome over CDP without per-action approval prompts.

`.mcp.json` example:

```json
{
  "mcpServers": {
    "chrome-devtools": {
      "command": "npx",
      "args": [
        "chrome-devtools-mcp@latest",
        "--browser-url=http://127.0.0.1:9222"
      ]
    }
  }
}
```
