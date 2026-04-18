---
uid: feat-025
status: done
priority: normal
scheduled: 2026-04-18
completed: 2026-04-18
timeEstimate: 45
pomodoros: 0
timeSpent: 38
timeEntries:
- date: 2026-04-18
  minutes: 38
  note: source=<usage>
  type: manual
  executor: claude-solo
  tokens: 136306
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

# feat(extract): Part B — rewrite TS edges to canonical ids + writer fan-out for alternative_paths

Wire the re-export graph produced by FEAT-021 Part A into the edge
emission path so TypeScript imports resolve to canonical declaration
ids, then fan the new `Node.alternative_paths` field through every
report writer. Closes the outstanding subtasks from FEAT-021 and lands
the hotspot-score regression on the reference monorepo.

## Description

FEAT-021 Part A (commit `e082c6a`) landed the scaffolding: the `Node`
type gained `alternative_paths: Vec<String>`, `ExtractionResult` gained
a `reexports` field, the TypeScript extractor now captures named and
star re-export statements, and the new `reexport_graph` module exposes
a cycle-safe walker with `Canonical | Unresolved | Cycle` outcomes.
21 new unit tests cover the scaffold, all workspace gates green.

What Part A did **not** do:
- Edges emitted by the extractor still point at the original import
  path, not the canonical declaration. The `reexports` field is
  populated but unread downstream.
- The `alternative_paths` field sits on every `Node` but is always
  empty because nothing writes to it.
- None of the seven report writers serialize the new field.
- No end-to-end regression on a real monorepo has been run.

This task closes the loop.

## Motivation

Without the wiring, FEAT-021 Part A is a no-op at the report level —
hotspot scores, cross-project edge counts, and consolidation
candidates all still suffer the barrel-inflation problem described in
the original FEAT-021 body. The scaffold is only useful once edges
flow through the canonical resolver and the alternative paths reach
analysis consumers.

## Likely Scope

**1. Resolver integration (extract pipeline).**
Hook the re-export graph walker into the per-project extraction step
(the function that runs after per-file extraction completes and
before the `CodeGraph` is constructed). For each `Imports` / `Calls`
edge whose target is a TS module, resolve via the re-export graph;
replace the target with the canonical id; accumulate the walked path
into the canonical node's `alternative_paths` (deduped, order-stable).

Confidence semantics (already specified in FEAT-021 body): direct
chain resolution → confidence stays `Extracted`; broken chain on
unresolved export → downgrade to `Ambiguous` with a stderr diagnostic.

**2. Writer fan-out.**
Every report writer in the `graphify-report` crate needs to emit
`alternative_paths` when non-empty. Additive for all; format-specific
shape decisions below. Order: JSON first (serde derive — free), then
CSV (new column), then Markdown (table cell), then HTML (tooltip /
expand block in the D3 node inspector), then Neo4j Cypher (node
property), then GraphML (data key), then Obsidian (frontmatter array
on each node note).

**3. Hotspot regression.**
Run `graphify run` against the reference TypeScript monorepo (any
local project with ≥10 barrel chains — the FEAT-021 original body
names `pageshell-core/formatters` as one such symbol). Capture the
`analysis.json` before and after this change. Assert: canonical
symbols' in-degree and hotspot scores rise; barrel-path symbols
disappear from the top-20 hotspot list. Document numbers in the
commit message.

**4. Open question resolution.**
FEAT-021 listed three open questions; this task should resolve at
least Q1 (tsconfig paths aliasing through barrels) by picking one
behavior and locking it via a fixture. Q2 (perf budget) should get a
back-of-envelope check — if extraction slows by >20% on the reference
monorepo, consider caching the re-export graph in the extraction
cache (deferred to a follow-up if non-trivial). Q3 (breaking change)
is documentation-only; add a line to the release notes draft.

## Subtasks

- [x] Wire `reexport_graph::resolve_to_canonical` into the
      per-project extraction step so `Imports` / `Calls` edges
      targeting TS modules rewrite to canonical ids.
      _Landed in commit `0cf10ed` (FEAT-021 Part B slice):
      `run_extract` aggregates `ReExportEntry`s → builds
      `ReExportGraph` → walks every TS symbol back to canonical.
      Note: raw edges on collapsed symbol nodes ARE rewritten, but
      `Imports` edges targeting barrel **modules** are NOT — that
      requires named-import capture in the extractor and stays
      deferred below._
- [x] Populate `Node.alternative_paths` from the accumulated walked
      paths (deduped, order-stable, excludes the canonical id itself).
      _Landed in `0cf10ed`._
- [ ] Downgrade confidence + emit stderr diagnostic on
      unresolved-chain fallthrough.
      _Partially: cycle outcome emits the stderr warning; unresolved
      outcome leaves the node as-is with no confidence downgrade.
      Decide behavior + finish in this task._
- [x] Fan `alternative_paths` through the JSON writer (serde derive,
      hide when empty).
      _Landed in `0cf10ed`._
- [ ] Add an `alternative_paths` column to the nodes CSV writer
      (pipe-joined string, empty when absent).
- [ ] Render `alternative_paths` in the Markdown report's node-detail
      section (collapsible bullet list under hotspot entries).
- [ ] Surface `alternative_paths` in the HTML D3 visualization's node
      tooltip / inspector panel.
- [ ] Emit `alternative_paths` as a node property in the Neo4j Cypher
      import script (array type).
- [ ] Emit `alternative_paths` as a GraphML data key (string with a
      documented separator since GraphML arrays are awkward).
- [ ] Emit `alternative_paths` in the Obsidian vault writer as a
      frontmatter array on each node note.
- [ ] **Rewrite `Imports` edges targeting barrel modules to canonical
      modules.** Requires the TS extractor to capture named imports
      (currently only the source module is captured). Explicitly NOT
      done by `0cf10ed` — symbol-level edges collapse, module-level
      `Imports` edges still point at barrel modules.
- [x] Integration fixture: multi-level barrel chain end-to-end,
      asserting the canonical node gets N-1 alternative paths for a
      chain of length N.
      _Landed in `0cf10ed` (`tests/fixtures/ts_barrel_project/`,
      2-level chain). Longer chains (N≥3) still worth adding._
- [ ] Integration fixture: aliased re-export preserves canonical name
      and records the alias in `alternative_paths`.
- [ ] Integration fixture: cyclic re-exports log the warning and
      leave the cycle participants as-is (no rewrite).
- [ ] Regression: on a real TS monorepo, capture before/after
      hotspot top-20 lists; commit both to `docs/regressions/feat-021/`
      (create the directory) with a short `README.md` summarizing the
      shift.
- [ ] Resolve FEAT-021 open question Q1 (tsconfig paths through
      barrels): pick behavior, add fixture, document in task notes.
- [ ] Back-of-envelope perf check on the reference monorepo; note
      the delta in the commit message.
- [ ] Update `CLAUDE.md`'s "## Conventions" bullets to mention the
      canonical-id rewrite + `alternative_paths` as part of the
      graph representation section.
      _Partially done in this session's close (see `CLAUDE.md`
      additions under "Graph representation" for the Part B slice);
      finish when the module-edge rewrite + writer fan-out land._

## Acceptance Criteria

- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace -- -D warnings` passes.
- `cargo test --workspace` passes, including all new fixtures.
- A sample `analysis.json` from the reference monorepo shows at
  least one canonical symbol with a populated `alternative_paths`
  array.
- The top-20 hotspot list no longer contains barrel-path duplicates
  of the same underlying class (verify against the names listed in
  the FEAT-021 motivation section).

## Notes

- Prereq: FEAT-021 Part A (commit `e082c6a`) + Part B slice (commit
  `0cf10ed`). Part B slice wired the resolver into `run_extract`,
  collapsed barrel symbol nodes, populated `alternative_paths`, fanned
  it through the JSON writer, and fixed the
  `resolve_ts_relative` `is_package` bug that blocked the collapse.
  This task picks up the remaining 6 writers, the import-edge rewrite
  (needs named-import capture), the confidence/diagnostic policy, and
  the regression + perf checks.
- Keep scope discipline tight — resist the urge to refactor other
  extractors. `alternative_paths` stays TS-only for v1.
- If the perf delta on the reference monorepo exceeds 20%, stop and
  plan a caching layer (re-export graph in the extraction cache);
  don't ship a slowdown.
- Python barrel equivalence (`from .foo import Bar` in
  `__init__.py`) is explicitly out of scope — track separately only
  if users report the pain.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-021-collapse-barrel-reexports-in-ts-extractor]] — Part A;
  this task picks up the unchecked subtasks.
- [[FEAT-017-classify-top-20-hotspots-as-hub-bridge-mixed-in-report-output]]
  — downstream consumer of the corrected fan-in.
- [[BUG-002-ts-reexport-missing-defines-edge]] — historical gap in
  re-export handling.
- GH issue: <https://github.com/parisgroup-ai/graphify/issues/13>
