---
uid: bug-023
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

# F5: rust extractor preserves nested scoped_use_list as literal text including braces

The Rust extractor's `collect_use_paths` handles `scoped_use_list` for the outer level but, when an item INSIDE the list is itself a `scoped_use_list` (`use foo::{bar::{baz, qux}}`), it captures the inner group's literal source text — `bar::{baz, qux}` — and emits a single edge with that as the target. No aliases are registered for the inner names. Surfaced by the FEAT-043 dogfood: graphify-cli's `use graphify_extract::{cache::{sha256_hex, CacheStats, ExtractionCache}, walker::detect_local_prefix, ...}` produces a graph node `graphify_extract::cache::{sha256_hex, CacheStats, ExtractionCache}` with the curly braces preserved.

## Description

Falls out of the BUG-022 investigation as Cat 3. After fixing the resolver (case 8.6 for scoped same-module), `ExtractionCache::load`, `Item::Table`, `Array::new`, `Value::Array` etc. are still misclassified because the use-aliases for `ExtractionCache`, `Item`, `Array`, `Value` are never registered.

Two distinct cases share this shape and both need the recursion fix:

1. Nested grouped imports at the top level (the cache example above).
2. Function-scoped `use toml_edit::{Array, DocumentMut, Item, Table, Value};` inside `apply_suggestions` in graphify-cli main — the extractor only walks `root.children`, not function bodies.

Case 1 is the higher-impact fix and has a clear scope: when `collect_use_paths` encounters a child of kind `scoped_use_list`, recurse into it with the combined prefix, instead of capturing its text. Case 2 needs broader extractor traversal and is tracked as a separate follow-up if it persists after case 1 lands.

Reference (post-fix `graphify suggest stubs` candidates): `ExtractionCache` 7 edges, `Item`/`Array`/`Value` 7 edges combined. Should drop to 0 once the aliases register correctly.

## Subtasks

- [x] Add a failing extractor test: nested `use a::{b::{c, d}}` produces 2 edges (`a::b::c`, `a::b::d`) and 2 use_aliases entries
- [x] Patch `collect_use_paths` `scoped_use_list` arm to recurse with combined prefix instead of capturing text
- [x] Decide whether to also walk function bodies for `use_declaration` (case 2) or split into a separate task — split into BUG-025 (case 2 needs broader extractor traversal: only `root.children` is walked today, and changing that touches more code paths than the recursion fix)
- [x] Re-run `graphify suggest stubs` on this repo, expect `ExtractionCache`, `Item`, `Array`, `Value` removed — `ExtractionCache` (case 1) gone; `Item`/`Array`/`Value` (case 2, function-scoped) persist exactly as predicted by the split

## Resolution

Implementation: `crates/graphify-extract/src/rust_lang.rs`. Refactored the `scoped_use_list` arm of `collect_use_paths` to delegate to a new `process_scoped_use_list(list_node, source, module_name, line, prefix, result)` helper. The helper handles the four child kinds (`identifier|self`, `scoped_identifier`, `use_as_clause`, `scoped_use_list`) with a shared `join` closure that combines the carried `prefix` with each leaf. The `scoped_use_list` arm fetches the child's inner `path` field, builds `combined_prefix = prefix::inner_path`, and recurses with the inner `list` field — instead of grabbing `child.utf8_text()` (which preserves `bar::{c, d}` braces).

Tests added (`bug_023_*` in `crates/graphify-extract/src/rust_lang.rs::tests`):

1. `bug_023_nested_scoped_use_list_decomposes` — minimal reproduction `use a::{b::{c, d}}` → 2 edges + 2 aliases.
2. `bug_023_nested_scoped_use_list_mixed_siblings` — dogfood shape `use foo::{bar::{baz, qux}, other}` → 3 edges + 3 aliases (verifies nested + flat siblings coexist).

Self-dogfood: `graphify suggest stubs` candidate count 18 → 14. `ExtractionCache` (was 7 edges) fully gone. `Item`/`Array`/`Value` reduced from 7 combined → 7 combined (all from `apply_suggestions`'s function-scoped `use toml_edit::{...}`, which the extractor never visits — case 2, now BUG-025).

Architecture: `graphify check` PASS on all 5 crates, max_hotspot scores identical to baseline (0.486/0.435/0.454/0.452/0.600), 0 cycles, 0 policy violations.

Workspace tests: 313 pass in graphify-extract (was 311, +2 new BUG-023 tests). All other crates unchanged.

Out of scope (filed as BUG-025): walking function bodies for `use_declaration` so function-scoped `use toml_edit::{Array, DocumentMut, Item, Table, Value}` inside `apply_suggestions` registers aliases too. The current `extract_file` only walks `tree.root_node().children(...)`; supporting function-scoped use-statements means either threading the extractor walk into `function_item` bodies or post-walking the AST for `use_declaration` nodes anywhere. Risk profile is different from the recursion fix and warrants its own TDD pass.

## Related

- Surfaced by BUG-022 root-cause investigation (commit on resolver case 8.6)
- Cat 3 in BUG-022 findings
- Reference file: rust extractor's use-declaration handling (top-level `collect_use_paths`)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
