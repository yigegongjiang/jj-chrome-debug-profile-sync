//! 选定 last_used profile + 退出运行中 Chrome + rsync 单 profile + 以 CDP 端口启动 debug Chrome。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::net;

const CHROME_BIN: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
/// 日常(源)User Data 根目录
pub static SRC: LazyLock<PathBuf> =
    LazyLock::new(|| home().join("Library/Application Support/Google/Chrome"));
/// 独立调试副本
pub static DST: LazyLock<PathBuf> =
    LazyLock::new(|| home().join(".config/jj-chrome-debug-profile-sync"));
pub const PORT: &str = "9222";

// 上轮同步的 profile 目录名。放 DST 内(删 DST 即重置),故 rsync 需排除自身,否则被 --delete 清掉。
const STATE_FILE: &str = ".synced-profile";
const DEFAULT_PROFILE: &str = "Default";

// 根级(User Data 根目录下)排除项:体积大(端侧模型 4G+)且对调试无价值,不从源迁移。
// 目标侧不清理:debug Chrome 运行中若自行下载,归其自身管理(切换 profile 会整体重建,不会跨 profile 累积)。
#[rustfmt::skip]
const ROOT_EXCLUDES: &[&str] = &[
    "Crashpad", "BrowserMetrics", // 崩溃 / 指标
    "OptGuideOnDeviceModel", "OptGuideOnDeviceClassifierModel", // 端侧 AI 模型
    "OnDeviceHeadSuggestModel", "optimization_guide_model_store", "WasmTtsEngine",
    "component_crx_cache", "extensions_crx_cache", // 组件 / 扩展 crx 缓存
    "GraphiteDawnCache", "GrShaderCache", "GPUPersistentCache", "ShaderCache", // GPU / Shader
];

// 任意层级(主要在 profile 内)排除项。不做目标侧清理:debug Chrome 自装的扩展与自建缓存应当持久,由 Chrome 自管上限。
// 登录态、书签、站点数据(Cookies、Login Data、IndexedDB、Local Storage)均保留;原始 profile 的扩展不迁移。
#[rustfmt::skip]
const ANY_EXCLUDES: &[&str] = &[
    // 原始扩展本体 / 状态不迁移(debug chrome 仍可自行安装扩展)
    "Extensions/", "Extension State/", "Extension Rules/", "Extension Scripts/",
    "Local Extension Settings/", "Sync Extension Settings/", "Managed Extension Settings/",
    // 通用缓存
    "Cache/", "Code Cache/", "GPUCache/", "DawnWebGPUCache/", "CacheStorage/", "ScriptCache/",
];

// 单例锁 / 运行态文件:源侧不同步;目标侧上轮残留必须显式清除(--exclude 会令 --delete 跳过),否则新实例误判"已在运行"。
#[rustfmt::skip]
const RUNTIME_FILES: &[&str] = &[
    "SingletonLock", "SingletonSocket", "SingletonCookie",
    "DevToolsActivePort", "RunningChromeVersion", "lockfile",
];

fn home() -> PathBuf {
    // 与 libuv 的 homedir() 一致:优先 $HOME。缺失时留空,由 preflight 报"源目录不存在"。
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// 等价 `access(path, X_OK)` 的近似:存在且是文件且带执行位。
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn is_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// 只取退出码, stdio 全丢弃(pgrep / lsof / osascript / pkill 的输出不该出现在终端)。
fn silent_status(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 宽容读 JSON: 非法 UTF-8 走 lossy 而非直接失败(Chrome pref 里是任意用户字符串)。
fn read_json(path: &Path) -> Option<Value> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_str(&String::from_utf8_lossy(&bytes)).ok()
}

fn write_json(path: &Path, data: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(data)
        .map_err(|e| format!("failed to serialize {}: {e}", path.display()))?;
    fs::write(path, bytes).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

fn chrome_running() -> bool {
    silent_status("pgrep", &["-x", "Google Chrome"])
}

fn port_in_use() -> bool {
    silent_status("lsof", &["-nP", &format!("-iTCP:{PORT}"), "-sTCP:LISTEN"])
}

/// 启动前置校验:可执行文件、源目录、固定目标目录。
fn preflight() -> Result<(), String> {
    if !is_executable(Path::new(CHROME_BIN)) {
        return Err(format!("Chrome not found: {CHROME_BIN}"));
    }
    if !is_dir(&SRC) {
        return Err(format!("Source profile not found: {}", SRC.display()));
    }
    if *DST == *SRC {
        return Err("Destination must not equal source".to_string());
    }
    Ok(())
}

struct ProfileSelection {
    target: String,
    /// 其余 profile 目录名,用于 rsync 排除(单 profile 副本,省去其余 profile 的体积)。
    others: Vec<String>,
}

fn is_numbered_profile(name: &str) -> bool {
    name.strip_prefix("Profile ")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// 选定要同步的 profile:源 Local State 的 profile.last_used = 日常 Chrome 最后使用的那个。
fn select_profile() -> Result<ProfileSelection, String> {
    let state = read_json(&SRC.join("Local State"));
    let profile = state.as_ref().and_then(|v| v.get("profile"));
    let mut known: Vec<String> = profile
        .and_then(|p| p.get("info_cache"))
        .and_then(|c| c.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let last_used = profile
        .and_then(|p| p.get("last_used"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Local State 缺失 / 损坏时回退扫目录,避免 others 为空导致其余 profile 被一并同步。
    if known.is_empty() {
        if let Ok(entries) = fs::read_dir(&*SRC) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if dir && (name == DEFAULT_PROFILE || is_numbered_profile(&name)) {
                    known.push(name);
                }
            }
        }
    }

    // last_used 缺失或指向已删除的 profile → 回退 Default(Chrome 必然存在的首个 profile)。
    let target = if known.contains(&last_used) {
        last_used
    } else {
        DEFAULT_PROFILE.to_string()
    };
    let target_dir = SRC.join(&target);
    if !is_dir(&target_dir) {
        return Err(format!(
            "Profile directory not found: {}",
            target_dir.display()
        ));
    }

    // Guest Profile 不在 info_cache 中,但会占体积且对调试无意义,一并排除。
    let mut others: Vec<String> = Vec::new();
    for p in known
        .into_iter()
        .chain(std::iter::once("Guest Profile".to_string()))
    {
        if p != target && !others.contains(&p) {
            others.push(p);
        }
    }
    Ok(ProfileSelection { target, others })
}

/// 目标 profile 与上轮不一致(或首次 / 状态缺失)→ 整个副本删除重建。
/// 沿用旧副本会残留另一个 profile 的目录与 Local State 记录,增量同步无法收敛。
fn reset_if_profile_changed(target: &str) -> Result<(), String> {
    if *DST == *SRC {
        // 删除前复核,防止误删日常 profile
        return Err("Destination must not equal source".to_string());
    }
    let previous = fs::read_to_string(DST.join(STATE_FILE))
        .map(|t| t.trim().to_string())
        .unwrap_or_default();
    if previous == target {
        return Ok(());
    }
    if !is_dir(&DST) {
        return Ok(());
    }
    let shown = if previous.is_empty() {
        "unknown"
    } else {
        &previous
    };
    println!("⏳ Profile changed ({shown} → {target}); rebuilding copy from scratch…");
    fs::remove_dir_all(&*DST).map_err(|e| format!("failed to remove {}: {e}", DST.display()))
}

/// 关闭所有 Chrome:先优雅退出让其 flush SQLite(WAL)得到一致快照,再兜底强杀残留。
fn quit_chrome() -> Result<(), String> {
    if chrome_running() {
        println!("⏳ Quitting Chrome…");
        silent_status("osascript", &["-e", "quit app \"Google Chrome\""]);
        let deadline = Instant::now() + Duration::from_secs(12);
        while chrome_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        silent_status("pkill", &["-9", "-x", "Google Chrome"]);
        silent_status("pkill", &["-9", "-f", "Google Chrome Helper"]);
        std::thread::sleep(Duration::from_secs(1));
    }
    // 兜底:端口仍被占用说明有残留调试实例,拒绝继续。
    if port_in_use() {
        return Err(format!("Port {PORT} still in use; check and retry"));
    }
    Ok(())
}

/// Preferences / Secure Preferences 里记录着已安装扩展的 id + Web Store update_url;
/// 仅排除 Extensions/ 目录不够 —— Chrome 发现"配置里登记了扩展但本地文件缺失"会用 update_url 静默重新下载安装。
/// 故这两个文件都要清掉 extensions 记录(Secure Preferences 同时清对应的防篡改 MAC,否则触发"设置被篡改"提示)。
fn strip_migrated_extensions(target: &str) -> Result<(), String> {
    for file in ["Preferences", "Secure Preferences"] {
        let path = DST.join(target).join(file);
        if !path.exists() {
            continue;
        }
        let Some(mut data) = read_json(&path) else {
            // 静默跳过会让"原扩展又回来了"无从排查,故显式告警。
            eprintln!(
                "⚠️ Cannot parse {}; extension records left untouched",
                path.display()
            );
            continue;
        };
        let Some(root) = data.as_object_mut() else {
            continue;
        };
        root.remove("extensions");
        if let Some(macs) = root
            .get_mut("protection")
            .and_then(|p| p.get_mut("macs"))
            .and_then(|m| m.as_object_mut())
        {
            macs.remove("extensions");
        }
        write_json(&path, &data)?;
    }
    Ok(())
}

/// 副本里只有 target 一个 profile,但 Local State 仍登记着全部 profile。
/// 不裁剪的话 debug Chrome 的头像菜单会列出未同步的 profile,点击即新建空目录(污染副本且 --delete 清不掉)。
fn prune_local_state(target: &str) -> Result<(), String> {
    let path = DST.join("Local State");
    let Some(mut data) = read_json(&path) else {
        eprintln!(
            "⚠️ Cannot parse {}; profile list left untouched",
            path.display()
        );
        return Ok(());
    };
    let Some(profile) = data
        .as_object_mut()
        .and_then(|root| root.get_mut("profile"))
        .and_then(|p| p.as_object_mut())
    else {
        return Ok(());
    };
    if let Some(cache) = profile.get("info_cache").and_then(|c| c.as_object()) {
        let mut kept = Map::new();
        if let Some(entry) = cache.get(target) {
            kept.insert(target.to_string(), entry.clone());
        }
        profile.insert("info_cache".to_string(), Value::Object(kept));
    }
    profile.insert("profiles_order".to_string(), Value::from(vec![target]));
    profile.insert("last_used".to_string(), Value::from(target));
    profile.insert(
        "last_active_profiles".to_string(),
        Value::from(vec![target]),
    );
    write_json(&path, &data)
}

/// 镜像同步:--delete 让目标向源对齐(等价“清理旧副本 + 拷新”,但增量、快)。
/// 只同步 target 一个 profile 的目录 + 根级共享数据(Local State 等),其余 profile 排除。
fn sync_profile(selection: &ProfileSelection) -> Result<(), String> {
    println!(
        "⏳ Syncing profile \"{}\": {} → {} …",
        selection.target,
        SRC.display(),
        DST.display()
    );
    fs::create_dir_all(&*DST).map_err(|e| format!("failed to create {}: {e}", DST.display()))?;

    let mut excludes: Vec<String> = Vec::new();
    excludes.push(format!("/{STATE_FILE}")); // 本工具状态文件,源侧不存在,不能被 --delete 清掉
    excludes.extend(selection.others.iter().map(|p| format!("/{p}/")));
    excludes.extend(ROOT_EXCLUDES.iter().map(|d| format!("/{d}/")));
    excludes.extend(RUNTIME_FILES.iter().map(|f| format!("/{f}")));
    excludes.push("*.pma".to_string());
    excludes.extend(ANY_EXCLUDES.iter().map(|s| (*s).to_string()));

    let mut cmd = Command::new("rsync");
    cmd.arg("-a").arg("--delete");
    for e in &excludes {
        cmd.arg(format!("--exclude={e}"));
    }
    cmd.arg(format!("{}/", SRC.display()))
        .arg(format!("{}/", DST.display()));
    let status = cmd
        .status()
        .map_err(|e| format!("rsync failed to start: {e}"))?;
    if !status.success() {
        return Err(format!(
            "rsync failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }

    for f in RUNTIME_FILES {
        let _ = fs::remove_file(DST.join(f));
    }
    prune_local_state(&selection.target)?;
    strip_migrated_extensions(&selection.target)?;
    fs::write(DST.join(STATE_FILE), format!("{}\n", selection.target))
        .map_err(|e| format!("failed to write {}: {e}", DST.join(STATE_FILE).display()))
}

/// 后台启动带调试端口的 Chrome。
fn launch_chrome(target: &str) -> Result<(), String> {
    println!(
        "🚀 Launching Chrome (CDP :{PORT}, profile={target}, user-data-dir={}) …",
        DST.display()
    );
    // --remote-debugging-port 要求非默认 user-data-dir(Chrome 136+),故启动于独立副本目录。
    // Local State 随副本带来,自动恢复 chrome://flags 开关,与日常实例等价。
    // --remote-allow-origins=* 放开 CDP WebSocket 的 Origin 校验,便于外部工具连接。
    let child = Command::new(CHROME_BIN)
        .arg(format!("--user-data-dir={}", DST.display()))
        // 显式锁定 profile:副本里只有这一个 profile,不指定则 Chrome 依 Local State 推断,
        // 且 unclean exit 时会连带拉起其它 last_active_profiles(目录已不存在 → 新建空 profile)。
        .arg(format!("--profile-directory={target}"))
        .arg("--remote-debugging-address=0.0.0.0")
        .arg(format!("--remote-debugging-port={PORT}"))
        .arg("--remote-allow-origins=*")
        .arg("--origin-trial-disabled-features=CanvasTextNg|WebAssemblyCustomDescriptors")
        // 关闭 Chrome Sync: 否则登录着 Google 账号的 debug Chrome 会通过云端同步把原 profile 的扩展/书签等静默拉回来。
        // 不影响 cookie / 密码 / 网站登录态 (那些是 profile 本地数据, 不走 sync)。
        .arg("--disable-sync")
        // 关闭 Chrome 出厂默认捆绑应用 (Application Launcher for Drive 等 Google 内置扩展, 走 external_extensions.json 分发)。
        .arg("--disable-default-apps")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to launch Chrome: {e}"))?;
    // 不 wait: 子进程脱离本进程生命周期, CLI 退出后 Chrome 继续运行(等价 & disown)。
    drop(child);
    Ok(())
}

/// 轮询 CDP 端点至就绪(最多 15s),打印外部工具连接所需信息。
fn wait_for_cdp(profile: &str) {
    let url = format!("http://127.0.0.1:{PORT}/json/version");
    print!("⏳ Waiting for CDP");
    let _ = std::io::stdout().flush();
    for _ in 0..30 {
        if let Ok(info) = net::get_json(&url, Duration::from_secs(1)) {
            let field = |k: &str| {
                info.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            println!();
            println!("✅ CDP ready");
            println!("   Browser   : {}", field("Browser"));
            println!("   WebSocket : {}", field("webSocketDebuggerUrl"));
            println!("   Endpoint  : {url}");
            println!("   user-data : {}", DST.display());
            println!("   profile   : {profile}");
            return;
        }
        // CDP 尚未就绪,继续轮询
        print!(".");
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_millis(500));
    }
    println!();
    println!("⚠️ Chrome launched but CDP not detected within 15s; open {url} to check");
}

/// 用原始 user-data-dir 启动日常 Chrome。debug Chrome 运行时,原始 Chrome 因单例锁通常无法直接打开,
/// 此命令通过 `open -na` 显式拉起新实例,与 debug 副本并存。
/// 不指定 --profile-directory:保持日常 Chrome 的原有行为(按 Local State 的 last_used / last_active_profiles 恢复窗口)。
pub fn run_original() -> i32 {
    if !is_executable(Path::new(CHROME_BIN)) {
        eprintln!("❌ Chrome not found: {CHROME_BIN}");
        return 1;
    }
    println!(
        "🚀 Launching original Chrome (user-data-dir={}) …",
        SRC.display()
    );
    // -n 强制新实例; -a 指定 app; --args 后传给 Chrome。显式 --user-data-dir 指向默认目录,
    // 绕过 macOS Launch Services 对已运行实例的复用。
    let status = Command::new("open")
        .args(["-na", "Google Chrome", "--args"])
        .arg(format!("--user-data-dir={}", SRC.display()))
        .status();
    match status {
        Ok(s) if s.success() => 0,
        _ => 1,
    }
}

/// 镜像日常 Chrome profile 到独立目录并以 CDP 调试端口启动,供外部工具(CDP / MCP)连接。
pub fn run() -> i32 {
    match run_inner() {
        Ok(()) => 0,
        Err(msg) => {
            eprintln!("❌ {msg}");
            1
        }
    }
}

fn run_inner() -> Result<(), String> {
    preflight()?;
    // 先退出 Chrome 再选 profile:Local State 是延迟落盘的,运行中读可能拿到切换 profile 前的旧值,
    // 优雅退出会 flush 出最终的 last_used。
    quit_chrome()?;
    let selection = select_profile()?;
    reset_if_profile_changed(&selection.target)?;
    sync_profile(&selection)?;
    launch_chrome(&selection.target)?;
    wait_for_cdp(&selection.target);
    Ok(())
}
