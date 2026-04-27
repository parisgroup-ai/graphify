# ADR 0002 — Cargo workspace ReExportGraph (deferred)

- **Status**: Deferred (2026-04-26)
- **Feature under gate**: FEAT-048 (NOT shipped — gate did not pass)
- **Decision owner**: solo-dev (Cleiton Paris)
- **Task**: FEAT-048

## Context

FEAT-046 shipped per-project Rust `pub use` re-export collapse (analogous
to TS FEAT-021/025/026). It walks each `[[project]]`'s `ReExportGraph` and
rewrites consumer-side edges to land at the canonical declaration —
**within a single crate**.

FEAT-046 cannot reach a canonical that lives in a *different* crate. The
walker's `is_local_module` callback returns `false` on cross-crate ids, so
the chain terminates as `Unresolved` and the consumer-side edge keeps
pointing at the barrel-scoped name. The TypeScript analogue is FEAT-028
(`WorkspaceReExportGraph`), which fans out cross-project alias imports
across `[[project]]` boundaries.

FEAT-048 was filed as the Rust mirror of FEAT-028: a workspace-wide
`CargoWorkspaceReExportGraph` plus a Cargo-`[dependencies]`-aware alias
resolver, gated behind `[settings] cargo_workspace_reexport_graph = true`.

The task body carries an explicit decision checkpoint as its first
subtask: *"run `graphify suggest stubs` post-FEAT-046 and count remaining
cross-crate misclassifications. Schedule this task only if the count is
meaningful (say, ≥5 across the workspace)."*

This ADR records the result of that gate check and defers the feature.

### Forces

1. **Implementation cost**. The TS analogue (FEAT-028) is one of the
   larger features in the codebase: `workspace_reexport.rs` plus a
   two-phase pipeline split (`build_project_reexport_context` +
   `run_extract_with_workspace`) plus integration tests plus an opt-out
   gate (ADR 0001). The Rust mirror would be similar in size, with
   additional complexity around translating Cargo's `[dependencies]`
   declarations to module-id lookup.
2. **Self-dogfood signal is the only reliable trigger**. Graphify's
   own workspace is the closest thing to representative Rust ground
   truth available right now. Other Rust workspaces are not yet under
   analysis.
3. **Already-covered baseline**. 40 prefixes are pinned in
   `[settings].external_stubs` (the dogfood baseline), and FEAT-043's
   self-dogfood UX rule explicitly warns against masking misclassifications
   by adding internal symbols to `external_stubs`. The remaining
   `suggest stubs` candidate list is the *true* uncovered surface.
4. **Recent root-cause work has compressed the surface**. BUG-022
   (resolver case 8.6, scoped same-module / sibling-mod-from-crate-root)
   and BUG-023/024/025 (nested grouped use, closure local scope,
   function-body use-declarations) brought the dogfood candidate count
   35 → 18 → 7 over recent sessions. The remaining 7 trace to several
   distinct fix shapes — only one of them is a cross-crate `pub use`.

## Gate-check evidence (2026-04-26)

Run on `v0.13.3` (workspace + binary on PATH aligned, verified pre-run).

```
$ graphify run --config graphify.toml --force        # full rebuild
$ graphify suggest stubs --config graphify.toml --format md
7 candidates above threshold (--min-edges=2)
```

Per-candidate classification against the FEAT-048 trigger condition
(*cross-crate `pub use` re-export the per-project walker cannot reach*):

| Candidate | Edges | Crate | Root cause | FEAT-048 fix? |
|---|---:|---|---|:-:|
| `matches` | 5 | cli + extract | Rust `matches!` macro stripping (BUG-026 territory; CLAUDE.md BUG-024 stretch note) | no |
| `toml_edit` | 13 | cli | Function-body `use_declaration` partial coverage / nested forms downstream of BUG-025 | no |
| `env` | 2 | cli | `std::env` bare reference (separate fix shape; CLAUDE.md BUG-024 stretch note) | no |
| `src.install.copy_plan.INTEGRATIONS` | 2 | cli | Symbol-level intra-crate reference; not cross-crate | no |
| `Selector` | 2 | core | Same-file enum reference (`Selector::Group`); intra-crate, resolver-shaped | no |
| `src.Community` | 4 | report | `pub use graphify_core::community::Community;` — **cross-crate `pub use`** | **yes** |
| `src.Cycle` | 4 | report | `pub type Cycle = Vec<String>;` — type alias, **explicitly out of scope** per task body | no (out of scope) |

Independent confirmation: `grep -rn 'pub use graphify_' crates/` returns
**exactly one** match (`graphify_core::community::Community` re-exported
from `graphify-report/src/lib.rs:45`). The entire workspace contains a
single cross-crate `pub use` declaration, contributing 4 misclassified
edges.

**Cross-crate `pub use` count FEAT-048 would resolve: 1 candidate / 4 edges.**

The task body's gate threshold is **≥5 across the workspace**. The
observed count (4 edges, 1 candidate) is below that threshold.

The other six candidates are real misclassifications, but none of them
share FEAT-048's fix shape — they belong to BUG-026 (macro stripping),
the BUG-024 follow-up (`std::env`), function-body `use_declaration`
follow-up downstream of BUG-025, and one resolver-case-8.6 hold-out for
intra-crate scoped enum constructors.

## Options considered

| ID | Policy | Rationale |
|---|---|---|
| A | Ship FEAT-048 anyway | Gate exists for a reason; shipping below-threshold violates the task's own guard and pre-empts root-cause work that would shrink the surface further. |
| B | **Defer with ADR** | Document the gate-check evidence, leave a clear re-open trigger, redirect effort to the higher-value misclassifications. |
| C | Lower the gate to ≥1 | Threshold was chosen deliberately to avoid building a workspace-wide system for a single edge cluster. Lowering it post-hoc defeats the gate's purpose. |
| D | Cover the symptom via `external_stubs` | Adding `src.Community` to `external_stubs` violates the FEAT-043 self-dogfood UX rule (don't mask graphify bugs with stubs). The right surface is the canonical-collapse machinery, not the noise filter. |

## Decision

**Option B — defer with ADR.**

FEAT-048 stays open in TaskNotes (`status: open`) but is not scheduled.
This ADR captures the gate-check evidence so a future session can re-open
without re-deriving the rationale.

## Re-open criteria

Re-schedule FEAT-048 when **any** of the following becomes true:

1. **Dogfood**: graphify's own `suggest stubs` cross-crate `pub use`
   count rises to ≥5 (e.g. a refactor splits a large crate and adds
   façade re-exports, or a new sibling crate introduces a fan-out).
2. **External consumer**: a user-reported analysis on a real Cargo
   workspace shows ≥5 cross-crate `pub use` candidates surfacing as
   misclassified externals. The user's `graphify suggest stubs --format
   md` output is the artifact of record.
3. **Architectural pressure**: the `src.Community` 4-edge cluster is
   *blocking* a downstream feature (e.g. `graphify check` policy rule,
   contract validation, hotspot scoring against a re-exported type) in
   a way the existing `external_stubs` escape hatch cannot resolve.
   Today no downstream feature is blocked.

The first re-open trigger that fires should append a "Re-opened YYYY-MM-DD"
section to this ADR with the new evidence, so the decision history stays
linear.

## Consequences

### Positive

- One-session work focuses on the fix shapes with the largest residual
  edges (BUG-026 macro stripping at 5 edges, the `toml_edit` cluster at
  13 edges, BUG-024 follow-up at 2 edges) — each individually a smaller
  lift than FEAT-048 and each closing more dogfood noise.
- The single legitimate cross-crate hit (`src.Community`, 4 edges) sits
  visibly in the candidate list as a known small artifact, not a hidden
  cost.
- ADR sets a clear, evidence-driven re-open threshold; future sessions
  don't re-debate whether FEAT-048 is "ready."

### Negative

- Users of large Cargo workspaces with extensive façade re-export
  patterns (e.g. workspace-public APIs surfacing crate-internal types
  via `pub use`) currently see those edges classified as externals and
  appearing in `suggest stubs`. Mitigation: the `external_stubs`
  escape hatch covers it cleanly today; it just doesn't *collapse* the
  edge to canonical.
- A small drift: graphify-report's public API surfaces `Community` from
  graphify-core, but `analysis.json` records the edge as if it
  originated at `src.Community` rather than `graphify_core.community.Community`.
  Affects nobody downstream right now; would be a real cost only if a
  consumer started cross-referencing graph node ids against Cargo
  dependency edges, which is not a feature graphify offers today.

### Neutral

- No code change. Gate flag `[settings] cargo_workspace_reexport_graph`
  is **not** introduced today — the gate would be added as part of the
  feature itself when re-scheduled.
- No cache invalidation. Per CLAUDE.md, external-stub classification is
  a post-resolution relabel and never enters the cache; the same
  property would hold for cross-crate canonical collapse when shipped.

## Implementation (when re-scheduled)

Sketch (matches task body subtasks 3-7, NOT done in this session):

- New file `crates/graphify-extract/src/cargo_workspace_reexport.rs`
  mirroring `workspace_reexport.rs` (TS FEAT-028). Public type
  `CargoWorkspaceReExportGraph`.
- Cargo-dependency-aware alias resolver: parse each project's
  `Cargo.toml` `[dependencies]` table, match dep names against sibling
  `[[project]]` `crate_name`s, register a workspace-wide alias map
  `dep_name → target_project.module_id`.
- Pipeline integration in `crates/graphify-cli/src/main.rs`: mirror
  FEAT-028's two-phase split. Phase 1 (`build_project_reexport_context`)
  collects per-project `ReExportEntry`s plus parsed
  `Cargo.toml`. Phase 2 (`run_extract_with_workspace`) fans out
  cross-crate `pub use` chains using the merged graph.
- Gate: `[settings] cargo_workspace_reexport_graph: Option<bool>`,
  default `true` (mirrors ADR 0001 Option B). Absent or `true` enables;
  explicit `false` forces legacy per-project path.
- Test: synthetic 2-crate workspace fixture under
  `crates/graphify-cli/tests/` — `crate_a/lib.rs` with
  `pub use crate_b::Foo;` and `crate_b/lib.rs` with `pub struct Foo;`.
  Assert consumer of `crate_a::Foo` lands canonical at
  `crate_b.Foo`, not `crate_a.Foo`.
- ADR update: append "Re-opened YYYY-MM-DD" section to **this** file,
  not a new ADR.

## Related

- FEAT-044 — Rust re-export collapse spike + design (parent)
- FEAT-045 — Rust `pub use` → `ReExportEntry` emission
- FEAT-046 — Rust per-project canonical collapse (predecessor; this
  ADR's "post-FEAT-046" baseline)
- FEAT-047 — Rust consumer-side `use_aliases` canonicalization
- FEAT-028 — TS workspace `ReExportGraph` (reference architecture)
- ADR 0001 — Workspace-wide `ReExportGraph` gate (TS analogue, the
  template for this ADR's shape)
- BUG-026 (filed only when user-visible) — `matches!` macro stripping;
  competing with FEAT-048 for residual edge surface
