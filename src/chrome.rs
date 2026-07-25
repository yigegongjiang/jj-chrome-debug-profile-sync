//! 选定活跃 profile + rsync 单 profile(热同步, 日常 Chrome 不退出) + 以 CDP 端口启动 debug Chrome。

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
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

/// SQLite 数据库文件头(前 16 字节),用于识别副本内的库,免维护随 Chrome 版本变化的文件名白名单。
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
/// rsync 退出码 24 = 源侧文件在传输中消失。日常 Chrome 不退出 → 缓存/临时文件必然出现增删,不算失败。
/// 其余非 0(含 23 partial transfer)仍视为错误:那是权限 / IO 问题,静默放过会产出不完整副本。
const RSYNC_VANISHED: i32 = 24;

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

/// 只取 stdout(失败→空串), stderr 丢弃。
fn silent_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// 匹配"绑定副本目录的 Chrome"的命令行片段。不带前导 `--`:pkill / pgrep 会把以 `-` 开头的 pattern 当选项。
fn debug_chrome_pattern() -> String {
    format!("user-data-dir={}", DST.display())
}

fn debug_chrome_running() -> bool {
    silent_status("pgrep", &["-f", &debug_chrome_pattern()])
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

/// 运行中日常 Chrome 实际打开着的 profile 目录名(取自 lsof 句柄路径)。
/// Local State 的 last_used 是延迟落盘的:刚切过 profile 立刻运行会读到旧值 → 同步到错的 profile 并整体重建副本。
/// 唯一活跃 profile 时它比 last_used 可信;多个(旧窗口未关)时无法判定"当前",交回 last_used。
fn active_profiles() -> Vec<String> {
    let prefix = format!("{}/", SRC.display());
    let mut found: Vec<String> = Vec::new();
    // -Fn = 每行一个字段, 路径行以 'n' 开头; -w 抑制警告行。
    for line in silent_output("lsof", &["-w", "-c", "Google Chrome", "-Fn"]).lines() {
        let Some(path) = line.strip_prefix('n') else {
            continue;
        };
        // 副本目录的句柄(debug 实例)不以源目录为前缀,自然被排除。
        let Some(rest) = path.strip_prefix(&prefix) else {
            continue;
        };
        let name = rest.split('/').next().unwrap_or("");
        if (name == DEFAULT_PROFILE || is_numbered_profile(name))
            && !found.iter().any(|p| p == name)
        {
            found.push(name.to_string());
        }
    }
    found
}

/// 选定要同步的 profile:优先运行中 Chrome 唯一活跃的那个,否则源 Local State 的 profile.last_used。
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

    // 唯一活跃 profile 优先(覆盖可能陈旧的 last_used);
    // 否则 last_used;last_used 缺失或指向已删除的 profile → 回退 Default(Chrome 必然存在的首个 profile)。
    let active = active_profiles();
    let target = match active.as_slice() {
        [only] if known.contains(only) => only.clone(),
        _ if known.contains(&last_used) => last_used,
        _ => DEFAULT_PROFILE.to_string(),
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

/// 只终止上一轮的 debug 实例(按 `--user-data-dir=DST` 精确匹配),日常 Chrome 全程不动。
/// 必须终止:旧实例持有副本目录的单例锁(新实例只会唤起旧窗口,拿不到 CDP 端口),且会与 rsync --delete 抢写。
/// 先 SIGTERM 走 Chrome 自身退出流程(flush 副本内数据),超时才 -9。
fn quit_debug_chrome() -> Result<(), String> {
    let pattern = debug_chrome_pattern();
    if debug_chrome_running() {
        println!("⏳ Quitting previous debug Chrome…");
        silent_status("pkill", &["-f", &pattern]);
        let deadline = Instant::now() + Duration::from_secs(12);
        while debug_chrome_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        if debug_chrome_running() {
            silent_status("pkill", &["-9", "-f", &pattern]);
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    // 兜底:端口仍被占用说明另有进程占着 CDP 端口(不是本工具的实例),拒绝继续。
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

/// 日常 Chrome 运行期间源 `Preferences` 里 `profile.exit_type` 恒为 "Crashed"(只在正常退出时写回 "Normal"),
/// 副本照搬 → debug Chrome 每次启动都弹 "Restore pages? Chrome didn't shut down correctly"。
/// 副本侧改回 "Normal" 即可(该键不在 Secure Preferences 的 protection.macs 中,不需重算 MAC);
/// `profile.exited_cleanly` 是已废弃字段(Chrome 正常退出也留 false),不动。
fn clear_crash_flags(target: &str) -> Result<(), String> {
    let path = DST.join(target).join("Preferences");
    if !path.exists() {
        return Ok(());
    }
    let Some(mut data) = read_json(&path) else {
        eprintln!(
            "⚠️ Cannot parse {}; crash restore prompt may appear",
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
    profile.insert("exit_type".to_string(), Value::from("Normal"));
    write_json(&path, &data)
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

fn is_sqlite(path: &Path) -> bool {
    let mut buf = [0u8; 16];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok()
        && &buf == SQLITE_MAGIC
}

/// 同目录同名 + 后缀(`Cookies` → `Cookies-journal`),不是扩展名替换。
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.file_name().unwrap_or_default());
    name.push(suffix);
    path.with_file_name(name)
}

/// APFS clonefile:元数据级 COW,单文件原子且不额外占空间;`-p` 保留 mtime/size(不破坏下一轮 rsync 增量判定)。
fn clone_file(src: &Path, dst: &Path) -> bool {
    silent_status(
        "cp",
        &["-pc", &src.to_string_lossy(), &dst.to_string_lossy()],
    )
}

/// 重取副本内 SQLite 库的一致快照。
///
/// rsync 是流式读:数 GB 副本的拷贝窗口内源库若提交写事务,单个库文件会读到跨事务的半新半旧页面(损坏)。
/// Chrome 还对 History / Web Data 等库开 exclusive locking,外部进程连读锁都拿不到(`VACUUM INTO` 直接 SQLITE_BUSY),
/// 只能走文件级拷贝 → 用 clonefile 逐库覆盖,把"文件内撕裂"收敛成"单文件某一时刻的完整内容"。
///
/// 顺序:先 clone `-journal` / `-wal` 再 clone 主库。反序遇到并发提交会得到"新库 + 已清空 journal"(无从恢复);
/// 此序最坏是"旧 journal + 新库" → SQLite 回滚,丢一个事务但库自洽。`-shm` 是派生索引,删掉让 SQLite 重建
/// (陈旧 -shm 配新 -wal 是已知的损坏来源)。
///
/// 遍历目标侧而非源侧:目标已由 rsync 的排除表过滤,不必在此复述排除规则,也不会把已排除内容重新拉回副本。
fn resnapshot_sqlite_dbs() {
    let mut dirs = vec![DST.clone()];
    let mut failed = 0usize;
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                dirs.push(path);
                continue;
            }
            if !file_type.is_file() || !is_sqlite(&path) {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&*DST) else {
                continue;
            };
            let src = SRC.join(rel);
            if !src.exists() {
                continue;
            }
            let _ = fs::remove_file(with_suffix(&path, "-shm"));
            for suffix in ["-journal", "-wal", ""] {
                let src_file = with_suffix(&src, suffix);
                if !suffix.is_empty() && !src_file.exists() {
                    continue;
                }
                if !clone_file(&src_file, &with_suffix(&path, suffix)) {
                    failed += 1;
                }
            }
        }
    }
    // clone 失败(例如目标落到非 APFS 卷)不致命:rsync 的拷贝仍在,只是一致性回落到流式读的水平。
    if failed > 0 {
        eprintln!("⚠️ {failed} database file(s) could not be re-snapshotted; using rsync copy");
    }
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
    if !status.success() && status.code() != Some(RSYNC_VANISHED) {
        return Err(format!(
            "rsync failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }

    resnapshot_sqlite_dbs();
    for f in RUNTIME_FILES {
        let _ = fs::remove_file(DST.join(f));
    }
    prune_local_state(&selection.target)?;
    strip_migrated_extensions(&selection.target)?;
    clear_crash_flags(&selection.target)?;
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

/// 用原始 user-data-dir 启动日常 Chrome。无参数同步已不再退出日常 Chrome,此命令仅用于日常实例确实不在时
/// (或此前被手动退出)显式拉起,与 debug 副本并存。
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
    // 日常 Chrome 不退出(热同步): 一致性由 resnapshot_sqlite_dbs() 的 clonefile 快照兜住,
    // last_used 的延迟落盘由 active_profiles() 兜住。只终止上一轮的 debug 实例。
    quit_debug_chrome()?;
    let selection = select_profile()?;
    reset_if_profile_changed(&selection.target)?;
    sync_profile(&selection)?;
    launch_chrome(&selection.target)?;
    wait_for_cdp(&selection.target);
    Ok(())
}
