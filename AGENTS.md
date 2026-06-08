# AGENTS

rsync the Chrome user profile, then launch a debug-mode Chrome for CDP / external tooling. Bun single-file executable, macOS only. Project overview → [README.md](./README.md); release flow → [deploy.md](./deploy.md); doc authoring → [llm-doc-style.md](./llm-doc-style.md).

## Workflow (AI-only)

- Code / test / build / deploy / release are all executed by Claude Code or Codex
- Design decisions (architecture / selection / naming / dependencies) follow AI judgment; MUST NOT force human conventions
- MUST NOT ask back unless necessary; decide and execute directly (deploy / technical choices / doc sync / version number / changelog)
- User role = online acceptance; MUST NOT pull humans into the design loop

## Doc constraints

- All docs (README / CHANGELOG / deploy / AGENTS / comments) MUST be concise, focused, zero redundancy
- Style spec → [llm-doc-style.md](./llm-doc-style.md); when reviewing, MUST check against the "Anti-patterns" section
- One line over two, a list over a paragraph; over-dense beats filler — MUST NOT pad with fluff
