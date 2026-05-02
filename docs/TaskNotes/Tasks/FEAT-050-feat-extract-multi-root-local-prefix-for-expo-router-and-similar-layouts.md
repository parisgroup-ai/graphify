---
uid: feat-050
status: done
priority: normal
scheduled: 2026-05-02
completed: 2026-05-02
timeEstimate: 240
pomodoros: 0
designDoc: '[[docs/superpowers/specs/2026-05-02-feat-049-multi-root-local-prefix-design.md]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# feat(extract): multi-root local_prefix for Expo Router and similar layouts

Released as `v0.14.0`. Spec/plan/commits all use the label "FEAT-049" — that was a misnumbering at session start (the slot was already taken by `FEAT-049-rust-pub-type-alias-collapse`, done 2026-04-27). The actual tn task is this one (`FEAT-050`); cross-reference is in `memory-bank/activeContext.md`. Work shipped under v0.14.0 in 14 commits between `dcefa79` and `7d85e10`.

## What shipped

`[[project]].local_prefix` in `graphify.toml` now accepts either a string (current wrapping behavior, zero breaking change) or an array of top-level dirs (no-wrap, is-local hint). Targets Expo Router and similar mobile/web layouts where source spans parallel root dirs (`app/`, `lib/`, `components/`, ...) without a common parent. Source: GitHub issue #16.

Internal flow: TOML `LocalPrefix { Single, Multi }` collapses to `EffectiveLocalPrefix { prefixes, wrap }` at config-load time; downstream walker / resolver / cache / barrel-exclusion / suggest-stubs consume the effective form. Cache key `prefix.cache_key()` invalidates automatically when shape switches (`"src"` vs `"multi:<sorted>"`). PHP rejects `Multi` fail-fast (PSR-4 already provides per-namespace roots).

Auto-detect emits an advisory stderr warning when ≥2 root dirs each carry ≥10 source files and `top1 < 3× top2` — does not auto-pick the array form (explicit opt-in only).

## Implementation history

11 sub-tasks executed via `superpowers:subagent-driven-development` with per-task spec compliance + code quality reviews:

| # | SHA | Subject |
|---|---|---|
| 1 | `e07a6ec` | LocalPrefix enum + EffectiveLocalPrefix collapse |
| 2 | `0360c5f` | validate_local_prefix + load_config wiring |
| 3 | `a80f88c` | walker `_eff` entry points |
| 4 | `35d8c3a` | resolver `set_local_prefixes(&[String], wrap_mode)` |
| 5 | `a9afd12` | ExtractionCache `_eff` constructors |
| 6 | `ec7d901` | CLI plumb EffectiveLocalPrefix end-to-end |
| 7 | `540494a` | MCP server mirror |
| 8 | `dd9931f` | barrel exclusion + suggest stubs handle multi-prefix |
| – | `b7e1437` | drop dead transitional helper |
| 9 | `d520e05` | auto-detect multi-root advisory warning |
| 10 | `2214c76` | end-to-end Expo fixture integration test |
| 11 | `7d85e10` | release 0.14.0 + CHANGELOG + CLAUDE.md |

End-to-end integration test at `crates/graphify-cli/tests/feat_049_multi_root.rs` builds a temp-dir Expo fixture and proves no-wrap claim + `react` external classification.

## Verification

- Workspace tests: 942 passing, +6 new tests for FEAT-050 across walker/resolver/cache/cli/report
- Dogfood smoke: 5/5 graphify projects PASS, 0 cycles, hotspots byte-identical to pre-feature baseline (`src.server@0.60`, `src.install@0.45`, `src.pr_summary@0.44`, `src.lang.ExtractionResult@0.40`, `src.graph.CodeGraph@0.40`)
- Tag `v0.14.0` created locally; CI release published binaries on push (handled by `release.yml` on tag push)

## Follow-up

`CHORE-012` — hoist `validate_local_prefix` into `graphify-extract::local_prefix` so MCP server picks up the same warnings as the CLI (currently MCP `load_config` is a duplicate that skips validation).

## Related

- Spec: `[[docs/superpowers/specs/2026-05-02-feat-049-multi-root-local-prefix-design.md]]`
- Plan: `[[docs/superpowers/plans/2026-05-02-feat-049-multi-root-local-prefix.md]]`
- GH issue: https://github.com/parisgroup-ai/graphify/issues/16
- Follow-up: `[[CHORE-012]]`
