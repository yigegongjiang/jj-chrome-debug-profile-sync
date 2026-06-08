# jj-chrome-debug-profile-sync

rsync 用户 Chrome 配置到目标目录, 再以 debug 模式 (远程调试端口) 启动新 Chrome 供 CDP / 外部工具使用. Bun 单文件可执行, 仅 macOS; `install.sh` 安装, `update` 自更新.

## 安装

```bash
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-chrome-debug-profile-sync/main/install.sh | bash
```

默认装到 `$HOME/.local/bin`. 可用 `VERSION` / `INSTALL_DIR` / `REPO` 覆写.

## 用法

安装后的命令名为 `jj-chrome-debug-profile-sync` (与仓库 / 二进制同名). 无参数运行即同步 profile 并启动 debug Chrome; 子命令:

<!-- prettier-ignore -->
| 命令 | 别名 | 说明 |
|---|---|---|
| `(无)` | — | 同步 profile → 启动 debug Chrome (CDP) |
| `help` | `-h` / `--help` | 用法 |
| `version` | `-v` / `--version` | 版本 |
| `update` | `upgrade` | 自更新 (仅编译后二进制) |
| `uninstall` | — | 卸载 (仅编译后二进制) |
