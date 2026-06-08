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
