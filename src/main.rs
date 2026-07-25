//! CLI 入口 / 子命令分发 / self-update / uninstall。

#[cfg(not(target_os = "macos"))]
compile_error!("only macOS is supported");

mod chrome;
mod net;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use sha2::{Digest, Sha256};

use chrome::{DST, PORT, SRC};

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cargo 的 repository 是完整 URL, self-update 的 Release URL 只要 `<owner>/<repo>`。
fn repo() -> &'static str {
    env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/")
}

fn usage() -> String {
    format!(
        "Usage: {NAME} [command]

Commands:
  (none)         Sync the last-used Chrome profile and launch a debug Chrome (CDP)
  original       Launch the original Chrome on its own profiles
  update         Download the latest release and replace this binary (alias: upgrade)
  uninstall      Remove this binary from disk

Flags:
  -h, --help     Show this help message
  -v, --version  Show version information"
    )
}

fn detect_asset() -> Result<String, String> {
    // 目标 OS 由 main.rs 顶部的 cfg 断言锁定 macOS, 只需判架构。
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => return Err(format!("unsupported arch: {other}")),
    };
    Ok(format!("{NAME}-darwin-{arch}"))
}

/// self-update / uninstall 只允许作用于安装后的同名二进制, 不能误伤 `cargo run` 的构建产物。
fn installed_binary(action: &str) -> Result<PathBuf, String> {
    let dest =
        std::env::current_exe().map_err(|e| format!("cannot resolve current executable: {e}"))?;
    let base = dest
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if base != NAME {
        return Err(format!(
            "refusing to {action}: current executable is \"{base}\", expected \"{NAME}\". \
             {action} only works on the installed binary, not when running from source via cargo."
        ));
    }
    Ok(dest)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn update() -> Result<i32, String> {
    let asset_name = detect_asset()?;
    let base = format!("https://github.com/{}/releases/latest/download", repo());
    let asset_url = format!("{base}/{asset_name}");
    let checksums_url = format!("{base}/checksums.txt");
    let dest = installed_binary("self-update")?;

    println!("==> Updating {NAME}");
    println!("    repo:   {}", repo());
    println!("    target: {}", dest.display());
    println!("    before: {NAME} {VERSION}");

    println!("==> Downloading {asset_url}");
    let asset_bytes = net::download_with_progress(&asset_url)?;

    // Verify checksum if checksums.txt exists for this release.
    if let Ok(text) = net::get_text(&checksums_url) {
        let suffix = format!(" {asset_name}");
        if let Some(line) = text.lines().find(|l| l.trim().ends_with(&suffix)) {
            let expected = line
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_lowercase();
            let actual = sha256_hex(&asset_bytes);
            if expected != actual {
                eprintln!("error: checksum mismatch (expected {expected}, got {actual})");
                return Ok(1);
            }
            println!("==> Checksum OK");
        }
    }

    // Atomic replace via tmp on the same filesystem.
    let dir = dest
        .parent()
        .ok_or_else(|| format!("cannot resolve parent directory of {}", dest.display()))?;
    fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".{NAME}.update.{}", std::process::id()));
    fs::write(&tmp, &asset_bytes).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("failed to chmod {}: {e}", tmp.display()))?;
    if let Err(e) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("failed to replace {}: {e}", dest.display()));
    }

    println!("==> Updated: {}", dest.display());
    // best-effort; if the new binary cannot exec, the replace itself already succeeded.
    if let Ok(out) = Command::new(&dest).arg("--version").output() {
        if out.status.success() {
            let after = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !after.is_empty() {
                println!("    after:  {after}");
            }
        }
    }
    Ok(0)
}

fn uninstall() -> Result<i32, String> {
    let dest = installed_binary("uninstall")?;

    println!("==> Uninstalling {NAME}");
    println!("    target: {}", dest.display());

    fs::remove_file(&dest).map_err(|e| format!("failed to remove {}: {e}", dest.display()))?;

    println!("==> Removed: {}", dest.display());
    Ok(0)
}

fn dispatch(cmd: Option<&str>) -> i32 {
    match cmd {
        None => chrome::run(),
        Some("original") => chrome::run_original(),
        Some("--help" | "-h") => {
            println!("{}", usage());
            println!(
                "\nProfile paths:\n  source:     {}\n  debug copy: {}\n  CDP:        http://127.0.0.1:{}",
                SRC.display(),
                DST.display(),
                PORT
            );
            0
        }
        Some("--version" | "-v") => {
            println!("{NAME} {VERSION}");
            0
        }
        Some("update" | "upgrade") => match update() {
            Ok(code) => code,
            Err(msg) => {
                eprintln!("error: update failed: {msg}");
                1
            }
        },
        Some("uninstall") => match uninstall() {
            Ok(code) => code,
            Err(msg) => {
                eprintln!("error: uninstall failed: {msg}");
                1
            }
        },
        Some(other) => {
            eprintln!("error: unknown command \"{other}\"\n");
            eprintln!("{}", usage());
            1
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dispatch(args.first().map(String::as_str)));
}
