```When Editing
本文档作用: 工程总览 (价值主张 / 使用 / 架构 / 结构); MUST NOT 写发布流程 (→ workflow.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 章节按需增删, 只留项目真有的; 首行一行价值主张, MUST NOT 带 LLM 提示
- 短并列项用表格; 可执行步骤 fenced + `#` 注释同行
- NEVER 写「开发」段 (VibeCoding 不向人类解释 dev 命令)
```

# jj-chrome-debug-profile-sync

rsync 本地 Chrome 当前使用的 profile 到独立副本 + 以 CDP 端口 `9222` 启动 debug Chrome 供外部工具 (chrome-devtools-mcp / DevTools / 自动化) 连接; 日常 Chrome 全程不退出; Rust 单文件可执行, 仅 macOS.

## 使用

安装 (默认装到 `$HOME/.local/bin`, 可用 `VERSION` / `INSTALL_DIR` / `REPO` 覆写):

```bash
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-chrome-debug-profile-sync/main/scripts/install.sh | bash
```

命令 `jj-chrome-debug-profile-sync`, 无参数运行即同步 profile 并启动 debug Chrome.

<!-- prettier-ignore -->
| 命令 | 别名 | 说明 |
|---|---|---|
| `(无)` | — | 同步当前 profile → 启动 debug Chrome (CDP `:9222`, 不迁移原扩展, 可自行装用); 只终止上一轮 debug 实例, 日常 Chrome 不动 |
| `original` | — | 启动原始 Chrome (沿用其自身 profile 状态, 与 debug 实例并存) |
| `update` | `upgrade` | 自更新 (仅编译后二进制) |
| `uninstall` | — | 卸载 (仅编译后二进制) |
| `--help` | `-h` | 用法 |
| `--version` | `-v` | 版本 |

配合 [chrome-devtools-mcp](https://github.com/ChromeDevTools/chrome-devtools-mcp), `.mcp.json`:

```json
{
  "mcpServers": {
    "chrome-devtools": {
      "command": "npx",
      "args": ["chrome-devtools-mcp@latest", "--browser-url=http://127.0.0.1:9222"]
    }
  }
}
```

跨 Chromium 产品迁移见 [Profile 迁移手册](docs/browser-profile-migration.md).

## 辨认 debug 窗口

副本沿用日常 profile 的头像 / 书签 / 主题, 两个窗口外观本来一致; 同步时在副本侧写入标记 (只作用于副本, 日常实例不受影响):

- 浏览器 UI 固定亮色 + 红色主题种子色 (`0xFFFF0000`) → 标签栏 / 工具栏是亮红, 与跟随系统 (暗色) 的日常窗口对比强烈
- profile 名 = `DEBUG :9222` → 头像菜单 / profile 卡片显示
- 附带差异: 工具栏无扩展图标 (原扩展不迁移), 头像菜单只列一个 profile

> 亮色 UI 的代价: Chrome 150 实测该配色键同时决定网页的 `prefers-color-scheme`, debug 侧网页按 light 渲染. 需要暗色渲染时用 CDP `Emulation.setEmulatedMedia` 覆盖.

Dock 图标 / `Cmd+Tab` 名称无法区分: 两个实例同属一个 app bundle, macOS 按 bundle 聚合; 要换图标须复制整个 `Google Chrome.app` 改 `Info.plist` 并重签名 (~1GB, Chrome 每次更新失效), 不做.

权威判据: `chrome://version` → `Command Line` 含 `--remote-debugging-port=9222`, `Profile Path` 指向 `~/.config/jj-chrome-debug-profile-sync`; 终端侧 `lsof -nP -iTCP:9222 -sTCP:LISTEN`.

## 热同步一致性

日常 Chrome 运行中做 rsync 有两处风险, 各自的处理:

- SQLite 库跨事务撕裂 (rsync 流式读, 数 GB 拷贝窗口内源库仍在提交): rsync 后按 SQLite 文件头识别副本内的库, 逐个用 APFS clonefile (`cp -pc`) 覆盖 → 单文件原子快照, 内容/mtime 与源一致 (rsync 增量不受影响); 先 clone `-journal` / `-wal` 再 clone 主库 (最坏是回滚丢一个事务, 而非损坏), `-shm` 删除让 SQLite 重建
- `Local State#profile.last_used` 延迟落盘 (刚切 profile 立刻运行会读到旧值): 优先取 `lsof` 中日常 Chrome 唯一活跃的 profile, 多个活跃窗口时回落 `last_used`

Chrome 对 `History` / `Web Data` 等库开 exclusive locking, 外部进程连读锁都拿不到 (`VACUUM INTO` 直接 `SQLITE_BUSY`), 故只能走文件级快照. LevelDB 目录 (Local Storage / IndexedDB) 无对应机制, 极端情况由 Chrome 自行重建.

## 多 Profile

- 同步对象 = 日常 Chrome 当前活跃 profile (回落 `Local State` 的 `profile.last_used`); 其余 profile + Guest Profile 不进副本
- 副本内 `Local State` 裁剪为仅该 profile, 启动以 `--profile-directory` 锁定 → debug Chrome 只开这一个, 头像菜单不会列出未同步的 profile
- 日常 Chrome 换了 profile 再运行 → 整个副本删除重建 (不增量); 同一 profile 连续运行 → rsync 增量

## 架构

Rust (edition 2024) + `cargo build --release`, 双 target 产出 macOS x64 / arm64 静态单文件 (~1.6MB); 依赖仅 `serde_json` / `ureq` (rustls) / `sha2`, 无系统运行时. GitHub Actions 在 `v*` tag push 时于 macOS runner 构建并发布 Release (附 `checksums.txt`); `scripts/install.sh` 从 Release 拉取对应架构资产 + SHA256 校验.

## 项目结构

```
src/
  main.rs       # CLI 入口 / 子命令分发 / self-update / uninstall
  chrome.rs     # 选定活跃 profile + 终止上一轮 debug 实例 + rsync 单 profile (热同步 + SQLite clone 快照) + 以 CDP 端口启动 debug Chrome
  net.rs        # 带进度条的 Release 资产下载 + CDP JSON 探测
docs/
  browser-profile-migration.md  # Chromium profile 跨产品迁移 / 加密 / 重签 / 验证
Cargo.toml      # 包名 = 二进制名 = 资产名前缀; NAME / VERSION / REPO 由 CARGO_PKG_* 注入
scripts/
  build.sh          # cargo build 双 target → dist/<name>-darwin-{arm64,x64}
  install.sh        # curl | bash 安装, 从 Release 拉二进制 + SHA256 校验 (macOS only)
  install-local.sh  # 源码构建 + 装到 ~/.local/bin (本地验证)
.github/workflows/  # tag push → fmt + clippy + build + checksums + release
```
