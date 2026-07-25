```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

## [0.5.0] - 2026-07-25

### Changed

- 运行时由 Bun 换成 Rust; 命令、输出、退出码、同步与启动行为完全不变, 无需改用法.
  - `src/{index,chrome,download}.ts` → `src/{main,chrome,net}.rs`; 删 `build.ts` / `package.json` / `bun.lock` / `tsconfig.json`, 新增 `Cargo.toml` + `Cargo.lock` (edition 2024, `rust-version = 1.85`).
  - `BUILD_NAME/VERSION/REPO` 的 `--define` 注入改为 `env!("CARGO_PKG_{NAME,VERSION,REPOSITORY}")`; `repository` 为完整 URL, self-update 前 trim `https://github.com/` 取 `<owner>/<repo>`.
  - spawn stdio 逐点对齐 Bun 语义: `pgrep`/`lsof`/`osascript`/`pkill` 走 `silent_status()` 全 `Stdio::null()` (Rust 默认 inherit, 会把 PID 列表与 lsof 表打到终端); `rsync`/`open` 保持 inherit.
  - `access(X_OK)` → `metadata().permissions().mode() & 0o111`; `homedir()` → `$HOME` (与 libuv 取值顺序一致); `process.execPath` → `current_exe()` (解析 symlink).
  - `serde_json` 启 `preserve_order` 保持 Chrome pref 键序; 读取走 `from_utf8_lossy` 兜非法 UTF-8 (JS `JSON.parse` 宽容, serde 严格).
  - HTTP 由全局 `fetch` 换 `ureq` 3 (rustls + webpki-roots, 关 `gzip` 以免 `content-length` 与进度条错位); CDP 探测用 `timeout_global(1s)` 的独立 Agent; SHA256 由 `node:crypto` 换 `sha2`.
  - `launch_chrome()` 以 `spawn()` + `drop(child)` 替代 `proc.unref()`; `panic = "abort"` + 全 `Result` 路径, 保证异常退出码仍为 1.
- 二进制体积由 ~63MB 降到 ~1.6MB, 启动更快; 已装用户直接 `update` 即可换到新版.
  - `[profile.release]` `lto = true` + `codegen-units = 1` + `strip = true` + `panic = "abort"`.
  - 新增 `scripts/build.sh` (双 target → `dist/<name>-darwin-{arm64,x64}`, 资产名不变以兼容 `install.sh` 与 v0.4.0 自更新); `install-local.sh` 改调 `build.sh <host_arch>`.
  - `release.yml`: `ubuntu-latest` + `setup-bun` → `macos-latest` (rustc 无 macOS SDK 无法从 Linux 交叉编译 apple-darwin), `typecheck` → `cargo fmt --check` + `cargo clippy --locked -D warnings`, 版本校验与 checksum glob 由 `package.json` 改 `cargo metadata` / `*-darwin-*`, `sha256sum` → `shasum -a 256` (输出同为双空格分隔).

### Fixed

- Chrome 配置文件损坏时不再静默跳过 profile 裁剪 / 扩展清理, 会打印告警指出具体文件.
  - `prune_local_state()` / `strip_migrated_extensions()` 解析失败时 `eprintln!("⚠️ Cannot parse …")`; TS 版 `.catch(() => null)` 后直接 return, 头像菜单裁剪失效却无任何输出.

## [0.4.0] - 2026-07-25

### Changed

- `help` / `version` 不再是子命令, 改用 `-h` / `--help` 与 `-v` / `--version`; 旧写法报未知命令并退出.
  - `src/index.ts` 删 `case "help"` / `case "version"`, USAGE 拆 `Commands` / `Flags` 两段; `update()` 自更新后回显改调 `--version` (仍调 `version` 会落 default 分支 exit 1, 静默丢 `after:` 行).
- 安装命令 URL 变更为 `.../main/scripts/install.sh` (脚本统一移入 `scripts/`), 旧 URL 失效; 已装用户可直接 `update` 升级.
  - `install.sh` → `scripts/install.sh` (git rename, mode 100755 保持); 新增 `scripts/install-local.sh`: `cd` 仓库根 + `bun run build` + tmp→`chmod`→`mv -f` 原子替换 (避免覆盖运行中二进制 ETXTBSY), `INSTALL_DIR` 可覆写, 收尾以 `--version` 验证 (无参会同步 profile 并拉起 Chrome).

## [0.3.0] - 2026-07-25

### Changed

- 只同步日常 Chrome 最后使用的那个 profile, 不再整份拷贝全部 profile; 副本体积按单 profile 计算.
  - 新增 `selectProfile()`: 读源 `Local State` 的 `profile.last_used` + `info_cache` 定 target, 其余 profile 与 `Guest Profile` 生成 `--exclude=/<name>/`; `Local State` 缺失/损坏时回退扫 `SRC` 目录(匹配 `Default` / `Profile N`), 避免 others 为空导致全量同步.
  - `quitChrome()` 移到 `selectProfile()` 之前: `Local State` 是 debounced JsonPrefStore, 运行中读可能拿到切换 profile 前的旧值.
  - `RSYNC_EXCLUDES` 拆为 `ROOT_EXCLUDES` (根级, 生成 `/<dir>/`) + `ANY_EXCLUDES` (任意层级) + `RUNTIME_FILES` (根级文件), 排除项按语义分组拼装.
- debug Chrome 启动时锁定该 profile, 只开这一个窗口; 头像菜单不再列出未同步的 profile.
  - `launchChrome()` 追加 `--profile-directory=<target>`; 不指定时 Chrome 依 `Local State` 推断, 且 unclean exit 会连带拉起 `last_active_profiles` 中已不存在的 profile(新建空目录).
  - 新增 `pruneLocalState()`: sync 后把副本 `Local State` 的 `profile.info_cache` / `profiles_order` / `last_used` / `last_active_profiles` 裁剪为仅 target.
  - `stripMigratedExtensions()` 由遍历全部 profile 目录改为只处理 target.
- 日常 Chrome 换了 profile 后再运行, 旧副本整体删除重建, 避免残留上一个 profile 的数据.
  - 新增 `resetIfProfileChanged()` + 状态文件 `DST/.synced-profile` (记录上轮 target, 加 `--exclude=/.synced-profile` 防被 `--delete` 清掉); 不一致或状态缺失 → `rm -rf DST`, 删除前复核 `DST !== SRC`.

## [0.2.1] - 2026-07-07

### Fixed

- debug Chrome 不再禁用扩展功能, 可正常安装/使用扩展; 同时彻底切断原 profile 扩展的迁移路径(本地文件清理 + 关闭 Chrome Sync 云端拉取), 不会再静默出现原 profile 装过的扩展.
  - `launchChrome()` 移除 `--disable-extensions`, 追加 `--disable-sync` + `--disable-default-apps`: 前者关 Chrome Sync (避免云端把原 profile 已同步扩展拉回), 后者关 Chrome 出厂捆绑应用 (Application Launcher for Drive 等走 `external_extensions.json` 分发的 Google 默认扩展); cookie / 密码 / 网站登录态是本地 profile 数据, 不走 sync, 保持不变.
  - `RSYNC_EXCLUDES` 保留(仍排除 `Extensions/` 等目录, 即不迁移原 profile 扩展本体).
  - 新增 `stripMigratedExtensions()`: sync 后清除各 profile 目录 `Preferences` / `Secure Preferences` 里的 `extensions` 记录(含 `protection.macs.extensions` 防篡改 MAC), 避免 Chrome 用其中残留的 Web Store `update_url` 静默重装 —— 与 `--disable-sync` 双保险.

## [0.2.0] - 2026-06-16

### Added

- 新增 `original` 子命令: 启动原始 Chrome (默认 profile), 可与 debug 实例并存使用.
  - `src/chrome.ts` 导出 `runOriginal()`, 通过 `open -na "Google Chrome" --args --user-data-dir=$SRC` 强制新实例; `src/index.ts` 增加 `case "original"` 分支与 USAGE 条目.

### Changed

- debug Chrome 启动时不再携带任何扩展 (同步阶段排除扩展数据 + 启动禁用扩展).
  - `RSYNC_EXCLUDES` 追加 `Extensions/` / `Extension State/` / `Extension Rules/` / `Extension Scripts/` / `Local|Sync|Managed Extension Settings/`; `launchChrome()` 启动参数追加 `--disable-extensions`.

## [0.1.1] - 2026-06-09

### Added

- README 补充 Chrome MCP 配合使用场景及 `.mcp.json` 配置示例
  - `README.md` / `README.zh.md` 新增 "With Chrome MCP" / "配合 Chrome MCP" 段落, 含 `chrome-devtools-mcp` 连接 `127.0.0.1:9222` 的 `.mcp.json` 示例.

## [0.1.0] - 2026-06-08

### Added

- 无参数运行: 同步本地 Chrome profile 到独立副本, 并以 CDP 调试端口 `9222` 启动 Chrome 供外部工具连接.
  - `src/chrome.ts` `run()` 编排 `preflight → quitChrome → syncProfile → launchChrome → waitForCdp`; `src/index.ts` 无参数入口由占位符切到 `run()`.
- 同步前自动退出在运行的 Chrome 取一致快照, 仅复制用户数据 (排除缓存 / 锁 / 端侧模型), 保留登录态、扩展、书签与站点数据.
  - `rsync -a --delete` + 排除表; `osascript quit` + `pkill -9` 兜底 + `lsof` 端口校验; Chrome 以 `--remote-debugging-port` / `--remote-allow-origins=*` 后台启动后 `unref()` 脱离.
- `help` 显示 chrome profile 路径 (日常源目录、调试副本目录、CDP 端点).
  - `src/chrome.ts` 导出 `SRC` / `DST` / `PORT`; `src/index.ts` help 分支追加 Profile paths 段.

[0.4.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/releases/tag/v0.1.0
