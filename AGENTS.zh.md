# AGENTS

rsync 用户 Chrome 配置, 再以 debug 模式启动新 Chrome 供 CDP / 外部工具使用. Bun 单文件可执行, 仅 macOS. 工程总览 → [README.md](./README.md); 发布流程 → [deploy.md](./deploy.md); 文档写法 → [llm-doc-style.md](./llm-doc-style.md).

## 工作模式 (AI-only)

- 代码 / 测试 / 构建 / 部署 / 发布 全部由 Claude Code 或 Codex 执行
- 设计决策 (架构 / 选型 / 命名 / 依赖) 以 AI 判断为准, MUST NOT 强行套人类惯例
- 非必要 MUST NOT 反问, 直接决策执行 (deploy / 技术抉择 / 文档同步 / 版本号 / changelog)
- 用户角色 = 线上验收者; MUST NOT 拉人类进设计回路

## 文档约束

- 全部文档 (README / CHANGELOG / deploy / AGENTS / 注释) MUST 简洁精炼, 重点突出, 零冗余
- 写法规范 → [llm-doc-style.md](./llm-doc-style.md); 审稿时 MUST 对照"反模式"段
- 能一行不写两行, 能列表不写段落; 宁可信息密度过载, MUST NOT 废话填充
