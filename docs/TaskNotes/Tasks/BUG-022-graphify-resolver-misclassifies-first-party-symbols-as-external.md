---
uid: bug-022
status: open
priority: high
scheduled: 2026-04-26
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- bug
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# F4: graphify resolver misclassifies first-party symbols as external

Running `graphify suggest stubs` on the graphify repo itself surfaced ~30 candidates that are NOT external dependencies — they are first-party symbols (modules, types, functions) inside the workspace that the resolver is marking `is_local=false`. Examples: `PolicyError`, `walker::DiscoveredFile`, `src.Community`, `manifest::sha256_of_bytes`, `find_sccs`, `ExtractionCache`, `src.install.copy_plan.INTEGRATIONS`, `pct`, `matches`, `Item`, `Array`, `Value`, `env`, `install::run_install`, `session::run_brief`, `write_grouped`, `sha256_hex`, `ScoringWeights`, `ExplainPalette`, `HotspotThresholds`.

## Description

Surfaced by FEAT-043 dogfood. The fix is NOT to add these to `external_stubs` (which would silence the symptom). The bug is in the resolver itself — likely related to bare-name resolution, intra-crate `super::`/`self::`, or `Item::*` (toml_edit) shadowing.

**Investigation strategy:**
1. Pick one specific case (e.g. `pct` in graphify-core/graphify-report) and trace from extraction → resolver decision → `is_local` flag
2. Check FEAT-031 (Rust bare-name fallback) and BUG-019 (case-8.5 same-module synthesis) — these are the most likely culprits
3. Once root cause known, decide if it's one bug or several

## Subtasks

- [ ] Pick `pct` as the canary case; reproduce by running `graphify run -p graphify-core --force` and inspecting `graph.json`
- [ ] Trace through `crates/graphify-extract/src/rust_lang.rs` + `resolver.rs` to find where `pct` lands
- [ ] Identify root cause (likely a single category of bug affecting all ~30)
- [ ] Fix + add regression test
- [ ] Re-run `graphify suggest stubs` on this repo, expect ~0 false-positive candidates

## Related

- FEAT-043 task body section "Follow-ups" → F4
- Possibly related: FEAT-031, BUG-019 (CLAUDE.md gotcha section)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
