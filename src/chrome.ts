import { constants } from "node:fs";
import { access, mkdir, readdir, rm, stat, unlink } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

const HOME = homedir();
const CHROME_BIN = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
export const SRC = join(HOME, "Library/Application Support/Google/Chrome"); // 日常(源)User Data 根目录
export const DST = join(HOME, ".cache/chrome-debug-profile-sync"); // 独立调试副本
export const PORT = "9222";

// 上轮同步的 profile 目录名。放 DST 内(删 DST 即重置),故 rsync 需排除自身,否则被 --delete 清掉。
const STATE_FILE = ".synced-profile";
const DEFAULT_PROFILE = "Default";

// 根级(User Data 根目录下)排除项:体积大(端侧模型 4G+)且对调试无价值,不从源迁移。
// 目标侧不清理:debug Chrome 运行中若自行下载,归其自身管理(切换 profile 会整体重建,不会跨 profile 累积)。
const ROOT_EXCLUDES = [
  "Crashpad", "BrowserMetrics", // 崩溃 / 指标
  "OptGuideOnDeviceModel", "OptGuideOnDeviceClassifierModel", // 端侧 AI 模型
  "OnDeviceHeadSuggestModel", "optimization_guide_model_store", "WasmTtsEngine",
  "component_crx_cache", "extensions_crx_cache", // 组件 / 扩展 crx 缓存
  "GraphiteDawnCache", "GrShaderCache", "GPUPersistentCache", "ShaderCache", // GPU / Shader
];

// 任意层级(主要在 profile 内)排除项。不做目标侧清理:debug Chrome 自装的扩展与自建缓存应当持久,由 Chrome 自管上限。
// 登录态、书签、站点数据(Cookies、Login Data、IndexedDB、Local Storage)均保留;原始 profile 的扩展不迁移。
const ANY_EXCLUDES = [
  "Extensions/", "Extension State/", "Extension Rules/", "Extension Scripts/", // 原始扩展本体 / 状态不迁移(debug chrome 仍可自行安装扩展)
  "Local Extension Settings/", "Sync Extension Settings/", "Managed Extension Settings/",
  "Cache/", "Code Cache/", "GPUCache/", "DawnWebGPUCache/", "CacheStorage/", "ScriptCache/", // 通用缓存
];

// 单例锁 / 运行态文件:源侧不同步;目标侧上轮残留必须显式清除(--exclude 会令 --delete 跳过),否则新实例误判"已在运行"。
const RUNTIME_FILES = [
  "SingletonLock", "SingletonSocket", "SingletonCookie",
  "DevToolsActivePort", "RunningChromeVersion", "lockfile",
];

function die(msg: string): never {
  throw new Error(msg);
}

function chromeRunning(): boolean {
  return Bun.spawnSync(["pgrep", "-x", "Google Chrome"]).exitCode === 0;
}

function portInUse(): boolean {
  return Bun.spawnSync(["lsof", "-nP", `-iTCP:${PORT}`, "-sTCP:LISTEN"]).exitCode === 0;
}

// 启动前置校验:可执行文件、源目录、固定目标目录。
async function preflight(): Promise<void> {
  const executable = await access(CHROME_BIN, constants.X_OK).then(() => true, () => false);
  if (!executable) die(`Chrome not found: ${CHROME_BIN}`);

  const srcIsDir = await stat(SRC).then((s) => s.isDirectory(), () => false);
  if (!srcIsDir) die(`Source profile not found: ${SRC}`);

  if (DST === SRC) die("Destination must not equal source");
}

type ProfileSelection = { target: string; others: string[] };

// 选定要同步的 profile:源 Local State 的 profile.last_used = 日常 Chrome 最后使用的那个。
// others = 其余 profile 目录名,用于 rsync 排除(单 profile 副本,省去其余 profile 的体积)。
async function selectProfile(): Promise<ProfileSelection> {
  const state = await Bun.file(join(SRC, "Local State")).json().catch(() => null);
  const profile = (state as { profile?: { last_used?: unknown; info_cache?: unknown } } | null)?.profile;
  const cache = profile?.info_cache;
  let known = cache && typeof cache === "object" ? Object.keys(cache as Record<string, unknown>) : [];
  const lastUsed = typeof profile?.last_used === "string" ? profile.last_used : "";

  // Local State 缺失 / 损坏时回退扫目录,避免 others 为空导致其余 profile 被一并同步。
  if (known.length === 0) {
    const entries = await readdir(SRC, { withFileTypes: true }).catch(() => []);
    known = entries
      .filter((e) => e.isDirectory() && (e.name === DEFAULT_PROFILE || /^Profile \d+$/.test(e.name)))
      .map((e) => e.name);
  }

  // last_used 缺失或指向已删除的 profile → 回退 Default(Chrome 必然存在的首个 profile)。
  const target = known.includes(lastUsed) ? lastUsed : DEFAULT_PROFILE;
  const targetIsDir = await stat(join(SRC, target)).then((s) => s.isDirectory(), () => false);
  if (!targetIsDir) die(`Profile directory not found: ${join(SRC, target)}`);

  // Guest Profile 不在 info_cache 中,但会占体积且对调试无意义,一并排除。
  const others = [...new Set([...known, "Guest Profile"])].filter((p) => p !== target);
  return { target, others };
}

// 目标 profile 与上轮不一致(或首次 / 状态缺失)→ 整个副本删除重建。
// 沿用旧副本会残留另一个 profile 的目录与 Local State 记录,增量同步无法收敛。
async function resetIfProfileChanged(target: string): Promise<void> {
  if (DST === SRC) die("Destination must not equal source"); // 删除前复核,防止误删日常 profile
  const previous = await Bun.file(join(DST, STATE_FILE)).text().then((t) => t.trim(), () => "");
  if (previous === target) return;
  const exists = await stat(DST).then((s) => s.isDirectory(), () => false);
  if (!exists) return;
  console.log(`⏳ Profile changed (${previous || "unknown"} → ${target}); rebuilding copy from scratch…`);
  await rm(DST, { recursive: true, force: true });
}

// 关闭所有 Chrome:先优雅退出让其 flush SQLite(WAL)得到一致快照,再兜底强杀残留。
async function quitChrome(): Promise<void> {
  if (chromeRunning()) {
    console.log("⏳ Quitting Chrome…");
    Bun.spawnSync(["osascript", "-e", 'quit app "Google Chrome"'], { stdout: "ignore", stderr: "ignore" });
    const deadline = Date.now() + 12_000;
    while (chromeRunning() && Date.now() < deadline) {
      await Bun.sleep(500);
    }
    Bun.spawnSync(["pkill", "-9", "-x", "Google Chrome"], { stdout: "ignore", stderr: "ignore" });
    Bun.spawnSync(["pkill", "-9", "-f", "Google Chrome Helper"], { stdout: "ignore", stderr: "ignore" });
    await Bun.sleep(1000);
  }
  // 兜底:端口仍被占用说明有残留调试实例,拒绝继续。
  if (portInUse()) die(`Port ${PORT} still in use; check and retry`);
}

// Preferences / Secure Preferences 里记录着已安装扩展的 id + Web Store update_url;
// 仅排除 Extensions/ 目录不够 —— Chrome 发现"配置里登记了扩展但本地文件缺失"会用 update_url 静默重新下载安装。
// 故这两个文件都要清掉 extensions 记录(Secure Preferences 同时清对应的防篡改 MAC,否则触发"设置被篡改"提示)。
async function stripMigratedExtensions(target: string): Promise<void> {
  for (const file of ["Preferences", "Secure Preferences"]) {
    const path = join(DST, target, file);
    const exists = await access(path, constants.F_OK).then(() => true, () => false);
    if (!exists) continue;
    const data = await Bun.file(path).json().catch(() => null);
    if (!data || typeof data !== "object") continue;
    delete data.extensions;
    delete data.protection?.macs?.extensions;
    await Bun.write(path, JSON.stringify(data));
  }
}

// 副本里只有 target 一个 profile,但 Local State 仍登记着全部 profile。
// 不裁剪的话 debug Chrome 的头像菜单会列出未同步的 profile,点击即新建空目录(污染副本且 --delete 清不掉)。
async function pruneLocalState(target: string): Promise<void> {
  const path = join(DST, "Local State");
  const data = await Bun.file(path).json().catch(() => null);
  if (!data || typeof data !== "object") return;
  const profile = (data as { profile?: Record<string, unknown> }).profile;
  if (!profile) return;
  const cache = profile.info_cache;
  if (cache && typeof cache === "object") {
    const entry = (cache as Record<string, unknown>)[target];
    profile.info_cache = entry === undefined ? {} : { [target]: entry };
  }
  profile.profiles_order = [target];
  profile.last_used = target;
  profile.last_active_profiles = [target];
  await Bun.write(path, JSON.stringify(data));
}

// 镜像同步:--delete 让目标向源对齐(等价“清理旧副本 + 拷新”,但增量、快)。
// 只同步 target 一个 profile 的目录 + 根级共享数据(Local State 等),其余 profile 排除。
async function syncProfile({ target, others }: ProfileSelection): Promise<void> {
  console.log(`⏳ Syncing profile "${target}": ${SRC} → ${DST} …`);
  await mkdir(DST, { recursive: true });
  const excludes = [
    `/${STATE_FILE}`, // 本工具状态文件,源侧不存在,不能被 --delete 清掉
    ...others.map((p) => `/${p}/`),
    ...ROOT_EXCLUDES.map((d) => `/${d}/`),
    ...RUNTIME_FILES.map((f) => `/${f}`),
    "*.pma",
    ...ANY_EXCLUDES,
  ].map((p) => `--exclude=${p}`);
  const r = Bun.spawnSync(["rsync", "-a", "--delete", ...excludes, `${SRC}/`, `${DST}/`], {
    stdout: "inherit",
    stderr: "inherit",
  });
  if (!r.success) die(`rsync failed (exit ${r.exitCode})`);
  await Promise.all(RUNTIME_FILES.map((f) => unlink(join(DST, f)).catch(() => {})));
  await pruneLocalState(target);
  await stripMigratedExtensions(target);
  await Bun.write(join(DST, STATE_FILE), `${target}\n`);
}

// 后台启动带调试端口的 Chrome。
function launchChrome(target: string): void {
  console.log(`🚀 Launching Chrome (CDP :${PORT}, profile=${target}, user-data-dir=${DST}) …`);
  // --remote-debugging-port 要求非默认 user-data-dir(Chrome 136+),故启动于独立副本目录。
  // Local State 随副本带来,自动恢复 chrome://flags 开关,与日常实例等价。
  // --remote-allow-origins=* 放开 CDP WebSocket 的 Origin 校验,便于外部工具连接。
  const proc = Bun.spawn(
    [
      CHROME_BIN,
      `--user-data-dir=${DST}`,
      // 显式锁定 profile:副本里只有这一个 profile,不指定则 Chrome 依 Local State 推断,
      // 且 unclean exit 时会连带拉起其它 last_active_profiles(目录已不存在 → 新建空 profile)。
      `--profile-directory=${target}`,
      "--remote-debugging-address=0.0.0.0",
      `--remote-debugging-port=${PORT}`,
      "--remote-allow-origins=*",
      "--origin-trial-disabled-features=CanvasTextNg|WebAssemblyCustomDescriptors",
      // 关闭 Chrome Sync: 否则登录着 Google 账号的 debug Chrome 会通过云端同步把原 profile 的扩展/书签等静默拉回来。
      // 不影响 cookie / 密码 / 网站登录态 (那些是 profile 本地数据, 不走 sync)。
      "--disable-sync",
      // 关闭 Chrome 出厂默认捆绑应用 (Application Launcher for Drive 等 Google 内置扩展, 走 external_extensions.json 分发)。
      "--disable-default-apps",
      "--no-first-run",
      "--no-default-browser-check",
    ],
    { stdin: "ignore", stdout: "ignore", stderr: "ignore" },
  );
  proc.unref(); // 脱离父进程,CLI 退出后 Chrome 继续运行(等价 & disown)。
}

// 轮询 CDP 端点至就绪(最多 15s),打印外部工具连接所需信息。
async function waitForCdp(profile: string): Promise<void> {
  const url = `http://127.0.0.1:${PORT}/json/version`;
  process.stdout.write("⏳ Waiting for CDP");
  for (let i = 0; i < 30; i++) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(1000) });
      if (res.ok) {
        const info = (await res.json()) as { Browser?: string; webSocketDebuggerUrl?: string };
        console.log("");
        console.log("✅ CDP ready");
        console.log(`   Browser   : ${info.Browser ?? ""}`);
        console.log(`   WebSocket : ${info.webSocketDebuggerUrl ?? ""}`);
        console.log(`   Endpoint  : ${url}`);
        console.log(`   user-data : ${DST}`);
        console.log(`   profile   : ${profile}`);
        return;
      }
    } catch {
      // CDP 尚未就绪,继续轮询
    }
    process.stdout.write(".");
    await Bun.sleep(500);
  }
  console.log("");
  console.log(`⚠️ Chrome launched but CDP not detected within 15s; open ${url} to check`);
}

// 用原始 user-data-dir 启动日常 Chrome。debug Chrome 运行时,原始 Chrome 因单例锁通常无法直接打开,
// 此命令通过 `open -na` 显式拉起新实例,与 debug 副本并存。
// 不指定 --profile-directory:保持日常 Chrome 的原有行为(按 Local State 的 last_used / last_active_profiles 恢复窗口)。
export async function runOriginal(): Promise<number> {
  if (process.platform !== "darwin") {
    console.error(`❌ macOS only (current: ${process.platform})`);
    return 1;
  }
  const executable = await access(CHROME_BIN, constants.X_OK).then(() => true, () => false);
  if (!executable) {
    console.error(`❌ Chrome not found: ${CHROME_BIN}`);
    return 1;
  }
  console.log(`🚀 Launching original Chrome (user-data-dir=${SRC}) …`);
  // -n 强制新实例; -a 指定 app; --args 后传给 Chrome。显式 --user-data-dir 指向默认目录,
  // 绕过 macOS Launch Services 对已运行实例的复用。
  const r = Bun.spawnSync(
    ["open", "-na", "Google Chrome", "--args", `--user-data-dir=${SRC}`],
    { stdout: "inherit", stderr: "inherit" },
  );
  return r.success ? 0 : 1;
}

// 镜像日常 Chrome profile 到独立目录并以 CDP 调试端口启动,供外部工具(CDP / MCP)连接。
export async function run(): Promise<number> {
  if (process.platform !== "darwin") {
    console.error(`❌ macOS only (current: ${process.platform})`);
    return 1;
  }
  try {
    await preflight();
    // 先退出 Chrome 再选 profile:Local State 是延迟落盘的,运行中读可能拿到切换 profile 前的旧值,
    // 优雅退出会 flush 出最终的 last_used。
    await quitChrome();
    const selection = await selectProfile();
    await resetIfProfileChanged(selection.target);
    await syncProfile(selection);
    launchChrome(selection.target);
    await waitForCdp(selection.target);
    return 0;
  } catch (err) {
    console.error(`❌ ${err instanceof Error ? err.message : String(err)}`);
    return 1;
  }
}
