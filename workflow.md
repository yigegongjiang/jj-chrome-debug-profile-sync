```When Editing
本文档作用: 工程工作流程 (可用工具 / 调试 / 发布); MUST NOT 写工程说明 (→ README.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 所有段落均为条件段, 根据工程实际决定保留或删除; 存在即为明确流程, MUST NOT 附加强度标记
- 发布内按顺序编号步骤; 顶部 TL;DR ≤ 5 行; 删除子段后重编号保持连续
- 风险点 / 不可逆操作用 `>` 引用块; 高危操作 MUST 标禁用条件
```

# 可用工具

- `gh` 已登录
- `chrome-devtools-mcp` (CDP `http://127.0.0.1:9222`)

# 调试

```bash
bun run start            # 直接运行 src/index.ts (无 compile)
bun run build && ./dist/jj-chrome-debug-profile-sync-darwin-arm64   # 跑编译产物
```

启动后访问 `http://127.0.0.1:9222/json/version` 验证 CDP 可用, 或通过 chrome-devtools-mcp 调用页面.

# 发布

push `v*` tag → GitHub Actions (`.github/workflows/release.yml`) 自动 typecheck + build + checksums + 发布 Release.

## TL;DR

1. 验证: `bun run typecheck && bun run build && ./dist/jj-chrome-debug-profile-sync-darwin-arm64 version`
2. 写版本: `package.json#version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步 (与 tag 一致)
3. 发布: commit + annotated tag (`-a -m`) + push branch + tag
4. 修上版 bug: amend + 删远程 tag + 重打 + force push

## 1. 验证

```bash
bun run typecheck
bun run build
./dist/jj-chrome-debug-profile-sync-darwin-arm64 version
```

## 2. 写版本

- 版本号: 默认递增 PATCH; 新功能 → MINOR; 不兼容改动 → MAJOR.
- `package.json#version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致); tag 带 `v` 前缀, version 字段不带.
- CHANGELOG.md = 用户向; CHANGELOG.dev.md = 镜像 + 技术子项.

> Actions 第一步校验 `v${package.json#version} == tag`, 不一致直接 fail.

## 3. 发布

```bash
git add -- package.json CHANGELOG.md CHANGELOG.dev.md   # 仅版本相关文件
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main
git push origin vX.Y.Z
```

> annotated tag (`-a -m`) 而非 lightweight: 兼容 `tag.gpgsign=true` (启用时 lightweight 会被强升为 signed 但缺 message → fail).

## 4. 修上版 bug

刚发布版本存在明显 bug (反馈直指刚推 tag / 改动微小且仅修缺陷 / 语气为上一版延续) → amend 修复后重发, 不出新版本号.

> commit + tag 必须一起更新: amend 后 commit hash 变, 远程 tag 仍指旧 hash → Release 产物与 main HEAD 偏离; 仅 force push commit 不够, 必须删远程 tag 重建, 否则 Actions 不会重跑 build.

```bash
git commit -a --amend --no-edit
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag -a vX.Y.Z -m "vX.Y.Z"
git push --force-with-lease origin main
git push origin vX.Y.Z
```
