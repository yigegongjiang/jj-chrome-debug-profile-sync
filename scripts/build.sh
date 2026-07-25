#!/usr/bin/env bash
# build.sh — build both macOS targets into dist/ under the released asset names.
#
# Usage:
#   ./scripts/build.sh              # both arm64 + x64
#   ./scripts/build.sh arm64        # single arch (arm64 | x64)

set -euo pipefail

cd "$(dirname "$0")/.."

err()  { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

command -v cargo >/dev/null 2>&1 || err "cargo is required"
command -v jq >/dev/null 2>&1 || err "jq is required"

# Convention: Cargo package name == binary name == released asset prefix.
BIN_NAME="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')"
[ -n "$BIN_NAME" ] && [ "$BIN_NAME" != "null" ] || err "cannot read package name from Cargo.toml"

declare -a wanted
if [ "$#" -gt 0 ]; then
  wanted=("$@")
else
  wanted=(arm64 x64)
fi

mkdir -p dist

for arch in "${wanted[@]}"; do
  case "$arch" in
    arm64) triple="aarch64-apple-darwin" ;;
    x64)   triple="x86_64-apple-darwin" ;;
    *) err "unsupported arch: ${arch} (expected arm64 | x64)" ;;
  esac

  # Idempotent; skipped when the toolchain is not managed by rustup.
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "$triple" >/dev/null 2>&1
  fi

  info "==> Building ${BIN_NAME}-darwin-${arch} (${triple})"
  cargo build --release --locked --target "$triple"

  artifact="target/${triple}/release/${BIN_NAME}"
  [ -f "$artifact" ] || err "build artifact not found: $artifact"
  out="dist/${BIN_NAME}-darwin-${arch}"
  cp -f "$artifact" "$out"
  chmod +x "$out"
  info "built: ${out}"
done
