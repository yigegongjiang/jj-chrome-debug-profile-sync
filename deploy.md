# Release flow

AI runs this on its own after finishing code changes. Pushing a `v*` tag triggers the Actions build + publish.

## TL;DR

1. `bun run typecheck && bun run build` to verify
2. Write a new version section in `CHANGELOG.md` + mirror it into `CHANGELOG` (add technical sub-items) + sync `package.json#version` (matching the tag)
3. commit + annotated tag (`-a -m`) + push branch + tag
4. Bug in the released version → amend + delete remote tag + re-tag + force push

## 1. Verify

```bash
bun run typecheck
bun run build
./dist/jj-chrome-debug-profile-sync-darwin-arm64 version
```

## 2. Write the version

- Version number: bump PATCH (third digit) by default; new feature → MINOR; breaking change → MAJOR.
- Add a `## [X.Y.Z] - YYYY-MM-DD` section at the top of `CHANGELOG.md` (+ a `[X.Y.Z]:` compare link at the bottom), then mirror that section into `CHANGELOG` with one technical sub-item per entry — both files move together → [llm-doc-style.md](./llm-doc-style.md). The user-facing summary stays concise; commit details are aggregated into the Release automatically by the Actions `generate_release_notes`.
- `package.json#version` must match the tag (tag has the `v`, version does not; injected into the binary by `build.ts`). The first Actions step validates this and fails on mismatch.

## 3. Publish

```bash
git add .
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin <branch> vX.Y.Z
```

> Use an annotated tag (`-a -m`) rather than a lightweight one: this is compatible with the `tag.gpgsign=true` config (when enabled, a lightweight tag is force-upgraded to signed but lacks a message → fail).

## 4. amend to fix a bug in the released version

The AI autonomously detects the "bug in the just-released version, don't ship a new version" scenario (signals: feedback points at the just-pushed tag / change is tiny and only fixes a defect / tone implies a continuation of the last version, e.g. "that one just now", "the one just shipped"). In that case:

> **commit + tag must be updated together**: after amend the commit hash changes, but the remote tag still points to the old hash → the Release artifact diverges from main HEAD. Force-pushing the commit alone is not enough; the remote tag must be deleted and re-created, otherwise Actions won't re-run the build.

```bash
git commit -a --amend --no-edit
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag -a vX.Y.Z -m "vX.Y.Z"
git push --force-with-lease origin <branch>
git push origin vX.Y.Z
```
