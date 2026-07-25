```When Editing
本文档作用: 面向使用者的发版记录; 只写用户感受得到的变化, MUST NOT 写技术细节 (→ CHANGELOG.dev.md)
遵循 AGENTS.md 文档编写规范
- 写: 新功能 / 行为修复 / 体验 / 安全 / 命令迁移
- MUST NOT 写: 文件路径 / 函数名 / 组件名 / 依赖包名 / 重构细节
- 单条 ≤ 2 行, 单版本 ≤ 5 条; 段落: Added / Changed / Fixed / Removed / Security
- 无用户可感知变化 → 占位: `跟随版本同步发布`
```

# Changelog

[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) + [SemVer](https://semver.org/).

## [0.7.0] - 2026-07-25

### Changed

- 同步不再退出日常 Chrome: 日常窗口原样保留, 只终止上一轮启动的 debug 实例.
- 同步对象改为日常 Chrome 当前活跃的 profile (原先依赖延迟落盘的记录, 刚切 profile 就运行会同步到上一个).

### Fixed

- 运行中拷贝 Cookies / 密码 / 历史等数据库改为原子快照, 消除边拷边写导致副本损坏、登录态丢失的风险.
- debug Chrome 启动不再弹 "Restore pages? / Chrome didn't shut down correctly" 恢复提示.

## [0.6.0] - 2026-07-25

### Changed

- debug 副本目录改为 `~/.config/jj-chrome-debug-profile-sync` (原 `~/.cache/chrome-debug-profile-sync`).
- 旧目录不自动迁移: 升级后首次运行会重新同步一份副本, 旧目录可手动删除回收空间.

## [0.5.0] - 2026-07-25

### Changed

- 运行时由 Bun 换成 Rust; 命令、输出、退出码、同步与启动行为完全不变, 无需改用法.
- 二进制体积由 ~63MB 降到 ~1.6MB, 启动更快; 已装用户直接 `update` 即可换到新版.

### Fixed

- Chrome 配置文件损坏时不再静默跳过 profile 裁剪 / 扩展清理, 会打印告警指出具体文件.

## [0.4.0] - 2026-07-25

### Changed

- `help` / `version` 不再是子命令, 改用 `-h` / `--help` 与 `-v` / `--version`; 旧写法报未知命令并退出.
- 安装命令 URL 变更为 `.../main/scripts/install.sh` (脚本统一移入 `scripts/`), 旧 URL 失效; 已装用户可直接 `update` 升级.

## [0.3.0] - 2026-07-25

### Changed

- 只同步日常 Chrome 最后使用的那个 profile, 不再整份拷贝全部 profile; 副本体积按单 profile 计算.
- debug Chrome 启动时锁定该 profile, 只开这一个窗口; 头像菜单不再列出未同步的 profile.
- 日常 Chrome 换了 profile 后再运行, 旧副本整体删除重建, 避免残留上一个 profile 的数据.

## [0.2.1] - 2026-07-07

### Fixed

- debug Chrome 不再禁用扩展功能, 可正常安装/使用扩展; 同时彻底切断原 profile 扩展的迁移路径(本地文件清理 + 关闭 Chrome Sync 云端拉取), 不会再静默出现原 profile 装过的扩展.

## [0.2.0] - 2026-06-16

### Added

- 新增 `original` 子命令: 启动原始 Chrome (默认 profile), 可与 debug 实例并存使用.

### Changed

- debug Chrome 启动时不再携带任何扩展 (同步阶段排除扩展数据 + 启动禁用扩展).

## [0.1.1] - 2026-06-09

### Added

- README 补充 Chrome MCP 配合使用场景及 `.mcp.json` 配置示例

## [0.1.0] - 2026-06-08

### Added

- 无参数运行: 同步本地 Chrome profile 到独立副本, 并以 CDP 调试端口 `9222` 启动 Chrome 供外部工具连接.
- 同步前自动退出在运行的 Chrome 取一致快照, 仅复制用户数据 (排除缓存 / 锁 / 端侧模型), 保留登录态、扩展、书签与站点数据.
- `help` 显示 chrome profile 路径 (日常源目录、调试副本目录、CDP 端点).

[0.7.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/releases/tag/v0.1.0
