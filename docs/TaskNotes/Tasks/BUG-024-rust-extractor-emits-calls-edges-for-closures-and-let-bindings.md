---
uid: bug-024
status: open
priority: normal
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
  uncertainty: med
  hintsInferred: true
---

# F6: rust extractor emits Calls edges for closures and bare let-bindings

The Rust extractor walks tree-sitter `call_expression` nodes and emits a Calls edge for every callee, regardless of whether the callee is a real function/method or a local binding. Closures (`let pct = |count: usize| -> f64 {...}; pct(extracted)`) and bare-bound locals get edges that have no canonical Defines target, so the resolver returns them as external. Surfaced by the FEAT-043 dogfood: `pct` (8 edges), `sort_key`, `threshold`, `write_grouped`, `find_sccs` and `sha256_hex` (cross-crate but same shape) showed up as bogus stub candidates.

## Description

Falls out of the BUG-022 investigation as Cat 4. The fix needs scope analysis — distinguishing `let pct = |...| ...` from `fn pct(...)` requires either:

1. Pre-scan the function body for local bindings (`let_statement`, `closure_expression`) and skip Calls whose callee matches a binding.
2. Post-resolution filter: drop Calls edges whose target stays bare (`is_local=false` AND no `::` and no use_alias backing). Risk: also drops legitimate external bare calls like `format` (already handled via external_stubs) — needs care to preserve those.

Option 1 is more precise but touches more code. Option 2 is cheaper but interacts with FEAT-033's hotspot scoring (ExpectedExternal edges).

Stretch: also fix `matches`/`env` (Cat 4 corollary). `matches` is the `matches!` macro; the extractor strips `!` per FEAT-031, so it lands as `matches`. `env` is `std::env` referenced bare. Adding both to the global `external_stubs` would silence them, but per the FEAT-043 close note, that's masking the symptom — the real fix is to recognize macro-call patterns and stdlib references separately.

## Subtasks

- [ ] Decide between option 1 (scope analysis) and option 2 (post-resolution filter)
- [ ] Add failing extractor test: closure binding doesn't produce a Calls edge
- [ ] Implement chosen approach
- [ ] Verify FEAT-033 hotspot deltas stay within acceptable bounds (no new cycles, no large hotspot-score swings)
- [ ] Re-run `graphify suggest stubs`, expect `pct`/`sort_key`/`threshold`/`write_grouped` removed

## Related

- Surfaced by BUG-022 root-cause investigation
- Cat 4 in BUG-022 findings
- Closures live alongside extractor logic for `call_expression` traversal in the rust extractor

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
