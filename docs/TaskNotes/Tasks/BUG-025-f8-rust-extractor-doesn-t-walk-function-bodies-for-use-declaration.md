---
uid: bug-025
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
  uncertainty: low
  hintsInferred: true
---

# F8: rust extractor doesn't walk function bodies for use_declaration

The Rust extractor's `extract_file` only iterates `tree.root_node().children(...)`, so `use` declarations inside function bodies (or any other non-top-level scope) are invisible to the extraction pass. Surfaced by the BUG-023 close: `apply_suggestions` in `crates/graphify-cli/src/main.rs` opens with `use toml_edit::{Array, DocumentMut, Item, Table, Value};` inside the function body, and `Item::Table(...)`, `Array::new()`, `Value::Array(...)` calls land as external because no use_aliases are registered. Continuation of BUG-022 Cat 2 (split out of BUG-023's case 2).

## Description

After BUG-023 fixed nested top-level `scoped_use_list` recursion, the remaining `Item`/`Array`/`Value` candidates in `graphify suggest stubs` (3+2+2 edges) all trace to one function-scoped use-statement in graphify-cli's `apply_suggestions`. The fix needs to broaden extractor traversal — two approaches:

1. **Recurse into `function_item` bodies** to look for `use_declaration`. Threads the use-extraction logic through one more AST level. Localized but adds an entry point that didn't exist before.
2. **Post-walk the entire AST** for `use_declaration` nodes anywhere. Simpler conceptually but changes the contract (use-declarations no longer always come from the top level — affects the `line` recorded, and may surprise resolver assumptions).

Option 1 is the safer first step. Option 2 would also catch `use` inside `impl` blocks, `mod` blocks (if anyone writes them in-line), match arms, etc. — likely overkill until a real consumer needs it.

## Subtasks

- [x] Add a failing extractor test: `fn f() { use foo::Bar; Bar::new(); }` registers `Bar → foo::Bar` in `use_aliases` and emits an `Imports` edge to `foo::Bar`
- [x] Decide between option 1 (recurse into `function_item` bodies) and option 2 (post-walk entire AST) — **option 1**: per-scope walker `walk_for_uses` that mirrors `walk_for_bindings` (BUG-024) in skip discipline. Option 2 was rejected for the contract reason in the body — and because walking the whole tree would bypass the lexical-scope hygiene that BUG-024 specifically established
- [x] Implement chosen approach
- [x] Re-run `graphify suggest stubs`, expect `Item`, `Array`, `Value` removed (down from 3/2/2 edges) — all three removed; collapsed under `toml_edit` (13 edges) once use_aliases populated
- [x] Verify no regressions: `cargo test --workspace`, `graphify check --config graphify.toml` on this repo — 856 tests pass, all 5 crates PASS with 0 cycles, hotspot scores identical

## Resolution

Implementation: `crates/graphify-extract/src/rust_lang.rs`. New helper `walk_for_uses(node, source, module_name, result)` recursively walks a function/method body for `use_declaration` nodes and dispatches each to the existing `extract_use_declaration` (which already handles all use-shapes: scoped, grouped, aliased, wildcard). Skip discipline mirrors `walk_for_bindings` from BUG-024 — `function_item` and `impl_item` subtrees return without descending so a `use` inside a nested fn does not leak its alias into the outer function's file-wide map.

Wired into two call sites:

1. `extract_function_item` — after `collect_local_bindings`, before `extract_calls_recursive` (order matters: aliases must be in `result.use_aliases` before the resolver pass runs, but call extraction itself doesn't consult them — so this ordering is just a readability convention, not a correctness requirement).
2. `extract_impl_item` — same ordering inside the per-method body loop.

Approximation accepted (documented in the helper docstring): aliases land in the file-wide `result.use_aliases` map, so a function-scoped `use foo::Bar;` becomes visible to other functions in the same file. In practice harmless — same-file shadowing of an aliased name is rare, and last-write-wins behaves sensibly when both functions import the same path. A truly per-scope alias map would require threading a `HashMap` through the resolver pass, which is a v2 refactor with no current consumer.

Tests added (`bug_025_*` in `crates/graphify-extract/src/rust_lang.rs::tests`):

1. `bug_025_function_scoped_use_emits_imports_edge` — minimal: `fn build() { use foo::Bar; }` produces an `Imports` edge to `foo::Bar`.
2. `bug_025_function_scoped_use_registers_alias` — `use_aliases["Bar"] == "foo::Bar"` after the same input.
3. `bug_025_function_scoped_grouped_use_decomposes` — exact canary from the BUG-022 dogfood: `fn apply() { use toml_edit::{Array, DocumentMut, Item, Table, Value}; }` emits 5 imports + 5 aliases. This is the regression guard against the original bug.
4. `bug_025_method_scoped_use_registers_alias` — same fix reaches impl-method bodies.
5. `bug_025_nested_fn_use_does_not_leak_to_outer` — lexical scope hygiene: a `use` inside `fn inner() { ... }` (nested in `fn outer()`) must NOT register an alias visible to `outer`. Load-bearing — a "fix without skip" would silently pass tests 1–4 but fail this one.

Self-dogfood: `graphify suggest stubs` candidate count 9 → 7 (-2 net prefixes) — the underlying drop is 3-into-1: `Item`, `Array`, `Value` all collapsed into the canonical `toml_edit` prefix (13 edges, now showing full paths like `toml_edit::Array` instead of bare `Array`). `toml_edit` itself is a legitimate per-project external for graphify-cli (toml editor crate, only used by `--apply` mode); the suggest output now correctly recommends adding it to graphify-cli's `[[project]].external_stubs`.

Out of scope: nested `function_item` extraction itself. A `fn outer() { fn inner() { ... } }` produces no `Defines` for `inner` and no Calls captured inside `inner`. The BUG-025 fix only addresses `use_declaration` walking — making nested fns first-class would require descending into their bodies for full extraction, which has different scope and naming implications (do nested fns get their own Defines? what's the fully-qualified id?). File as a separate task only when a real consumer surfaces it.

Architecture: `graphify check` PASS on all 5 crates after rebuild. Counts shifted slightly (graphify-extract +3 edges from the new helper itself contributing imports it couldn't before, graphify-core +1 edge), no cycles, no policy violations, hotspot scores identical to baseline.

Workspace tests: 856 total pass (was 851 — +5 BUG-025 tests).

## Related

Surfaced by BUG-023's close note. Direct continuation of BUG-022 Cat 2 ("function-scoped use blind spot"). Walker discipline mirrors BUG-024 (`walk_for_bindings` per-scope skip pattern). The remaining suggest-stubs candidates after BUG-025 trace to FEAT-044 (Rust re-export collapse — `src.Community`, `src.Cycle`, `src.install.copy_plan.INTEGRATIONS`) and the still-out-of-scope macro/stdlib heuristics (`matches!`, `env`).

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
