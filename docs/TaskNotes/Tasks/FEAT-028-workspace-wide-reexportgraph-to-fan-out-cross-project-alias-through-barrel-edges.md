---
uid: feat-028
status: open
priority: normal
scheduled: 2026-04-20
timeEstimate: 300
pomodoros: 0
contexts:
- extract
- typescript
- barrels
- workspace
- monorepo
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: high
  estimateTokens: 180000
  hintsInferred: false
---

# feat(extract): workspace-wide ReExportGraph to fan out cross-project alias-through-barrel edges

Close the cross-project half of FEAT-027. Today each `[[project]]` builds its own `ReExportGraph` and the walker stops at the project boundary, so a consumer project importing `import { Foo } from '@repo/core'` (alias → `../../packages/core/src`) lands on the raw alias string instead of the canonical `Foo` declaration in the core project. Approach A: lift the per-project graph into a workspace-scoped structure so the walker can cross `[[project]]` boundaries when resolving a barrel.

## Description

`tests/fixtures/ts_cross_project_alias/` + integration test `feat_027_cross_project_alias_stays_at_barrel_v1_contract` pin the current v1 contract: the consumer project emits `src.main → @repo/core [Imports]` (raw alias), has zero edges reaching `packages/core`'s internals (`src.foo`, `src.foo.Foo`), and the two graphs are islands. That test is the tripwire — it should **invert** when this feature lands.

The existing pipeline (see `crates/graphify-cli/src/main.rs` `run_extract`) builds a `ReExportGraph` from `all_reexports` inside a single-project scope and walks it via `reexport_graph.resolve_canonical(barrel_module, spec_name, is_local_fn)`. FEAT-026 added the module-layer fan-out for `all_named_imports` on top of that same graph. Both loops are strictly per-project today.

Approach A lifts this by introducing a workspace-scoped analogue:

- The CLI (at the outer loop in `main.rs` that iterates over `[[project]]` entries) collects every project's `(all_reexports, known_modules, module_paths)` triple into a workspace aggregate **before** any project's `run_extract` emits edges.
- A new `WorkspaceReExportGraph` (parallel to the existing per-project graph; probably lives in `graphify-extract` alongside `reexport_graph.rs`) merges them into a single DAG keyed by fully-qualified module id. Module ids that collide across projects (e.g. both `apps/consumer/src/index.ts` and `packages/core/src/index.ts` map to `src.index`) need a per-project prefix — see "Open questions" below for the naming scheme.
- Each project's `run_extract` receives the workspace graph (new parameter) instead of (or alongside) a per-project one. The fan-out loop at `main.rs:1882` now resolves cross-project aliases: for a consumer `@repo/core` import, the resolver still returns a raw alias, but a new step looks up the alias target path in a workspace-wide `path → module_id` index, finds that it resolves into the core project's `src.index` barrel, then walks the workspace graph to `src.foo.Foo` in the core project.
- `is_local_fn` widens its semantics from "local to this project" to "local to this workspace" (everything discovered by any `[[project]]` walker). Callers outside the fan-out loop that use `is_local_fn` for a different reason (e.g. the existing resolver flag in `resolver.rs`) need audit to make sure widening doesn't regress confidence scoring.

## Motivation

- The `code-consolidation` skill (external consumer) ranks shared-kernel candidates by cross-project edge count in `graphify-summary.json`. Today those counts are inflated by barrels — every consumer of `@repo/core` contributes one edge to a fake `@repo/core` node instead of distributing across canonical declarations in core.
- Hotspot scores on a monorepo's shared package today stay artificially low because cross-project edges land on the raw alias, not on the actual declarations. Post-FEAT-028, a monorepo's `packages/core/src/Foo` should bubble up in cross-project hotspots if it's widely used.
- The `parisgroup-ai/cursos` benchmark (CHORE-003 report at `docs/benchmarks/2026-04-20-feat-021-025-cursos-regression.md`) showed `web → mobile` cross-project edges at 2,165 — mostly the same shared `pageshell-core/formatters` attributed through barrels. FEAT-028 is the change that turns that 2,165 into meaningful per-symbol fan-in.

## Likely scope

Timeboxed target: 4–6h (larger than FEAT-026 because this crosses the project-loop boundary). If any sub-step blows up, land as `partial` and split.

1. **Workspace aggregation** — extend the outer project loop in `main.rs` (the one that owns `[[project]]` iteration) to collect `(project_name, known_modules, module_paths, all_reexports)` into a workspace-scoped struct before the per-project pipeline runs. Probably a new type in `graphify-extract` or a plain aggregate struct in `main.rs`.
2. **Module id namespacing** — decide the workspace-key scheme. Two options to evaluate in the task (not pre-committed here):
   - **`{project_name}.{module_id}`** prefix everywhere (e.g. `consumer.src.main`, `core.src.foo`). Clean namespacing, but every existing consumer reading `analysis.json` / `graph.json` ids would break — **not backwards compatible**.
   - **Workspace-scoped lookup map keyed by `(project, module_id) → canonical`**, but public node ids stay `src.foo` etc. Adds complexity to the graph library but keeps `graph.json` stable.
   - Recommendation (for the task author): lean toward (2), document why, add a regression test asserting existing per-project fixtures' node ids don't change.
3. **Workspace ReExportGraph** — build a DAG that understands cross-project edges. The `resolve_canonical` API may need a new entry point `resolve_canonical_workspace(from_project, barrel_module, spec_name) → CanonicalResolution` that returns `(canonical_project, canonical_module)` tuples.
4. **Alias → workspace-target lookup** — the resolver's workspace-alias path (`apply_ts_alias_with_context`) currently returns the raw alias when the target path is outside `self.root`. New path: check if the target falls inside ANY project's root; if yes, return the target project's module id + mark `is_local = true` (at workspace scope). Needs a workspace-scoped resolver or a workspace registry passed in.
5. **Fan-out loop integration** — update the fan-out block at `main.rs:1882` to use the workspace resolver. For the consumer `@repo/core` case: resolver returns `(core, src.index)` → walker walks to `(core, src.foo)` → emit edge `src.main → src.foo` (or the namespaced equivalent per decision in step 2).
6. **Cross-project edge bookkeeping** — `graphify-summary.json` aggregates cross-project edge counts. Verify that edges going into fanned-out canonical targets count correctly; add a regression on the summary's `cross_project_edges` field.
7. **Invert the tripwire** — update `feat_027_cross_project_alias_stays_at_barrel_v1_contract` (rename to `feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope` or similar) to assert the new contract: consumer has `src.main → src.foo` (or namespaced), the `@repo/core` raw node is gone, core project's `Foo` carries `alternative_paths` with the dropped alias ids.
8. **Feature gate** — consider shipping behind `[settings] workspace_graph = true` (default false) for one release so consumers can opt in before it becomes default. Or not, if the risk surface is small. Call this out in the task and let the reviewer decide.

## Boundaries / non-goals for v1

- Does **not** attempt to unify symbol-layer nodes across projects if the same class name exists in two projects (e.g. both projects have a `Config` class). FEAT-028 only walks barrel re-exports — it doesn't merge unrelated same-named declarations.
- Does **not** change the cross-language intentional-mirror contract (FEAT-020/023). That's a separate, orthogonal rebucketing story.
- Does **not** resolve `import X from 'react'` — external npm packages stay non-local regardless of workspace topology.

## Open questions

- Naming scheme (step 2 above): backwards-incompatible full prefix vs lookup-only. Affects every downstream consumer; needs explicit sign-off.
- Cycle semantics: what happens if a cross-project re-export chain cycles? Reuse the existing `Cycle` outcome and stderr diagnostic; verify the walker handles cross-project visited-set correctly.
- Confidence scoring: should edges that cross a project boundary via a workspace alias carry a confidence downgrade (e.g. Ambiguous instead of Extracted) to signal the inferred jump? Likely no — the tsconfig alias is declarative — but worth the explicit decision during implementation.
- Graph serialization: if namespace prefix changes, all 7 writers (JSON, CSV, Markdown, HTML, Neo4j, GraphML, Obsidian) need a parallel audit pass to confirm nothing breaks. Lean on the FEAT-025 fan-out as precedent.

## Related

- FEAT-027 — spike that identified this gap and landed the tripwire regression test
- FEAT-026 — named-import fan-out, the per-project precedent this generalises
- FEAT-025 — writer fan-out for `alternative_paths`, probably reusable here
- FEAT-021 Part B — original barrel-collapse pass, the architectural origin
- CHORE-003 — cursos benchmark, quantifies the business value (2,165 inflated cross-project edges that should redistribute)
- GitHub issue #13 (closed) — Ask B's cross-project case that FEAT-026 + FEAT-027 couldn't fully close

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
