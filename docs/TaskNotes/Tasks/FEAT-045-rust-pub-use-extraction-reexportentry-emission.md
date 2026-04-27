---
uid: feat-045
status: open
priority: low
scheduled: 2026-04-26
timeEstimate: 25
pomodoros: 0
designDoc: '[[docs/superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md]]'
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: low
  hintsInferred: false
---

# Rust pub use extraction + ReExportEntry emission

Extend the Rust extractor to detect `pub use` and emit `ReExportEntry` records, mirroring the TS extractor surface that already feeds `ReExportGraph`. Pure data-hook addition — no pipeline integration in this task.

## Description

Surfaced by FEAT-044 spike (see `docs/superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md`). The Rust extractor currently emits a single `Imports` edge from `module → full_path` for every `use` declaration, regardless of visibility. To enable canonical-collapse, we need to recognize `pub use` separately and feed those entries into the existing language-agnostic `ReExportEntry` channel that TS already populates.

The visibility check is mechanical — tree-sitter's `use_declaration` node carries a `visibility_modifier` child for `pub`. The new emission reuses the existing `process_scoped_use_list` recursion for grouped/nested cases (BUG-023's helper).

## Subtasks

- [x] Tree-sitter playground check: confirm `visibility_modifier` placement on `use_declaration` (sibling child? wrapped node?)
- [x] In `extract_use_declaration` (rust_lang.rs), short-circuit the existing alias-emission path when `pub` is absent and emit `ReExportEntry` when present
- [x] Map `pub use foo::bar::Baz;` → `ReExportEntry { from_module, raw_target: "foo::bar", line, specs: [{exported_name: "Baz", local_name: "Baz"}], is_star: false }`
- [x] Map `pub use foo::bar::Baz as Qux;` → set `local_name: "Qux"`, `exported_name: "Baz"`
- [x] Map `pub use foo::{Bar, Baz};` → one `ReExportSpec` per leaf (reuse `process_scoped_use_list`)
- [x] Tests: 4-5 cases mirroring `reexport_graph.rs`'s test suite (simple, aliased, grouped, nested grouped, intra-crate canonical chain)

## Out of scope

- Wildcard `pub use foo::*;` — defer to v2; same boundary as FEAT-031
- Function-body `pub use` — not legal Rust syntax
- Pipeline integration — that's FEAT-046

## Related

- Parent: FEAT-044 (spike + design)
- Reference: TS FEAT-021 / FEAT-025
- Blocks: FEAT-046, FEAT-047, FEAT-048

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
