---
title: "ADR-009: Watch Mode (`graphify watch`)"
created: 2026-04-13
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-010"
tags:
  - type/adr
  - status/accepted
  - cli
supersedes:
superseded_by:
---

# ADR-009: Watch Mode

## Status

**Accepted** — 2026-04-13

## Context

During active development, users had to manually re-run `graphify run` after every code change to see updated reports. This broke the inner-loop feedback. We wanted a single command that **watches source files** and re-runs the pipeline on change, leveraging [[ADR-003 SHA256 Extraction Cache]] to make rebuilds cheap.

## Decision

**Chosen option:** Add `graphify watch` using the `notify` v7 crate (`RecommendedWatcher` — FSEvents on macOS, inotify on Linux) with `notify-debouncer-mini` for a 300ms debounce window. **Re-runs the full pipeline** for affected projects on change — leans on the SHA256 cache rather than implementing incremental graph patching.

Per-project path matching means a TS edit only re-extracts the TS project. `--force` applies to the **initial** rebuild only; subsequent rebuilds always use the cache. Output directory is excluded from watching to avoid feedback loops. Config file changes are **not** watched — the user must restart.

## Consequences

### Positive

- Sub-second feedback after first cold build (cache pays off)
- Cross-platform via `notify`'s automatic backend selection
- 300ms debounce handles IDE auto-save, formatter-on-save, `git checkout` correctly
- Per-project rebuild scope keeps wall-clock low on monorepos
- Built on existing pipeline — no new code paths to maintain
- No incremental graph patching means no stale-state bug surface

### Negative

- Full pipeline rebuild per change — heavy projects can still feel slow
- Config changes require manual restart
- File-watch tests are inherently flaky (timing-dependent) — relies on manual verification
- Stderr output during rebuild can interleave with the user's terminal during fast typing
- HTML report not hot-reloaded in browser (browser refresh required)

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Full pipeline + cache** (chosen) | Simple, correct | Bigger work units than incremental |
| Incremental graph patching | Theoretically faster | Complex; high stale-state risk |
| Two-tier (extract + lazy analyze) | Tunable | UX complexity for marginal gain |
| Polling instead of `notify` | No native deps | High CPU; noisy |
| External tool (`watchexec`) | Reuses existing binary | One more dep for users to install; doesn't know about excludes |

## Plan de Rollback

**Triggers:** `notify` v7 produces spurious events on a future macOS/Linux version; or the debounce window proves wrong for common workflows.

**Steps:**
1. Remove `Commands::Watch` from `graphify-cli`
2. Remove `notify` and `notify-debouncer-mini` from `Cargo.toml`
3. Document a one-liner alternative in README: `find ./src | entr -r graphify run`

**Validation:** `graphify --help` no longer lists `watch`. Pipeline commands unaffected.

## Links

- Spec: `docs/superpowers/specs/2026-04-13-feat-010-watch-mode-design.md`
- Plan: `docs/superpowers/plans/2026-04-13-feat-010-watch-mode.md`
- Task: `[[FEAT-010-watch-mode]]`
- Related ADRs: [[ADR-003 SHA256 Extraction Cache]] (dependency)
