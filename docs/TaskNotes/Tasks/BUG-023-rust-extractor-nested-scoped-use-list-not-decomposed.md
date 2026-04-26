---
uid: bug-023
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

- [ ] Add a failing extractor test: nested `use a::{b::{c, d}}` produces 2 edges (`a::b::c`, `a::b::d`) and 2 use_aliases entries
- [ ] Patch `collect_use_paths` `scoped_use_list` arm to recurse with combined prefix instead of capturing text
- [ ] Decide whether to also walk function bodies for `use_declaration` (case 2) or split into a separate task
- [ ] Re-run `graphify suggest stubs` on this repo, expect `ExtractionCache`, `Item`, `Array`, `Value` removed

## Related

- Surfaced by BUG-022 root-cause investigation (commit on resolver case 8.6)
- Cat 3 in BUG-022 findings
- Reference file: rust extractor's use-declaration handling (top-level `collect_use_paths`)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
