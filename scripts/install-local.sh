#!/usr/bin/env bash
# install-local.sh — build from source and install to ~/.local/bin, for local verification.
#
# Usage:
#   ./scripts/install-local.sh
#   INSTALL_DIR=/usr/local/bin ./scripts/install-local.sh

set -euo pipefail

cd "$(dirname "$0")/.."

err()  { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

command -v bun >/dev/null 2>&1 || err "bun is required"

case "$(uname -s)" in
  Darwin) ;;
  *) err "unsupported OS: $(uname -s) (only macOS is supported)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  host_arch="x64" ;;
  aarch64|arm64) host_arch="arm64" ;;
  *) err "unsupported macOS architecture: $(uname -m)" ;;
esac

# Convention: package.json#name == binary name (same as the released asset).
BIN_NAME="${BIN_NAME:-$(bun --print 'require("./package.json").name')}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
artifact="dist/${BIN_NAME}-darwin-${host_arch}"

info "==> Building ${BIN_NAME} (bun build --compile)"
info "    target: ${INSTALL_DIR}/${BIN_NAME}"
bun run build

[ -f "$artifact" ] || err "build artifact not found: $artifact"

mkdir -p "$INSTALL_DIR"
dest="${INSTALL_DIR}/${BIN_NAME}"
tmp_dest="${dest}.tmp.$$"
trap 'rm -f "$tmp_dest"' EXIT
cp -f "$artifact" "$tmp_dest"
chmod +x "$tmp_dest"
# Atomic replace: overwriting in place would fail ("Text file busy") while an old instance runs.
mv -f "$tmp_dest" "$dest"

info "==> Installed: $dest"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    info ""
    info "warning: $INSTALL_DIR is not on your PATH."
    info "add to your shell rc:"
    info "    export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

info "==> Verifying"
# `--version` only: running with no args would sync the profile and launch Chrome.
"$dest" --version
