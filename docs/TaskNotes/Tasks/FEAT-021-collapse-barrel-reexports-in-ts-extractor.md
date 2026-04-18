---
uid: feat-021
status: open
priority: low
scheduled:
pomodoros: 0
contexts:
- extract
- typescript
- hotspots
tags:
- task
- feat
---

# Collapse barrel re-exports to a canonical declaration source (TS)

## Description

The TypeScript extractor treats each import path as a distinct node, so
a single class reached through N `export … from …` chains becomes N
nodes. `tsc --traceResolution` confirms they resolve to the same class
at compile time, but Graphify inflates fan-in, hotspot scores, and
cross-project edge counts by attributing consumers to the import path
rather than the declaration source.

Source: [parisgroup-ai/graphify#13](https://github.com/parisgroup-ai/graphify/issues/13)
(Proposal B — the reporter suggests this as a separate v1.0 milestone
after Proposal A lands as [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]]).

## Motivation

Observed on a real monorepo:

```
src.modules.courses.domain.Course                            ← via index.ts re-export
src.modules.courses.domain.entities.Course                   ← canonical declaration
src.modules.courses.domain.guards.entities.Course            ← deeper re-export chain
src.modules.courses.presentation.domain.entities.Course      ← presentation barrel
```

Consequences today:

- **Hotspot scores inflated**: canonical `Course` may score *lower* than
  a barrel that aggregates many consumers — the wrong signal for refactor
  prioritization.
- **Cross-project double-counting**: `web -> mobile` shows 2,165 edges
  in the monorepo summary; a large share is the same shared symbol
  (`pageshell-core/formatters`) attributed twice via different barrel
  paths.
- **Noisy consolidation candidates**: lexical matchers see N spelling
  variants of the same class and flag them as duplicates.

## Proposed Outcome

TS extractor follows `export ... from ...` chains (including `export *`)
and attributes fan-in to the **declaration source**. Optional metadata
records the alternative import paths that reached it.

```json
{
  "id": "src.modules.courses.domain.entities.Course",
  "in_degree": 27,
  "alternative_paths": [
    "src.modules.courses.domain.Course",
    "src.modules.courses.domain.guards.entities.Course",
    "src.modules.courses.presentation.domain.entities.Course"
  ]
}
```

## Likely Scope

- Two-pass extraction for TS projects:
  1. Pass 1 collects every `export … from …` / `export * from …`
     statement into a re-export graph.
  2. Pass 2 resolves each import to the declaration by walking the
     re-export graph to its source.
- Extractor attributes `Imports` / `Calls` edges to the resolved source,
  not the import path.
- Node metadata gains `alternative_paths: Vec<String>` (additive).
- Handle `export { Foo as Bar }` rename chains (keep the canonical
  declaration name; record the alias in `alternative_paths`).
- Handle cycles in the re-export graph (defensive: stop at first revisit,
  emit a warning).
- Preserve confidence semantics:
  - Direct resolution via barrel chain: confidence stays `Extracted`.
  - Chain broken by unresolved export: downgrade to `Ambiguous`, emit
    diagnostic.

## Subtasks

- [ ] Design re-export graph data structure (in-memory, per-project,
      lives alongside the existing module map).
- [ ] Extend `graphify-extract::typescript` to collect `export … from`
      and `export *` statements in the same AST pass used for imports.
- [ ] Implement re-export graph walker with cycle guard.
- [ ] Update resolver to consult the re-export graph when resolving
      imports.
- [ ] Add `alternative_paths` to `Node` schema (additive, TS-only for
      v1; no-op for other extractors).
- [ ] Update JSON/CSV/Markdown/HTML/Neo4j/GraphML/Obsidian writers to
      emit `alternative_paths` where applicable.
- [ ] Fixture: multi-level barrel chain (domain → domain/index →
      domain/entities/index → entities/Course.ts) with known expected
      canonical target.
- [ ] Fixture: aliased re-export (`export { Foo as Bar }`) preserves
      canonical name.
- [ ] Fixture: cyclic re-exports don't hang, emit warning.
- [ ] Regression: hotspot scores on the reference TS monorepo move in
      the expected direction (canonical symbols up, barrels down).
- [ ] Migration note: consumers of `analysis.json` should prefer
      canonical IDs; `alternative_paths` is informational.

## Open Questions

1. Behaviour under `tsconfig paths` aliasing through barrels. Should
   `@repo/pkg/Course` alias-resolve to the canonical declaration in the
   aliased project, or stay at the alias boundary? (Default proposal:
   resolve through, matching the compile-time behavior.)
2. Cost. Two-pass extraction on a large monorepo (16 projects, 23k
   nodes) needs a perf budget check — ideally ≤20% slower than today.
   Consider caching the re-export graph per project in the extraction
   cache.
3. Breaking change? Node IDs stay stable (canonical was always a valid
   ID). But users who pinned queries against barrel-path IDs will see
   those IDs disappear. Treat as a v1.0 breaking change and document in
   release notes.

## Notes

- Prereq: [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]]
  is lower-risk and tackles the same user pain through a different lens.
  This task assumes A ships first.
- Consumer-side workarounds (regex ignore-lists) partially mitigate the
  symptom but cannot fix the centrality inflation — only this task can.
- Applies to TypeScript first. Python barrel-equivalent (`__init__.py`
  re-exports via `from .foo import Bar`) is out of scope for v1 — track
  separately if users report the same pattern.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]] —
  Proposal A from the same issue; prereq for this task.
- [[FEAT-017-classify-top-20-hotspots-as-hub-bridge-mixed-in-report-output]]
  — hotspot signal consumers that benefit from corrected fan-in.
- [[BUG-002-ts-reexport-missing-defines-edge]] — historical gap in
  re-export handling, different symptom, same area of code.
- GH issue: <https://github.com/parisgroup-ai/graphify/issues/13>
