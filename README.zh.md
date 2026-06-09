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

## 配合 Chrome MCP

通过本工具启动 debug Chrome 后, 将 [chrome-devtools-mcp](https://github.com/ChromeDevTools/chrome-devtools-mcp) 指向 `http://127.0.0.1:9222` — AI agent (Claude Code / Codex 等) 即可通过 CDP 直接操控浏览器, 无需逐次手动审批.

`.mcp.json` 示例:

```json
{
  "mcpServers": {
    "chrome-devtools": {
      "command": "npx",
      "args": [
        "chrome-devtools-mcp@latest",
        "--browser-url=http://127.0.0.1:9222"
      ]
    }
  }
}
```
