---
uid: bug-024
status: done
priority: normal
scheduled: 2026-04-26
completed: 2026-04-26
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

- [x] Decide between option 1 (scope analysis) and option 2 (post-resolution filter) — **option 1**: pre-scan body for `let_declaration` + nested `function_item` names, skip Calls whose `identifier` callee matches. Option 2 was rejected because it masks future extractor bugs (drops bare-not-stub edges silently, no signal in `suggest stubs`); per the FEAT-043 self-dogfood UX rule, extractor bugs are fixed at the source, not silenced post-resolution
- [x] Add failing extractor test: closure binding doesn't produce a Calls edge
- [x] Implement chosen approach
- [x] Verify FEAT-033 hotspot deltas stay within acceptable bounds (no new cycles, no large hotspot-score swings) — `graphify check` PASS on all 5 crates, max_hotspot scores identical to baseline (0.486/0.435/0.454/0.452/0.600), graphify-core node/edge counts shifted slightly (287→285 nodes, 441→438 edges, 9→10 communities — clean ripples from cleaner edge classification)
- [x] Re-run `graphify suggest stubs`, expect `pct`/`sort_key`/`threshold`/`write_grouped` removed — all 4 gone, plus `find_sccs`, `sha256_hex`, `join` also dropped (additional canaries the original task body didn't enumerate)

## Resolution

Implementation: `crates/graphify-extract/src/rust_lang.rs`. Added a `local_bindings: &HashSet<String>` parameter to `extract_calls_recursive`, plumbed through all 3 call sites. Helper `collect_local_bindings(body, source)` walks the function/method body once via `walk_for_bindings` and collects names from two sources:

1. `let_declaration` with single-identifier `pattern` (closures bound to a name, let-bound function pointers/values).
2. Nested `function_item` names — `fn sort_key(...) { ... }` inside another function body. The extractor's top-level walk only emits `Defines` for root-level items, so nested fns get no canonical target and would otherwise look external.

Descent is per-function: `walk_for_bindings` collects the name of nested `function_item` and `impl_item` then RETURNS without descending. A binding inside a nested fn does not leak into the outer function's set, and a binding in fn A does not shadow a real call in fn B.

The `identifier` arm of `call_expression` adds a single check: skip emission if `local_bindings.contains(&callee)`. The `scoped_identifier` arm is unchanged — `Type::method()` calls don't fit the bare-name false-positive pattern.

Tests added (`bug_024_*` in `crates/graphify-extract/src/rust_lang.rs::tests`):

1. `bug_024_closure_binding_skipped` — minimal reproduction: `let pct = |...| ...; pct(10);` no longer emits.
2. `bug_024_let_binding_skipped_when_called` — let-bound function pointer pattern.
3. `bug_024_real_external_call_still_emitted` — regression guard: bare external call still works alongside an unrelated let-binding.
4. `bug_024_closure_scope_per_function` — scope correctness: binding in fn a() does NOT shadow `pct()` in fn b().
5. `bug_024_method_body_local_binding_skipped` — same scope rule applies inside impl method bodies.
6. `bug_024_nested_fn_item_skipped` — nested `fn sort_key(...)` regression guard. Surfaced during the GREEN cycle when `sort_key` didn't drop as expected from `suggest stubs` — investigation found it was a nested fn (in `crates/graphify-core/src/contract.rs::compare_violations`) rather than a let-binding. Helper extended to collect nested fn names.

Self-dogfood: `graphify suggest stubs` candidate count 14 → 9 (combined with the previous BUG-023 commit: 18 → 9, 50% session-cumulative drop). Removed candidates: `pct` (8 edges), `write_grouped` (2), `join` (4), `threshold` (2), `sort_key` (2), `find_sccs` (implicit), `sha256_hex` (implicit). Total ~16+ edges reclassified.

Out of scope (per task body's "Stretch" section, no new follow-up filed unless user-visible):

- **`matches`** (5 edges, cross-project): `matches!` macro — `extract_macro_invocation` strips the `!` per FEAT-031 grammar, so it lands as bare `matches`. Different fix shape (built-in macro recognizer or grammar-level unstrip).
- **`env`** (2 edges): `std::env` referenced bare. Different fix shape (stdlib heuristic).

Both noted in BUG-024's body as separate concerns; if they become user-visible, file as `BUG-026` (macro-name stripping) and `BUG-027` (stdlib bare reference) respectively.

Architecture: `graphify check` PASS on all 5 crates, max_hotspot scores identical to baseline. graphify-core had small structural ripples (287→285 nodes, 441→438 edges, 9→10 communities) — expected from cleaner edge classification, not a regression.

Workspace tests: 319 pass in graphify-extract (was 313, +6 new BUG-024 tests). All other crates unchanged.

## Related

- Surfaced by BUG-022 root-cause investigation
- Cat 4 in BUG-022 findings
- Closures live alongside extractor logic for `call_expression` traversal in the rust extractor

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
