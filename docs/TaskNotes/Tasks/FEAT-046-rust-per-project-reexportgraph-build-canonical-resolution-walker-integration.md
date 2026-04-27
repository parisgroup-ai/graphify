---
uid: feat-046
status: open
priority: low
scheduled: 2026-04-26
timeEstimate: 35
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
  uncertainty: med
  hintsInferred: false
---

# Rust per-project ReExportGraph build + canonical-resolution walker integration

Wire the existing TS-only `ReExportGraph` build pass to also consume Rust `ReExportEntry` records (from FEAT-045) and apply canonical-collapse to consumer-side `Imports` and `Calls` edges within the same project.

## Description

Surfaced by FEAT-044 spike. The TS pipeline in `graphify-cli/src/main.rs::run_extract_with_workspace` already builds a per-project `ReExportGraph`, walks `resolve_canonical`, accumulates `barrel_to_canonical` rewrites + `canonical_to_alt_paths`, and rewrites edge targets at consumer call sites. The Rust analogue reuses every piece of that pipeline as-is — what's needed is gating the build pass to also fire on Rust projects and threading the resolver callback to call `apply_local_prefix` on Rust raw targets the same way the TS path does today.

One asymmetry vs TS Part B: `pub use` in Rust does NOT create a barrel symbol node today (the extractor emits an edge, not a node), so the barrel-symbol-node-drop step is a no-op for Rust. The edge-target rewrite at consumer call sites IS load-bearing — that's where `crate::Bar` edges get repointed at `src.foo.Bar`.

## Subtasks

- [ ] Gate the existing `has_ts_reexport_work` block in `run_extract_with_workspace` on `Rust` language presence too
- [ ] Wire a Rust resolver callback that mirrors the TS one (apply local_prefix; lookup in `known_modules`)
- [ ] For each Rust `ReExportEntry`, walk `resolve_canonical` and accumulate `barrel_to_canonical` + `canonical_to_alt_paths`
- [ ] At consumer-side edge resolution, repoint targets matching `barrel_to_canonical` keys to the canonical id (no symbol-node drop)
- [ ] Integration test: 2-file Rust project (`lib.rs` with `pub use foo::Bar;` + `consumer.rs` with `use crate::Bar; fn _test() { Bar::new(); }`) asserts the Calls edge from `consumer` lands at `src.foo.Bar`, not `src.Bar`
- [ ] Verify `cargo test --workspace` and `graphify run --config graphify.toml` baseline holds

## Out of scope

- Cross-crate fan-out → FEAT-048 (gated)
- Type-alias collapse → tracked separately in design doc (FEAT-049 candidate)
- Named-import / consumer-use rewrite → FEAT-047

## Related

- Parent: FEAT-044 (spike + design)
- Depends on: FEAT-045
- Reference: TS FEAT-021 Part B (lines ~2611–2716, ~2912–2925 in graphify-cli/src/main.rs)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
