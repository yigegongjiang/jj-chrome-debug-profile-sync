import { constants } from "node:fs";
import { access, mkdir, readdir, stat, unlink } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

const HOME = homedir();
const CHROME_BIN = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
export const SRC = join(HOME, "Library/Application Support/Google/Chrome"); // 日常(源)User Data 根目录
export const DST = join(HOME, ".cache/chrome-debug-profile-sync"); // 独立调试副本
export const PORT = "9222";

// 只同步用户数据;排除缓存 / 锁与运行态 / 可按需重建的端侧模型(约 4G)。
// 登录态、书签、站点数据(Cookies、Login Data、IndexedDB、Local Storage)均保留;原始 profile 的扩展不迁移。
const RSYNC_EXCLUDES = [
  "/SingletonLock", "/SingletonSocket", "/SingletonCookie", // 单例锁
  "/DevToolsActivePort", "/RunningChromeVersion", "/lockfile", // 运行态
  "/Crashpad/", "/BrowserMetrics/", "*.pma", // 崩溃 / 指标
  "/OptGuideOnDeviceModel/", "/OptGuideOnDeviceClassifierModel/", // 端侧 AI 模型
  "/OnDeviceHeadSuggestModel/", "/optimization_guide_model_store/", "/WasmTtsEngine/",
  "/component_crx_cache/", "/extensions_crx_cache/", // 组件 / 扩展 crx 缓存
  "Extensions/", "Extension State/", "Extension Rules/", "Extension Scripts/", // 原始扩展本体 / 状态不迁移(debug chrome 仍可自行安装扩展)
  "Local Extension Settings/", "Sync Extension Settings/", "Managed Extension Settings/",
  "/GraphiteDawnCache/", "/GrShaderCache/", "/GPUPersistentCache/", "/ShaderCache/", // GPU / Shader
  "Cache/", "Code Cache/", "GPUCache/", "DawnWebGPUCache/", "CacheStorage/", "ScriptCache/", // 通用缓存
];

// debug 目录上轮运行 Chrome 自生成的锁/运行态文件,--exclude 会令 --delete 跳过,故显式清除。
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
// 故每个 profile 目录下这两个文件都要清掉 extensions 记录(Secure Preferences 同时清对应的防篡改 MAC,否则触发"设置被篡改"提示)。
async function stripMigratedExtensions(): Promise<void> {
  const entries = await readdir(DST, { withFileTypes: true }).catch(() => []);
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    for (const file of ["Preferences", "Secure Preferences"]) {
      const path = join(DST, entry.name, file);
      const exists = await access(path, constants.F_OK).then(() => true, () => false);
      if (!exists) continue;
      const data = await Bun.file(path).json().catch(() => null);
      if (!data || typeof data !== "object") continue;
      delete data.extensions;
      delete data.protection?.macs?.extensions;
      await Bun.write(path, JSON.stringify(data));
    }
  }
}

// 镜像同步:--delete 让目标向源对齐(等价“清理旧副本 + 拷新”,但增量、快)。
async function syncProfile(): Promise<void> {
  console.log(`⏳ Syncing profile: ${SRC} → ${DST} …`);
  await mkdir(DST, { recursive: true });
  const excludes = RSYNC_EXCLUDES.map((p) => `--exclude=${p}`);
  const r = Bun.spawnSync(["rsync", "-a", "--delete", ...excludes, `${SRC}/`, `${DST}/`], {
    stdout: "inherit",
    stderr: "inherit",
  });
  if (!r.success) die(`rsync failed (exit ${r.exitCode})`);
  await Promise.all(RUNTIME_FILES.map((f) => unlink(join(DST, f)).catch(() => {})));
  await stripMigratedExtensions();
}

// 后台启动带调试端口的 Chrome。
function launchChrome(): void {
  console.log(`🚀 Launching Chrome (CDP :${PORT}, user-data-dir=${DST}) …`);
  // --remote-debugging-port 要求非默认 user-data-dir(Chrome 136+),故启动于独立副本目录。
  // Local State 随副本带来,自动恢复 chrome://flags 开关,与日常实例等价。
  // --remote-allow-origins=* 放开 CDP WebSocket 的 Origin 校验,便于外部工具连接。
  const proc = Bun.spawn(
    [
      CHROME_BIN,
      `--user-data-dir=${DST}`,
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
async function waitForCdp(): Promise<void> {
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
// 此命令通过 `open -na` 显式拉起新实例,使用默认 profile,与 debug 副本并存。
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
    await quitChrome();
    await syncProfile();
    launchChrome();
    await waitForCdp();
    return 0;
  } catch (err) {
    console.error(`❌ ${err instanceof Error ? err.message : String(err)}`);
    return 1;
  }
}
