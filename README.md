```When Editing
本文档作用: 工程总览 (价值主张 / 使用 / 架构 / 结构); MUST NOT 写发布流程 (→ workflow.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 章节按需增删, 只留项目真有的; 首行一行价值主张, MUST NOT 带 LLM 提示
- 短并列项用表格; 可执行步骤 fenced + `#` 注释同行
- NEVER 写「开发」段 (VibeCoding 不向人类解释 dev 命令)
```

# jj-chrome-debug-profile-sync

rsync 本地 Chrome profile 到独立副本 + 以 CDP 端口 `9222` 启动 debug Chrome 供外部工具 (chrome-devtools-mcp / DevTools / 自动化) 连接; Bun 单文件可执行, 仅 macOS.

## 使用

安装 (默认装到 `$HOME/.local/bin`, 可用 `VERSION` / `INSTALL_DIR` / `REPO` 覆写):

```bash
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-chrome-debug-profile-sync/main/install.sh | bash
```

命令 `jj-chrome-debug-profile-sync`, 无参数运行即同步 profile 并启动 debug Chrome.

<!-- prettier-ignore -->
| 命令 | 别名 | 说明 |
|---|---|---|
| `(无)` | — | 同步 profile → 启动 debug Chrome (CDP `:9222`, 无扩展) |
| `original` | — | 启动原始 Chrome (默认 profile, 与 debug 实例并存) |
| `help` | `-h` / `--help` | 用法 |
| `version` | `-v` / `--version` | 版本 |
| `update` | `upgrade` | 自更新 (仅编译后二进制) |
| `uninstall` | — | 卸载 (仅编译后二进制) |

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

## 架构

Bun runtime + TypeScript; `bun build --compile` 产出 macOS x64 / arm64 单文件二进制; GitHub Actions 在 `v*` tag push 时构建并发布 Release (附 `checksums.txt`); `install.sh` 从 Release 拉取对应架构资产 + SHA256 校验.

## 项目结构

```
src/
  index.ts      # CLI 入口 / 子命令分发 / self-update / uninstall
  chrome.ts     # rsync profile + 退出运行中 Chrome + 以 CDP 端口启动 debug Chrome
  download.ts   # 带进度条的 GitHub Release 资产下载
build.ts        # bun build --compile, 注入 BUILD_NAME / BUILD_VERSION / BUILD_REPO
install.sh      # curl | bash 安装脚本 (macOS only)
.github/workflows/  # tag push → typecheck + build + checksums + release
```
