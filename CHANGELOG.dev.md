```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

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

[0.1.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/releases/tag/v0.1.0
