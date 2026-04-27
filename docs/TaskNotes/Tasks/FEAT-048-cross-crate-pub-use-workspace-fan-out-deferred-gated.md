---
uid: feat-048
status: open
priority: low
scheduled: 2026-04-26
timeEstimate: 90
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
  uncertainty: high
  hintsInferred: false
---

# Cross-crate pub use workspace fan-out (deferred, gated)

Workspace-wide cross-crate `pub use` collapse, gated behind `[settings] cargo_workspace_reexport_graph = true`. Mirrors the TS FEAT-028 architecture for Cargo workspaces. Deferred — schedule only if dogfood evidence shows the cross-crate pattern is common enough to justify the lift.

## Description

Surfaced by FEAT-044 spike. The per-project walker from FEAT-046 cannot reach a canonical declaration that lives in a different `[[project]]` — the walker's `is_local_module` callback returns `false` and the chain terminates as `Unresolved`. Today, consumers cover the cross-crate noise via `external_stubs`. This task would build a workspace-wide `CargoWorkspaceReExportGraph` analogous to `WorkspaceReExportGraph` (TS), plus a Cargo-dependency-aware alias resolver analogous to `apply_ts_alias_workspace`.

In the graphify workspace, intra-crate `pub use` outnumbers cross-crate ~25:1. The cross-crate case is real but rare ("facade re-export"). FEAT-048 should land only after dogfood evidence post-FEAT-046 shows it moves the needle.

## Open questions (defer until scheduled)

- How does Cargo's `[dependencies]` declaration map to module-id lookup in our resolver?
- Does the gate need a per-project opt-in or is workspace-level sufficient?
- Should the cross-crate canonical id surface in public node ids (`graph.json`) or stay inside the workspace registry (mirrors TS FEAT-028 decision)?

## Subtasks

- [x] Decision checkpoint: run `graphify suggest stubs` post-FEAT-046 and count remaining cross-crate misclassifications. Schedule this task only if the count is meaningful (say, ≥5 across the workspace).
- [x] Write ADR documenting the gate (`[settings] cargo_workspace_reexport_graph = true`) — template: `docs/adr/0001-workspace-reexport-graph-gate.md`
- [ ] Build `CargoWorkspaceReExportGraph` data structure in graphify-extract
- [ ] Build Cargo-dependency-aware alias resolver
- [ ] Pipeline integration in graphify-cli (mirror FEAT-028's two-phase split: `build_project_reexport_context` + `run_extract_with_workspace`)
- [ ] Tests: at least one synthetic 2-crate workspace fixture with a cross-crate `pub use`
- [ ] CHANGELOG + CLAUDE.md update

## Out of scope

- Type-alias collapse
- Wildcard `pub use foo::*;` cross-crate

## Related

- Parent: FEAT-044 (spike + design)
- Depends on: FEAT-046 (intra-crate must work first)
- Reference: TS FEAT-028 (workspace reexport graph), ADR-0001 (workspace_reexport_graph_gate)
- Risk: estimate is rough; this task likely needs its own multi-day decomposition once scheduled

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
