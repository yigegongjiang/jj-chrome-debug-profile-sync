# md Authoring Guide

High-density principle; the root-level md files below (`AGENTS` / `README` / `deploy` / `CHANGELOG` and this one) MUST follow this guide.

## Layering (single source of truth, cross-reference instead of restating)

<!-- prettier-ignore -->
| File | Content |
|---|---|
| `AGENTS.md` | LLM constraints / workflow / hard rules (`CLAUDE.md` symlinks here) |
| `README.md` | Project overview / subcommands / commands |
| `deploy.md` | Release flow (tag → Actions) |
| `CHANGELOG.md` | User-facing release notes |
| `CHANGELOG` | Developer-facing: mirror of `CHANGELOG.md` + per-entry technical detail |

Cross-reference with `[xxx.md](./xxx.md)`, MUST NOT restate facts. One-time secrets / resources go into `setup.md` on demand (none in this project).

## General style

- One line over two, a list over a paragraph
- Short sentences; use `->` `/` `+` instead of conjunctions
- Strength words: MUST / MUST NOT / SHOULD
- Short parallel items (≤12 CJK chars / cell) use a table; place `<!-- prettier-ignore -->` immediately before the table
- Long parallel points use a list
- CommonMark/GFM; MUST NOT use Obsidian syntax / HTML collapsibles
- Prose in Chinese; keep commands / terms / error messages verbatim

## Code blocks

- Every fenced code block MUST declare a language; MUST NOT leave it untagged
- Put command comments inline on the same line with `#`

## AGENTS.md

- Write only LLM constraints; MUST NOT write engineering notes (structure / commands -> README)
- First paragraph: one-line role positioning + links to README / deploy / this file
- MUST include: workflow (fully autonomous AI loop, including release) / doc constraints; a hard-constraints section only when the project has hard rules
- `CLAUDE.md` is a symlink to this file; editing `AGENTS.md` syncs it

## README.md

- First paragraph: one-line value proposition; MUST NOT carry LLM hints
- Subcommands / commands use tables
- Command blocks: fenced + `#` comments inline
- Release details are extracted to `deploy.md`; only link here

## deploy.md

- TL;DR at the top, ≤ 4 lines
- Clearly numbered steps (verify -> write version -> tag + push -> Actions auto-publishes)
- Risks / irreversible operations -> `>` blockquote
- Dangerous operations (amend / force push) MUST mark their forbidden conditions

## CHANGELOG — two files (Keep a Changelog + SemVer)

`CHANGELOG.md` (user-facing) + `CHANGELOG` (developer-facing), kept in lockstep -> [deploy.md](./deploy.md).

### CHANGELOG.md (user-facing)

- Write what they can perceive
- Write: new features / behavior fixes / experience / security / command migrations
- MUST NOT write: file paths / function names / component names / dependency package names / refactor details / "which line changed"
- ≤ 2 lines per entry, ≤ 5 entries per version
- Sections: Added / Changed / Fixed / Removed / Security
- A version with no user-perceivable change uses a placeholder: `released in sync with the version`
- Prose in Chinese; keep commands / terms verbatim

### CHANGELOG (developer-facing)

- Superset of `CHANGELOG.md`: mirror every entry 1:1, append one indented sub-item carrying the technical change
- Sub-items MAY name paths / functions / mechanisms (inverse of the user-facing rule); ≤ 1 line, file/function/mechanism level
- Same language as `CHANGELOG.md`

## Anti-patterns (catch these first when reviewing)

- Paragraph-style description -> split into a list
- The same fact written twice in two files -> keep one + link
- CHANGELOG saying "which file / function changed" -> rewrite as "what the user sees change"
- AGENTS stuffed with structure / commands / install instructions -> extract to README
- Long sentence crammed into a table cell -> switch to a list
- Fenced code with no language -> add the language tag
