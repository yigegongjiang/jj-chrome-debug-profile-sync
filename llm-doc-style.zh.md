# md 编写指南

高密度原则; 下列根目录 md (`AGENTS` / `README` / `deploy` / `CHANGELOG` 及本文) MUST 符合本指南.

## 分层 (单一信源, 互引不复述)

<!-- prettier-ignore -->
| 文件 | 内容 |
|---|---|
| `AGENTS.md` | LLM 约束 / 工作模式 / 硬规则 (`CLAUDE.md` 软链至此) |
| `README.md` | 工程总览 / 子命令 / 命令 |
| `deploy.md` | 发布流程 (tag → Actions) |
| `CHANGELOG.md` | 面向使用者的发版记录 |
| `CHANGELOG` | 面向开发者: 镜像 `CHANGELOG.md` + 每条补技术细节 |

跨文档用 `[xxx.md](./xxx.md)` 引用, MUST NOT 复述事实. 一次性 secrets / 资源按需抽到 `setup.md` (当前工程无).

## 通用风格

- 能一行不写两行, 能列表不写段落
- 短句; 用 `->` `/` `+` 替连接词
- 强度词: MUST / MUST NOT / SHOULD
- 短并列项 (≤12 中文/单元格) 用表格; 表格前紧贴 `<!-- prettier-ignore -->`
- 长并列点用列表
- CommonMark/GFM; MUST NOT Obsidian 语法 / HTML 折叠
- 中文行文; 命令 / 术语 / 报错保留原文

## 代码块

- 所有 fenced code MUST 指定语言, MUST NOT 无语言标识
- 命令块注释贴 `#` 同行

## AGENTS.md

- 只写 LLM 约束, MUST NOT 写工程说明 (结构 / 命令 -> README)
- 首段一行角色定位 + link 到 README / deploy / 本文
- 必含: 工作模式 (AI 全程闭环, 含发布) / 文档约束; 硬约束按需 (有硬规则才设段)
- `CLAUDE.md` 是本文件软链, 改 `AGENTS.md` 即同步

## README.md

- 首段一行价值主张, MUST NOT 带 LLM 提示
- 子命令 / 命令用表格
- 命令块 fenced + `#` 注释同行
- 发布细节抽到 `deploy.md`, 此处仅 link

## deploy.md

- 顶部 TL;DR ≤ 4 行
- 步骤编号清晰 (验证 -> 写版本 -> tag + push -> Actions 自动发布)
- 风险点 / 不可逆操作用 `>` 引用块
- 高危操作 (amend / force push) MUST 标禁用条件

## CHANGELOG — 双文件 (Keep a Changelog + SemVer)

`CHANGELOG.md` (用户向) + `CHANGELOG` (开发向) 同步推进 -> [deploy.md](./deploy.md).

### CHANGELOG.md (用户向)

- 写他们感受得到的事
- 写: 新功能 / 行为修复 / 体验 / 安全 / 命令迁移
- MUST NOT 写: 文件路径 / 函数名 / 组件名 / 依赖包名 / 重构细节 / "改了哪行"
- 单条 ≤ 2 行, 单版本 ≤ 5 条
- 段落: Added / Changed / Fixed / Removed / Security
- 无用户可感知变化的版本用占位: `跟随版本同步发布`
- 中文行文; 命令 / 术语保留原文

### CHANGELOG (开发向)

- `CHANGELOG.md` 的超集: 每条 1:1 镜像, 各加一条缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制 (用户向的反向); ≤ 1 行, 文件/函数/机制级
- 与 `CHANGELOG.md` 同语言

## 反模式 (审稿时优先抓)

- 段落式描述 -> 拆列表
- 同一事实两个文件各写一遍 -> 留一处 + link
- CHANGELOG 写"改了哪个文件 / 函数" -> 改写"用户看到什么变化"
- AGENTS 塞结构 / 命令 / 安装说明 -> 抽到 README
- 表格单元格塞长句 -> 改列表
- fenced code 不写语言 -> 补语言标识
