//! HTTP 辅助: GitHub Release 资产下载(带进度条) + 小体积 JSON / 文本 GET。

use std::io::{IsTerminal, Read, Write};
use std::time::{Duration, Instant};

use serde_json::Value;

const BAR: usize = 30;
const CHUNK: usize = 64 * 1024;

fn err_string(url: &str, e: ureq::Error) -> String {
    format!("GET {url} -> {e}")
}

fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n}B");
    }
    if n < 1024 * 1024 {
        return format!("{:.1}KB", n as f64 / 1024.0);
    }
    format!("{:.2}MB", n as f64 / 1024.0 / 1024.0)
}

fn render_bar(downloaded: u64, total: u64) -> String {
    if total == 0 {
        return format!("    {} downloaded", format_bytes(downloaded));
    }
    let ratio = (downloaded as f64 / total as f64).min(1.0);
    let filled = (ratio * BAR as f64).floor() as usize;
    let head = if filled < BAR { ">" } else { "" };
    let bar = format!(
        "{}{}{}",
        "=".repeat(filled),
        head,
        " ".repeat(BAR.saturating_sub(filled + 1))
    );
    let pct = format!("{:.1}", ratio * 100.0);
    format!(
        "    [{bar}] {:>5}% {}/{}",
        pct,
        format_bytes(downloaded),
        format_bytes(total)
    )
}

/// TTY 下 `\r` 原地重绘, 限流 100ms; 非 TTY 不输出(收尾由调用方打一行)。
fn render(downloaded: u64, total: u64, is_tty: bool, last: &mut Option<Instant>, force: bool) {
    if !is_tty {
        return;
    }
    let now = Instant::now();
    if !force && last.is_some_and(|t| now.duration_since(t) < Duration::from_millis(100)) {
        return;
    }
    *last = Some(now);
    print!("\r{}", render_bar(downloaded, total));
    let _ = std::io::stdout().flush();
}

/// 拉取小体积文本 (checksums.txt)。非 2xx 视为错误, 与 `fetch(...).ok` 判定一致。
pub fn get_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .call()
        .map_err(|e| err_string(url, e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| err_string(url, e))
}

/// 带超时的 JSON GET (CDP `/json/version` 轮询)。
pub fn get_json(url: &str, timeout: Duration) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();
    let text = agent
        .get(url)
        .call()
        .map_err(|e| err_string(url, e))?
        .body_mut()
        .read_to_string()
        .map_err(|e| err_string(url, e))?;
    serde_json::from_str(&text).map_err(|e| format!("GET {url} -> invalid JSON: {e}"))
}

/// 流式下载并渲染进度条: TTY 下 `\r` 原地刷新(限流 100ms), 非 TTY 只在结束时打一行。
pub fn download_with_progress(url: &str) -> Result<Vec<u8>, String> {
    let mut res = ureq::get(url).call().map_err(|e| err_string(url, e))?;
    let total: u64 = res
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let is_tty = std::io::stdout().is_terminal();
    let mut out: Vec<u8> = Vec::with_capacity(total as usize);
    let mut buf = vec![0u8; CHUNK];
    let mut reader = res.body_mut().as_reader();
    let mut downloaded: u64 = 0;
    let mut last_render: Option<Instant> = None;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("GET {url} -> read failed: {e}"))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        downloaded += n as u64;
        render(downloaded, total, is_tty, &mut last_render, false);
    }
    render(downloaded, total, is_tty, &mut last_render, true);
    if is_tty {
        println!();
    } else {
        println!("{}", render_bar(downloaded, total).trim_start());
    }
    Ok(out)
}
