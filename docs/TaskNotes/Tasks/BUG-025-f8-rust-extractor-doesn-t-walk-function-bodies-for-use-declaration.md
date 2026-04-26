---
uid: bug-025
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

# F8: rust extractor doesn't walk function bodies for use_declaration

The Rust extractor's `extract_file` only iterates `tree.root_node().children(...)`, so `use` declarations inside function bodies (or any other non-top-level scope) are invisible to the extraction pass. Surfaced by the BUG-023 close: `apply_suggestions` in `crates/graphify-cli/src/main.rs` opens with `use toml_edit::{Array, DocumentMut, Item, Table, Value};` inside the function body, and `Item::Table(...)`, `Array::new()`, `Value::Array(...)` calls land as external because no use_aliases are registered. Continuation of BUG-022 Cat 2 (split out of BUG-023's case 2).

## Description

After BUG-023 fixed nested top-level `scoped_use_list` recursion, the remaining `Item`/`Array`/`Value` candidates in `graphify suggest stubs` (3+2+2 edges) all trace to one function-scoped use-statement in graphify-cli's `apply_suggestions`. The fix needs to broaden extractor traversal — two approaches:

1. **Recurse into `function_item` bodies** to look for `use_declaration`. Threads the use-extraction logic through one more AST level. Localized but adds an entry point that didn't exist before.
2. **Post-walk the entire AST** for `use_declaration` nodes anywhere. Simpler conceptually but changes the contract (use-declarations no longer always come from the top level — affects the `line` recorded, and may surprise resolver assumptions).

Option 1 is the safer first step. Option 2 would also catch `use` inside `impl` blocks, `mod` blocks (if anyone writes them in-line), match arms, etc. — likely overkill until a real consumer needs it.

## Subtasks

- [ ] Add a failing extractor test: `fn f() { use foo::Bar; Bar::new(); }` registers `Bar → foo::Bar` in `use_aliases` and emits an `Imports` edge to `foo::Bar`
- [ ] Decide between option 1 (recurse into `function_item` bodies) and option 2 (post-walk entire AST)
- [ ] Implement chosen approach
- [ ] Re-run `graphify suggest stubs`, expect `Item`, `Array`, `Value` removed (down from 3/2/2 edges)
- [ ] Verify no regressions: `cargo test --workspace`, `graphify check --config graphify.toml` on this repo

## Notes

Reference site for the canary: `crates/graphify-cli/src/main.rs::apply_suggestions` — the use-statement is local to the function on purpose (`toml_edit` is only used for `--apply` mode, and keeping it function-scoped avoids polluting the rest of the binary's namespace). This is idiomatic Rust, so the extractor should support it.

Related: BUG-023 (case 1, nested top-level grouped imports — landed); BUG-022 (root-cause investigation that surfaced both); BUG-024 (closures emitted as Calls — also Cat 4 of BUG-022, separate concern).

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
