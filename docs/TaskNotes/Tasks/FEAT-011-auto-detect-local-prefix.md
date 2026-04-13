---
uid: feat-011
status: done
completed: 2026-04-13
priority: medium
timeEstimate: 240
tags:
  - task
  - feature
  - config
  - dx
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - extract
  - walker
  - config
---

# feat(config): Auto-detect local_prefix when omitted from project config

## Description

When `local_prefix` is omitted from a `[[project]]` entry in `graphify.toml`, Graphify should auto-detect the source root directory instead of requiring users to guess the correct value.

## Motivation

Real-world monorepos have mixed project structures:

| Framework | Source Root | local_prefix needed |
|-----------|------------|---------------------|
| Next.js / Node | `src/` | `src` |
| Expo (React Native) | `app/`, `lib/`, `components/` (root-level) | None — no single prefix |
| Python (FastAPI) | `app/` | `app` |
| Flat packages | `index.ts` at root | None |

The Expo case is particularly problematic: setting `local_prefix = "src"` produces a warning and reduces confidence, but there's no single correct prefix to set. Omitting it currently works (Graphify still scans) but produces leading-dot artifacts in node IDs (see BUG-011).

### Evidence from ToStudy

| Config | Nodes | Confidence | Mangled |
|--------|------:|------------|---------|
| `local_prefix = "src"` (wrong for Expo) | 412 | mean 0.51 | `src.lib.auth` prefix on non-src files |
| `local_prefix` omitted | 392 | mean 0.56 | 5 leading-dot artifacts |

Omitting the prefix improved confidence by 10% but introduced a different class of artifacts.

## Proposed Behavior

When `local_prefix` is absent from a project config:

1. **Scan** `{repo}/` for directories containing source files (`.ts`, `.tsx`, `.py`)
2. **Rank** candidate roots by file count:
   - If `src/` exists and has >50% of source files → use `src`
   - If `app/` exists (Python/Expo convention) → use `app`
   - If multiple directories at root (`lib/`, `components/`, `app/`) → use empty prefix (root-relative)
3. **Log** the auto-detected prefix: `[mobile] Auto-detected source root: (root-level, no prefix)`
4. **Override**: explicit `local_prefix` in config always takes precedence

## Affected Code

- `crates/graphify-extract/src/walker.rs` — add prefix detection before walk
- `crates/graphify-cli/src/main.rs` — propagate auto-detected prefix to extract pipeline
- Config parsing — make `local_prefix` optional (may already be)

## Related

- BUG-009 (done) — added warning for missing `src/` directory
- BUG-011 (done) — mangled node IDs when prefix misapplied to external refs

## Verification (2026-04-13)

- Added runtime auto-detection for omitted `local_prefix` with conservative heuristic:
  - `src` if it holds >60% of eligible files
  - else `app` if it holds >60%
  - else empty prefix
- Added walker unit tests for `src`, `app`, ambiguous layout, and root-level files
- Added integration tests for auto-detected prefix and explicit empty `local_prefix`
- Verified with `cargo test -p graphify-extract` → 154 passed
- Verified with `cargo build -p graphify-cli --bin graphify`
- Verified with `cargo test --test integration_test` → 9 passed
