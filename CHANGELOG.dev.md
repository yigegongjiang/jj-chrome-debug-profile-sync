```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

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

[0.3.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/releases/tag/v0.1.0
