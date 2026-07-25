```When Editing
本文档作用: 工程工作流程 (可用工具 / 调试 / 发布); MUST NOT 写工程说明 (→ README.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 所有段落均为条件段, 根据工程实际决定保留或删除; 存在即为明确流程, MUST NOT 附加强度标记
- 发布内按顺序编号步骤; 顶部 TL;DR ≤ 5 行; 删除子段后重编号保持连续
- 风险点 / 不可逆操作用 `>` 引用块; 高危操作 MUST 标禁用条件
```

# 可用工具

- `gh` 已登录
- `cargo` / `rustup` 已装 (含 clippy / rustfmt, 双 target `aarch64-apple-darwin` + `x86_64-apple-darwin`)
- `chrome-devtools-mcp` (CDP `http://127.0.0.1:9222`)

# 发布

代码变更完成后立即执行（= 需求交付的最后环节）。交付 = 预部署 + push; push `v*` tag → GitHub Actions (`.github/workflows/release.yml`) 于 macOS runner 自动 fmt + clippy + build + checksums + 发布 Release.

## TL;DR

1. 验证: `cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && ./scripts/build.sh && ./dist/jj-chrome-debug-profile-sync-darwin-arm64 --version`
2. 写版本: `Cargo.toml#version` + `Cargo.lock` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步 (与 tag 一致)
3. 预部署: `./scripts/install-local.sh`
4. 发布: commit + annotated tag (`-a -m`) + push branch + tag

## 1. 验证

```bash
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
./scripts/build.sh
./dist/jj-chrome-debug-profile-sync-darwin-arm64 --version
```

行为改动 (rsync 排除项 / Chrome 启动参数 / profile 选择) 另需实跑无参路径, 确认 CDP 就绪:

```bash
jj-chrome-debug-profile-sync   # 会退出日常 Chrome, 同步副本, 拉起 debug 实例
curl -s http://127.0.0.1:9222/json/version
```

## 2. 写版本

- 版本号: 默认递增 PATCH (第三位); 超大功能更新/调整 → MINOR; 禁止 → MAJOR（除非人类主动要求）.
- `Cargo.toml#version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致); tag 带 `v` 前缀, version 字段不带.
- 改 `Cargo.toml#version` 后跑一次 `cargo build` 让 `Cargo.lock` 同步 (CI 用 `--locked`, 不同步会 fail).
- CHANGELOG.md = 用户向; CHANGELOG.dev.md = 镜像 + 技术子项.

> Actions 第一步校验 `v${Cargo.toml#version} == tag`, 不一致直接 fail.

## 3. 预部署

本机装载 = 交付必经节点; 与改动大小无关, 每次发布都执行.

```bash
./scripts/install-local.sh   # 构建 + 原子替换 ~/.local/bin 同名二进制
```

## 4. 发布

```bash
git add -- Cargo.toml Cargo.lock CHANGELOG.md CHANGELOG.dev.md   # 仅版本相关文件
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main
git push origin vX.Y.Z
```

> annotated tag (`-a -m`) 而非 lightweight: 兼容 `tag.gpgsign=true` (启用时 lightweight 会被强升为 signed 但缺 message → fail).
