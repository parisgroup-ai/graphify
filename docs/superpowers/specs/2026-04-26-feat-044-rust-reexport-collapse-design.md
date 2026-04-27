# FEAT-044 — Rust re-export canonical collapse Design

**Status:** Draft (spike + plan)
**Date:** 2026-04-26
**Author:** spike session (Cleiton + Claude)
**Depends on:** FEAT-031 (Rust `use_aliases` map), BUG-022 (Cat 5 surfacing)
**Reference architecture:** TS FEAT-021 (per-project `ReExportGraph`), FEAT-025 (alternative_paths writers), FEAT-026 (named-import fan-out), FEAT-028 (workspace-wide cross-project fan-out)

## Motivation

The Rust extractor doesn't follow `pub use` re-exports across modules or crates. When `graphify-report/src/lib.rs` declares `pub use graphify_core::community::Community;` and a sibling file does `use crate::Community`, the resolver lands on `src.Community` (after `crate::Community` → strip prefix → re-apply local_prefix). That id isn't a real local symbol — it's a re-export pointer to `graphify_core.community.Community`, which lives in a different `[[project]]`.

Self-dogfood surfaces this as two `graphify suggest stubs` candidates: `src.Community` (4 edges) and `src.Cycle` (4 edges). They look first-party but resolve to non-local placeholders.

This task was filed as the Rust analogue of TS FEAT-021/025/026/028. The 32-minute scope on the `tn` task is for the spike + plan only; implementation is expected to span 3–5 follow-up tasks.

## Spike findings

### Finding 1 — TS architecture maps cleanly to Rust, with one exception

The TS pipeline (`reexport_graph.rs` + `workspace_reexport.rs` + the `run_extract_with_workspace` harness) is structurally reusable:

| TS concept | Rust analogue | Status |
|---|---|---|
| `export … from …` statement → `ReExportEntry` | `pub use foo::Bar;` → new `ReExportEntry` (or extend the existing `use_aliases` map with a `is_pub` flag) | Hook exists: `extract_use_declaration` in `crates/graphify-extract/src/rust_lang.rs` already walks `use_declaration` nodes and populates `ExtractionResult::use_aliases`. Adding a visibility check + a parallel `reexports` channel is mechanical |
| `import { Foo } from '…'` → `NamedImportEntry` | `use crate::Bar;` consumer side | Already captured in `use_aliases` (FEAT-031). No new extractor surface needed — the consumer-side data hook is in place |
| Per-project `ReExportGraph::resolve_canonical` | Same data structure, same algorithm | Reusable as-is. The Rust call sites would feed `(barrel_module, local_name)` pairs derived from `pub use` entries instead of `export … from` entries |
| Cross-project `WorkspaceReExportGraph` (FEAT-028) | Cross-crate fan-out for `pub use graphify_core::…` | The shape works, but the alias-resolution layer differs: TS uses `tsconfig.paths` (`@repo/*` → `../../packages/*`); Rust uses Cargo workspace member declarations + the `extern crate` namespace. Need a new `apply_cargo_alias_workspace` analogue, OR sidestep the alias layer entirely (see Finding 2) |

The exception: TS has a clean barrel-symbol-node concept (the barrel re-publishes a name as a *symbol id* like `src.domain.Course`). Rust's `pub use` doesn't create a barrel symbol node today — the existing extractor emits an `Imports` edge from `module → full_path` only. So the Rust collapse is **edge-only** in the consumer crate, not symbol-node-dropping like TS Part B. That simplifies the writer fan-out: no nodes to drop, only `alternative_paths` to populate on the canonical target (which lives in another crate's project, so it may not even be a node in this run's per-project graph).

### Finding 2 — `Cycle` is a type alias, not a re-export

The two candidates that triggered FEAT-044 (`src.Community`, `src.Cycle`) are **not symmetric**:

- `src.Community` ← `pub use graphify_core::community::Community;` in `graphify-report/src/lib.rs`. This IS a re-export — within scope of FEAT-044.
- `src.Cycle` ← `pub type Cycle = Vec<String>;` in `graphify-report/src/lib.rs`. This is a **type alias**, not a re-export. FEAT-044 will NOT collapse it.

Type-alias collapse is a different mechanism (recognise `pub type X = Y;` declarations, build a separate alias graph, walk to the underlying type). It's conceptually simpler than re-export collapse but still a distinct feature. **Tracked as a follow-up out of scope:** see "Out of scope" below.

This means FEAT-044's self-dogfood win is one `suggest stubs` candidate eliminated (`src.Community`), not two. The remaining `src.Cycle` candidate stays until a separate type-alias feature lands.

### Finding 3 — workspace pub use is sparser than expected

Manual count of `pub use` in the graphify workspace:

| Crate | Count | Cross-crate? |
|---|---|---|
| `graphify-report/src/lib.rs` | 16 | 1 cross-crate (`graphify_core::community::Community`); 15 intra-crate (`pub use json::write_graph_json;` …) |
| `graphify-extract/src/lib.rs` | ~8 | All intra-crate |
| `graphify-core/src/lib.rs` | 1 | Intra-crate (`pub use stubs::ExternalStubs;`) |
| `graphify-cli`, `graphify-mcp` | 0 | n/a (binaries, no public surface) |

Net: in this workspace, **1 cross-crate `pub use` and ~25 intra-crate `pub use` statements**. This shapes Q1 below.

### Finding 4 — partial implementation already exists

The data hook the TS pipeline relies on is half-built for Rust:

- `ExtractionResult::use_aliases` (FEAT-031, `crates/graphify-extract/src/lang.rs:125`) already captures `short_name → full_path` for every `use` declaration, regardless of visibility.
- `extract_use_declaration` (`crates/graphify-extract/src/rust_lang.rs:89`) walks every `use` shape (scoped, grouped, aliased, wildcard, function-body BUG-025).

What's missing:

1. Visibility detection — distinguishing `pub use` from regular `use`. The tree-sitter `use_declaration` node has a `visibility_modifier` child for `pub`. Trivial to add.
2. A parallel `reexports: Vec<ReExportEntry>` channel on `ExtractionResult` analogous to TS, OR a flag on each `use_aliases` entry. The architectural decision is in Q3 below.
3. The pipeline integration — feeding the new entries into the existing `ReExportGraph::build` and threading the canonical-resolution outcomes back into edge rewrites at consumer call sites.

This means the **subtask 4 ("Implement per-project ReExportGraph for Rust")** is smaller than originally feared. The TS architecture is reusable wholesale; the new code surface is roughly:

- ~50 lines in `rust_lang.rs` for the visibility check + reexport entry emission
- 0 lines in `reexport_graph.rs` (reused as-is)
- ~100 lines of pipeline plumbing in `graphify-cli/src/main.rs::run_extract_with_workspace`
- Tests proportional to the above

## Decisions

### Q1 — Does the Rust ecosystem care enough about cross-crate re-exports?

**Decision:** Ship per-project (intra-crate) re-export collapse first. Defer cross-crate fan-out to a separate, gated follow-up.

**Rationale:**
- In this workspace, intra-crate `pub use` outnumbers cross-crate ~25:1. The dominant pattern is `lib.rs` aggregating sibling-module exports for ergonomic `use mycrate::Foo;` imports.
- The cross-crate case (`pub use graphify_core::community::Community;`) is real but rare. It's the textbook "facade re-export" pattern — useful but not where the bulk of the noise is.
- Intra-crate collapse is also cheaper to verify: the canonical declaration site is a node in the same per-project graph, so node-rewrite + `alternative_paths` works the same way TS Part B does. No new workspace plumbing needed.
- Cross-crate collapse is structurally similar to TS FEAT-028 (workspace-wide graph + cross-project resolver), but the alias layer is different (Cargo workspace members vs tsconfig paths). Worth its own design decision rather than coat-tailing.

**Implication for Q3:** The canonical id format question only arises for cross-crate collapse. Intra-crate collapse keeps the per-project `src.…` prefix shape, identical to TS Part B.

### Q2 — Workspace-sibling re-exports: separate fan-out pass or per-project walker?

**Decision:** Separate fan-out pass (mirror FEAT-028 architecture), gated behind `[settings] cargo_workspace_reexport_graph = true` and shipped only after intra-crate collapse stabilizes. **Out of scope for the immediate FEAT-045/046/047 trio**; tracked as FEAT-048.

**Rationale:**
- The per-project walker can't reach a canonical declaration that lives in a different `[[project]]` — the walker's `is_local_module` callback returns `false` and the chain terminates as `Unresolved`. That's already the correct behaviour for cross-crate re-exports today; the consumer's `external_stubs` declaration covers the noise.
- Cross-crate fan-out adds the cost of a workspace-wide graph build pass plus a Cargo-aware alias resolver. Both are non-trivial and should land behind a feature gate analogous to ADR-0001 (workspace_reexport_graph_gate) — different gate name, same ergonomics: fast path for single-crate / non-Cargo-workspace configs, opt-in heavyweight pass when the user wants the full picture.
- Cargo's resolution rules don't have a clean alias layer like tsconfig paths. `pub use other_crate::Foo;` resolves through `Cargo.toml` `[dependencies]` declarations, not file globs. Building a Cargo-aware alias resolver is a meaningful design lift in its own right.

### Q3 — Canonical id format: `src.…` (per-project) or `crate_name.…` (workspace)?

**Decision:** Keep `src.…` (per-project view) for intra-crate collapse. Cross-crate canonical ids would use the workspace-qualified form `{project_name}.{module_id}` ONLY inside `WorkspaceReExportGraph` lookups — public node ids in `graph.json` / `analysis.json` stay per-project to preserve backward compatibility.

**Rationale:**
- This mirrors the TS FEAT-028 decision (see `crates/graphify-extract/src/workspace_reexport.rs` module-level docstring, "Module-id namespacing (open question, step 2 in task body)"). Option (2) won there for the same reason: every existing consumer reading `graph.json` ids would break under option (1).
- Intra-crate collapse never crosses the project boundary, so the canonical id stays `src.community.Community` (the canonical declaration in `graphify-core`'s OWN per-project graph). No change to public ids.
- For the cross-crate case (FEAT-048 deferred), the workspace registry would key on `(project_name, module_id)` — same shape as `WorkspaceReExportGraph::resolve_canonical_cross_project` — and the consumer's per-project edge would still be a single `Imports` edge to a canonical id resolved within the workspace lookup, fanned back into the per-project graph as one extra edge per specifier (mirrors TS FEAT-028's `named_import_edges` accumulator).

## Out of scope

- **Type alias collapse** (`pub type Cycle = Vec<String>;`). Different mechanism, different syntax-tree shape, different graph semantics (edges target the underlying type, not a re-export module). Track as FEAT-049 if it surfaces in dogfood after FEAT-044 lands.
- **Cross-crate `pub use` collapse** beyond the per-project walker. Track as FEAT-048 (gated).
- **Wildcard re-exports** (`pub use foo::*;`). Already excluded by the FEAT-031 boundary — wildcard short-name expansion is v2 across the board. The TS analogue (`export * from '…'`) IS handled in `ReExportGraph::star_edges`, but that relies on knowing the upstream module's exports. For Rust this would require crawling the target module's symbol table; defer.
- **Trait re-exports with method resolution.** `pub use foo::TraitX;` and a consumer call `obj.method()` where `method` comes from `TraitX` is a different problem (trait method dispatch); FEAT-044 only addresses path-style references like `crate::TraitX` or `crate::TraitX::method`.

## Follow-up tasks

The remaining subtasks 4–7 from the FEAT-044 task body split into the following concrete `tn` follow-ups. The user should create these via `tn` CLI in a subsequent session — this design doc is the deliverable, not the task files.

### FEAT-045 — Rust `pub use` extraction + `ReExportEntry` emission
**Estimate:** 25 min, uncertainty=low
**Depends on:** none (data-hook work; `use_aliases` map is the existing template)
**Scope:**
- Extend `extract_use_declaration` in `crates/graphify-extract/src/rust_lang.rs` to detect the `visibility_modifier` child of `use_declaration` and short-circuit non-`pub` entries.
- For each `pub use` entry, emit a `ReExportEntry` (reuse the existing TS struct in `crates/graphify-extract/src/lang.rs`; the shape is language-agnostic). Map `use foo::bar::Baz;` → `ReExportEntry { from_module: <current>, raw_target: "foo::bar", line, specs: [{exported_name: "Baz", local_name: "Baz"}], is_star: false }`.
- For `pub use foo::bar::Baz as Qux;`, set `local_name: "Qux"` and `exported_name: "Baz"` (parity with TS aliased re-export).
- For `pub use foo::{Bar, Baz};`, emit one `ReExportSpec` per leaf (reuse `process_scoped_use_list` recursion).
- Tests: 4-5 cases mirroring `reexport_graph.rs`'s test suite (simple, aliased, grouped, nested grouped, intra-crate canonical chain).
**Out of scope:** wildcard `pub use foo::*;`, function-body `pub use` (not legal Rust), pipeline integration.

### FEAT-046 — Rust per-project `ReExportGraph` build + canonical-resolution walker integration
**Estimate:** 35 min, uncertainty=med
**Depends on:** FEAT-045
**Scope:**
- In `graphify-cli/src/main.rs::run_extract_with_workspace`, gate the existing TS-only `ReExportGraph` build pass on `languages.contains(&Language::TypeScript) || languages.contains(&Language::Rust)`. Reuse `ReExportGraph::build` as-is.
- Wire the resolver callback to call `apply_local_prefix` on Rust raw targets the same way the TS path does today (case 7-equivalent).
- For each Rust `ReExportEntry`, walk `resolve_canonical` and accumulate `barrel_to_canonical` rewrites + `canonical_to_alt_paths` exactly like the TS Part B block does (lines ~2611–2716). The barrel-symbol-node-drop step is a no-op for Rust (`pub use` doesn't create a symbol node today), but the edge target rewrite at lines 2912–2925 IS load-bearing — that's where consumer-side `crate::Bar` edges get repointed at the canonical declaration.
- Tests: integration test that takes a 2-file Rust project (`lib.rs` with `pub use foo::Bar;` + `consumer.rs` with `use crate::Bar; fn _test() { Bar::new(); }`) and asserts the Calls edge from `consumer` lands at `src.foo.Bar`, not `src.Bar`.
**Out of scope:** cross-crate fan-out, type-alias collapse, named-import fan-out (analog of TS FEAT-026 — defer to FEAT-047).

### FEAT-047 — Rust consumer-side fan-out (named-import equivalent)
**Estimate:** 30 min, uncertainty=med
**Depends on:** FEAT-046
**Scope:**
- The consumer-side analog of TS FEAT-026: when a `use` declaration imports a name that's published by a barrel via re-export, the Imports edge should target the canonical declaration module, not the barrel.
- For Rust this is more subtle than TS: the consumer's `use crate::Bar;` already populates `use_aliases` (FEAT-031). The resolver's case-9 fallback then rewrites bare-name calls. We need an additional step: after FEAT-046's `barrel_to_canonical` map is built, also rewrite **alias targets** in `use_aliases_by_module` so `crate::Bar` resolves to `src.foo.Bar` not `src.Bar`.
- Tests: same fixture as FEAT-046 but assert the Imports edge from `consumer.rs` (the `use crate::Bar;` itself, not the call site) targets `src.foo.Bar`.

### FEAT-048 — Cross-crate `pub use` workspace fan-out (deferred, gated)
**Estimate:** 90+ min, uncertainty=high — likely needs its own multi-day decomposition
**Depends on:** FEAT-046 (per-project intra-crate collapse must work first)
**Scope:** workspace-wide `CargoWorkspaceReExportGraph` mirroring `WorkspaceReExportGraph` for TS. Cargo-dependency-aware alias resolver mirroring `apply_ts_alias_workspace`. New ADR documenting the gate (`[settings] cargo_workspace_reexport_graph = true`).
**Decision deferred:** flag as candidate at end of FEAT-046 — only schedule if dogfood evidence shows the cross-crate pattern is common enough to justify the lift.

## Validation plan

For FEAT-045 + FEAT-046 + FEAT-047 (the immediate cluster), the dogfood acceptance criteria are:

1. `cargo test --workspace` — all green.
2. `graphify run --config graphify.toml` — completes without new warnings beyond the existing baseline.
3. `graphify suggest stubs` — `src.Community` candidate disappears from `graphify-report`'s suggestions. (`src.Cycle` remains; that's the type-alias case, intentionally out of scope.)
4. `report/graphify-report/graph.json` — no `src.Community` placeholder node, instead `Community` appears on `graphify_core.community.Community`'s `alternative_paths` (or, given graphify-core is a separate `[[project]]`, the `crate::Community` consumer edges target `graphify_core.community.Community` and the local placeholder is gone).

## Open risks

- **The `use_aliases` map is per-file, not per-scope.** BUG-025 already documented this trade-off. For FEAT-046's consumer-side fan-out (FEAT-047), we lean on the same `register_use_aliases` API to feed `barrel_to_canonical` rewrites. If a file has both `use crate::Bar;` (re-export consumer) and `use other::Bar;` (shadowing import), last-write-wins behaviour will misroute one of them. Defer until dogfood surfaces it.
- **Tree-sitter `visibility_modifier` placement.** Need to verify whether `pub` on a `use_declaration` is a sibling child or wrapped node; FEAT-045's first 5 minutes should be a tree-sitter playground check.
- **Cycle / Unresolved diagnostics.** TS prints `Info:` and `Warning:` messages for unresolved chains and cycles. Rust would inherit the same shape. Acceptable.

## Status of FEAT-044 task

Subtasks 1–3 (spike, decisions, plan) are complete. Subtasks 4–7 remain open and are tracked as the FEAT-045/046/047/(048) follow-up cluster above. The task file stays `status: open`; this dispatch's outcome is `partial`.
