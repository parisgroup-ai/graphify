---
uid: feat-043
status: done
priority: normal
scheduled: 2026-04-26
completed: 2026-04-26
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

# graphify suggest stubs (FEAT-043)

Shipped FEAT-043 — `graphify suggest stubs` post-analysis subcommand that scans `graph.json` per project, groups external references by language-aware prefix, auto-classifies cross-project candidates as `[settings].external_stubs` and single-project candidates per-project, and emits md/toml/json or applies in place via `toml_edit`.

## Description

Spec: `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
Plan: `docs/superpowers/plans/2026-04-26-feat-043-suggest-stubs.md`

Built via subagent-driven-development workflow: 13 tasks, ~14 commits to `main`, all CI gates green.

## Subtasks

- [x] Task 1 — toml_edit 0.22 workspace dep
- [x] Task 2 — suggest module skeleton + extract_prefix + 10 unit tests
- [x] Task 3 — score_stubs + threshold + auto-classify + shadowing + 6 unit tests
- [x] Task 4 — render_markdown + 2 unit tests
- [x] Task 5 — render_toml + 1 unit test
- [x] Task 6 — render_json + 1 unit test
- [x] Task 7 — CLI Commands::Suggest + cmd_suggest_stubs (read-only)
- [x] Task 8 — apply via toml_edit (atomic write, idempotent)
- [x] Task 9 — integration test fixture (2-project graph.json)
- [x] Task 10 — 4 integration tests (md/json/apply/clap-conflict)
- [x] Task 11 — dogfood: applied 5 legitimate stubs (graphify_core/extract/report, include_str, anstyle); flagged ~30 first-party misclassifications as separate graphify-resolver follow-up
- [x] Task 12 — README + CHANGELOG
- [x] Task 13 — fmt + clippy + test + check all green

## Follow-ups (filed informally; not blocking ship)

- F1: `already_covered_prefixes` records `extract_prefix(target)`, broader than the actual matched stub
- F2: Cross-language same-prefix collision in `(project, prefix)` aggregation key
- F3: Move `ExternalStubs` from `graphify-extract` to `graphify-core` to remove `graphify-report → graphify-extract` layer crossing
- F4: ~30 first-party symbols misclassified as `is_local=false` by graphify resolver (surfaced by dogfood; out of scope for FEAT-043)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
