# 部署流程

AI 改完代码主动执行. push `v*` tag 触发 Actions 构建发布.

## TL;DR

1. `bun run typecheck && bun run build` 验证
2. `CHANGELOG.md` 写新版段 + 镜像到 `CHANGELOG` (加技术子项) + `package.json#version` 同步 (与 tag 一致)
3. commit + annotated tag (`-a -m`) + push branch + tag
4. 上版 bug → amend + 删远程 tag + 重打 + force push

## 1. 验证

```bash
bun run typecheck
bun run build
./dist/jj-chrome-debug-profile-sync-darwin-arm64 version
```

## 2. 写版本

- 版本号: 默认递增 PATCH (第三位); 新功能 → MINOR; 不兼容改动 → MAJOR.
- `CHANGELOG.md` 顶部新增 `## [X.Y.Z] - YYYY-MM-DD` 段并列改动, 底部补 `[X.Y.Z]:` 对比链接; 再镜像该段到 `CHANGELOG` 每条加一条技术子项 — 两文件同步推进 → [llm-doc-style.md](./llm-doc-style.md). 用户向摘要保持精简; commit 详情由 Actions `generate_release_notes` 自动汇总到 Release.
- `package.json#version` 与 tag 一致 (tag 含 `v`, version 不含, 经 `build.ts` 注入二进制). Actions 第一步会校验, 不一致直接 fail.

## 3. 发布

```bash
git add .
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin <branch> vX.Y.Z
```

> 用 annotated tag (`-a -m`) 而非 lightweight: 兼容 `tag.gpgsign=true` 配置 (开启时 lightweight tag 会被强制升级为 signed 但缺 message → fail).

## 4. amend 修上版 bug

AI 自主识别 "刚发版的 bug, 不发新版" 场景 (信号: 反馈指向刚 push 的 tag / 改动极小仅修缺陷 / 语气暗示是上版延续如 "刚那个" "刚发的"). 此时:

> **commit + tag 必须同步更新**: amend 后 commit hash 变了, 远程 tag 仍指向旧 hash → Release artifact 与 main HEAD 分离. 只 force push commit 不够, 必须删远程 tag 后重打, 否则 Actions 不会重跑构建.

```bash
git commit -a --amend --no-edit
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag -a vX.Y.Z -m "vX.Y.Z"
git push --force-with-lease origin <branch>
git push origin vX.Y.Z
```
