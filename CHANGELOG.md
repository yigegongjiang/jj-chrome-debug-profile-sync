# Changelog

[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) + [SemVer](https://semver.org/).

## [0.1.0] - 2026-06-08

### Added

- 无参数运行:同步本地 Chrome profile 到独立副本,并以 CDP 调试端口 `9222` 启动 Chrome 供外部工具连接。
- 同步前自动退出在运行的 Chrome 取一致快照,仅复制用户数据(排除缓存 / 锁 / 端侧模型),保留登录态、扩展、书签与站点数据。
- `help` 显示 chrome profile 路径(日常源目录、调试副本目录、CDP 端点)。

[0.1.0]: https://github.com/yigegongjiang/jj-chrome-debug-profile-sync/releases/tag/v0.1.0
