---
uid: feat-028
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 300
pomodoros: 0
timeSpent: 151
timeEntries:
- date: 2026-04-20
  minutes: 28
  note: source=<usage>; landed step 1 (WorkspaceReExportGraph scaffold) + namespacing ADR; v1 tripwire green; remaining steps 3-7 ~3-5h
  type: manual
  executor: claude-solo
  tokens: 87816
- date: 2026-04-20
  minutes: 22
  note: source=<usage>; slice 2 landed (resolve_canonical_cross_project walker + 7 tests, commit 9f2ba22); remaining steps 4-7 ~2-3h
  type: manual
  executor: claude-solo
  tokens: 94097
- date: 2026-04-20
  minutes: 28
  note: source=<usage>; slice 3 landed (workspace-aware apply_ts_alias_workspace + WorkspaceAliasTarget + module_paths index, commit a15f566); discovered match_alias_target inner-glob limitation (pre-existing, affects step 5 scope); remaining steps 5-7 ~2h
  type: manual
  executor: claude-solo
  tokens: 131898
- date: 2026-04-20
  minutes: 25
  note: source=<usage>; slice 4 P1 landed (inner-glob tsconfig matcher, commit 2904e85); P2/P3 deferred — run_extract refactor needs own 45k-token dispatch (6 call sites); step 5-7 remain
  type: manual
  executor: claude-solo
  tokens: 64977
- date: 2026-04-20
  minutes: 48
  note: source=<usage>; slice 5 landed P2a+P2b+P3 (commits a4f8972 refactor + 60c6a85 workspace-wide fan-out + cd760a1 tripwire inversion); step 7 done, tripwire now feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope; steps 6 (summary regression) + 8 (feature-gate decision) deferred to follow-up
  type: manual
  executor: claude-solo
  tokens: 142248
projects:
- '[[sprint.md|Current Sprint]]'
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

## Status (2026-04-20, session 2026-04-20-1437)

**Feature functionally shipped** — tripwire inverted, cross-project edges emit end-to-end.

| Step | State | Commit |
|---|---|---|
| 1 Workspace aggregation (`WorkspaceReExportGraph` + `ProjectReExportContext`, first-wins `modules_to_project` index) | done | `0fe862b` |
| 2 Namespacing decision (option 2 — stable public ids, workspace lookup map; ADR in module doc-comment) | done | `0fe862b` |
| 3 Workspace `resolve_canonical_cross_project` walker + `CrossProjectResolution` / `CrossProjectHop` | done | `9f2ba22` |
| 4 `ModuleResolver::apply_ts_alias_workspace` + `WorkspaceAliasTarget` + `lookup_module_by_path` | done | `a15f566` |
| 4b `match_alias_target` inner-glob support (blocker found during step 4) | done | `2904e85` |
| 5a `build_project_reexport_context` refactor (separates phase-1 collection from phase-2 fan-out) | done | `a4f8972` |
| 5b Workspace-wide fan-out wiring at `main.rs:1953` (7 call sites: Extract/Analyze/Report/Run/Check/Watch-init/Watch-rebuild; 3 single-project sites kept on legacy path) | done | `60c6a85` |
| 7 Invert tripwire → `feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope` | done | `cd760a1` |
| **6** `graphify-summary.json` `cross_project_edges` regression (benchmark on `parisgroup-ai/cursos`) | **open** | — |
| **8** Feature-gate decision (opt-in `[settings] workspace_graph = bool` vs always-on; currently always-on since tests are green) | **open** | — |

Remaining scope for this task: only steps 6 + 8 below. The original "Description" / "Likely scope" sections further down are retained for historical context.

## Remaining work (follow-up session)

### Step 6 — `graphify-summary.json` `cross_project_edges` regression

Run Graphify before/after this feature on the `parisgroup-ai/cursos` monorepo (same workload as CHORE-003 — pin to a fixed commit, use identical `graphify.toml`). Capture:
- `cross_project_edges` total delta from `graphify-summary.json` (task motivation claimed 2,165 inflated barrel edges should redistribute into per-canonical-symbol fan-in).
- Top-N post-fan-out cross-project destinations (which canonical symbols in shared packages now carry measurable fan-in?).
- Any new cycles introduced (expected: zero).
- Hotspot score movement on canonical symbols in shared packages (e.g. `packages/core/src/Foo`) — should bubble up if the fan-out works as motivated.

Write a dated regression report under `docs/benchmarks/` mirroring `docs/benchmarks/2026-04-20-feat-021-025-cursos-regression.md`. Link from README.

### Step 8 — Feature-gate decision

Currently the workspace-wide ReExportGraph is always built when ≥2 projects AND ≥1 TS project (see `60c6a85`). Single-project and non-TS single-language configs stay on the legacy fast path. Decision needed: is this topology-based gating sufficient, or should there be an explicit `[settings] workspace_graph = false` opt-out for users who hit an edge case?

Tradeoffs to document:
- **Always-on (current state)**: simplest UX, best default; risk = an undiscovered resolver edge case produces wrong edges in some monorepo shape.
- **Opt-in flag default false**: most conservative; requires every monorepo user to opt in, defeats the value of the change.
- **Opt-out flag default true**: compromise — all monorepos get the new behavior, users can disable per-project if a regression surfaces.

Recommendation direction (leave final decision to step 8 implementer): opt-out flag default true, documented in `graphify.toml` comments and README, with a loud stderr line on the first run of a workspace where the flag is at default so users know it's active.

## Description (original design, retained)

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
