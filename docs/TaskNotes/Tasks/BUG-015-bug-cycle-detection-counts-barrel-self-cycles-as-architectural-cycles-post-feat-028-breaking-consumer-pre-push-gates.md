---
uid: bug-015
status: done
priority: normal
scheduled: 2026-04-20
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- bug
- feat-028-followup
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# BUG: cycle detection counts barrel self-cycles as architectural cycles post-FEAT-028, breaking consumer pre-push gates

## Repro

Consumer: `parisgroup-ai/cursos` monorepo (16 graphify projects, `src/index.ts` barrel is the npm package entry point in 13 of them).

Before FEAT-028 (graphify 0.10.0):

```
graphify check --max-cycles 0 → PASS for all 16 projects
```

After FEAT-028 (graphify 0.11.0):

```
[pkg-api]      FAIL cycles=500 (100% route through bare `src` barrel)
[pkg-jobs]     FAIL cycles=41  (100% route through bare `src` barrel)
[tostudy-core] FAIL cycles=1   (100% route through bare `src` barrel)
```

Sample cycles from `report/pkg-api/analysis.json`:

```
["src", "src.context"]
["src", "src.trpc", "src.context"]
["src", "src.root", "src.trpc", "src.context"]
```

All cycles have the shape `src → <submodule-A> → <submodule-B>` where `src` barrel re-exports submodule-A, and submodule-A imports from submodule-B which is also re-exported by `src`.

## Root cause

FEAT-028 added workspace-wide cross-project fan-out edges through `ReExportGraph`. For packages with a broad `src/index.ts` barrel (npm entry point), every transitive import between sub-modules now also traverses the bare root `src` node, forming synthetic cycles that don't exist in the underlying code.

Evidence:

- The underlying TypeScript code has zero circular imports (same code passed on 0.10.0).
- 100% of the newly-surfaced cycles include the bare `src` node.
- Real architectural hotspots in the same packages remain healthy (top sub-node in pkg-api is `src.shared.domain.errors.entity-not-found` @ 0.151 after hotspot allowlist).

## Consumer workaround (already shipped)

`graphify.toml` in cursos:

```toml
[consolidation]
allowlist = [
  "logger",
  "src",  # added 2026-04-20 for this bug
]
```

Plus `SKIP_GRAPHIFY=1 git push` escape hatch in the pre-push hook for the cycle axis (allowlist resolves hotspot score but not cycle count). Tracked in cursos as CHORE-1343.

## Suggested fix

Exclude cycles whose only "cycle-making" edge routes through a node matching `[consolidation].allowlist` AND that node is the root barrel of its project (leaf-name matches `local_prefix`).

Two possible implementations:

1. Post-process the cycle list: after Tarjan/SCC, drop any cycle whose nodes minus the allowlisted barrel would be acyclic (re-run cycle detection on the subgraph without the barrel; if it's empty, the original cycle was a barrel artifact).
2. Skip barrel nodes during SCC: if a project's root node (leaf == `local_prefix`) is allowlisted, exclude it from cycle detection entirely. Simpler but coarser.

Option 1 is more surgical; option 2 is faster and probably sufficient for the narrow FEAT-028 regression.

## Scope

- `crates/graphify-analyze` (or wherever cycle detection lives)
- New config field in `[consolidation]` or `[settings]`: `suppress_barrel_cycles = true` — opt-in so existing consumers do not change behaviour silently

## Validation

- [x] Regression test: graph `A → B, B → A` via a barrel node `X` that re-exports both A and B. Assert cycle is filtered when `X` is allowlisted. → `bug_015_find_sccs_excluding_drops_barrel_only_cycle`, `bug_015_find_simple_cycles_excluding_drops_barrel_only_cycle` (`crates/graphify-core/src/cycles.rs`)
- [x] Regression test: graph `A → B → A` directly (no barrel involvement). Assert cycle is still detected even when `X` is allowlisted. → `bug_015_find_sccs_excluding_preserves_direct_cycle`, `bug_015_find_simple_cycles_excluding_preserves_direct_cycle`
- [x] Cursos-shape regression: `src → src.context → src` barrel fan-out cycle is dropped → `bug_015_cursos_like_barrel_cycle_is_dropped`
- [ ] Benchmark on real cursos monorepo: 542 → 0 cycles across pkg-api/pkg-jobs/tostudy-core when `src` is allowlisted. (Consumer-side validation; tracked on cursos CHORE-1343)

## Implementation

- Cycle detection gained `find_sccs_excluding(graph, excluded_ids)` and `find_simple_cycles_excluding(graph, max_cycles, excluded_ids)` in `crates/graphify-core/src/cycles.rs`. Zero-overhead fall-through to the original functions when the exclusion set is empty or contains no known node IDs.
- `[consolidation].suppress_barrel_cycles = true` is the opt-in toggle (default `false`). When `true` AND the project's `local_prefix` is matched by the allowlist, the barrel node ID (equal to `local_prefix`) is removed from cycle detection for that project only.
- Piped through `graphify-core/src/consolidation.rs` (`ConsolidationConfigRaw.suppress_barrel_cycles`, `ConsolidationConfig::suppress_barrel_cycles()`) and `graphify-cli/src/main.rs` (`ConsolidationConfigToml.suppress_barrel_cycles`, `barrel_exclusion_ids(project, consolidation)`).
- All 4 `run_analyze` call sites updated (Analyze, Diff baseline-vs-live, Run pipeline helper, Check). Query engine call site uses an empty exclusion set (query commands do not have the consolidation config in scope; cycle data is not their primary output).
- Debug flag `--ignore-allowlist` already bypasses the full consolidation config — barrel suppression inherits that behaviour for free.

## Consumer follow-up

Once this ships in graphify 0.11.1+:

- Consumer updates `cargo install --path ...` to pull the fix
- Consumer removes `SKIP_GRAPHIFY=1` usage in cursos session notes
- Consumer updates `CLAUDE.md` Graphify version reference
- Consumer closes CHORE-1343
