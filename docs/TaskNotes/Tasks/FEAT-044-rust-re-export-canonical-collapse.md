---
uid: feat-044
status: open
priority: low
scheduled: 2026-04-26
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: high
  hintsInferred: true
---

# F7: Rust re-export collapse — canonical-id resolution like TS FEAT-021

The Rust extractor doesn't follow `pub use` re-exports across modules or crates. So when graphify-report's `lib.rs` does `pub use graphify_core::community::Community;` and a sibling file does `use crate::Community`, the resolver lands on `src.Community` (after `crate::Community` → strip → apply local_prefix). That id isn't a real local symbol — it's a re-export pointer to `graphify_core.community.Community`. Two re-export targets surface as suggest-stubs candidates (`src.Community` 4 edges, `src.Cycle` 4 edges). They look like first-party but resolve to non-local placeholders.

## Description

Falls out of the BUG-022 investigation as Cat 5. TypeScript already has the equivalent feature (FEAT-021/025/026/028: `ReExportGraph`, canonical-resolution walker, named-import canonicalization, cross-project workspace fan-out). Rust would benefit from the same architecture but the extractor surface is different — `pub use` syntax tree is `use_declaration` with a `pub` modifier, and the same alias-rewrite logic from FEAT-031 already captures most of the data needed. What's missing is the per-project `ReExportGraph` build pass for Rust and the canonical-resolution walker that replaces re-export placeholder ids with the canonical declaration site.

Scope-of-work is significant; this is a multi-day task on the order of FEAT-021 (TS Part A). Open questions before planning:

1. Does the Rust ecosystem care enough about cross-crate re-exports for this to move the needle, or is it a niche concern? (graphify itself triggered it, but most Rust crates don't re-export aggressively.)
2. Are workspace-sibling re-exports (graphify-report → graphify-core) a separate fan-out pass like FEAT-028, or does the per-project walker suffice?
3. Should the canonical id keep `src.` prefix (per-project view) or fully-qualified `graphify_core.community.Community` (workspace view)?

## Subtasks

- [x] Spike: read FEAT-021/025/026/028 implementations, sketch a Rust analogue
- [x] Decide on canonical id format
- [x] Plan task breakdown
- [ ] Implement per-project ReExportGraph for Rust (mirror TS architecture)
- [ ] Add canonical-resolution walker
- [ ] Optional: cross-project fan-out (mirror FEAT-028)
- [ ] Add report writer fan-out for `alternative_paths` (mirror TS Part B / FEAT-025)

## Design doc

- [docs/superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md](../../superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md) — spike findings, decisions for Q1/Q2/Q3, FEAT-045/046/047(/048) follow-up split

## Related

- Surfaced by BUG-022 root-cause investigation
- Cat 5 in BUG-022 findings
- Reference architecture: TypeScript FEAT-021, FEAT-025, FEAT-026, FEAT-028
- Out of scope (filed during spike): `src.Cycle` is a type alias (`pub type Cycle = Vec<String>;`), not a re-export — separate feature, tracked as FEAT-049 candidate

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
