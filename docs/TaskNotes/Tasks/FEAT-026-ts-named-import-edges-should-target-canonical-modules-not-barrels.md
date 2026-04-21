---
uid: feat-026
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 180
pomodoros: 0
timeSpent: 34
timeEntries:
- date: 2026-04-20
  minutes: 34
  note: source=<usage>
  type: manual
  executor: claude-solo
  tokens: 111787
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- extract
- typescript
- barrels
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  estimateTokens: 90000
  hintsInferred: false
---

# feat(extract): TS named-import edges should target canonical modules, not barrels

FEAT-021 Part B collapsed TS symbol edges (`Calls`, `Defines`) to canonical declaration ids via the re-export graph, but module-level `Imports` edges still point at the barrel module they were imported from. Close the remaining gap so named imports (`import { Foo } from '.../index'`) contribute fan-in to the canonical module instead of the barrel.

## Description

After FEAT-021 Part B + FEAT-025 shipped, `CLAUDE.md` notes the outstanding item:

> **module-level `Imports` edges** still point at barrel modules (TS extractor doesn't capture named imports yet)

Today the TS extractor records one module-level `Imports` edge per `import` statement, targeting whatever the import path literally says. For `import { Foo, Bar } from '../entities'` where `entities/index.ts` re-exports from `entities/foo.ts` and `entities/bar.ts`, the resulting edge is `caller -> entities` (the barrel), inflating the barrel's module-level fan-in and leaving `entities.foo` / `entities.bar` visually disconnected at the module layer even though the symbol-layer edges are now correct.

## Motivation

- Hotspot scoring mixes symbol-level and module-level signals; barrels still score as hubs at the module layer after FEAT-025.
- Cross-project edge counts at the module granularity (used by `graphify-summary.json` aggregates) still double-count through barrels.
- Consistency: the symbol layer is canonical post-FEAT-025; the module layer should match.

## Likely scope

1. Capture the named-import *specifiers* in the TS extractor, not just the source path — already partially done for re-export statements in FEAT-021 Part A; extend to regular `import { X, Y } from '...'` and `import X from '...'`.
2. Emit one `Imports` edge per resolved specifier (to the canonical module of each symbol) instead of one edge per import statement.
3. Weight handling: dedup via existing `CodeGraph` weight-increment path; same statement importing 3 specifiers from the same canonical module collapses to one edge with weight 3.
4. `import * as ns from '...'` stays as a single edge to the barrel (no specifiers to fan out). Document this as the intentional v1 boundary.
5. Star re-exports (`export * from '...'`) already walk via the re-export graph — reuse that walker to resolve each specifier's canonical home.
6. Regression: add integration coverage in the existing `graphify-extract` test suite that reproduces the barrel pattern and asserts the module-level fan-in distribution.

## Open questions

- Type-only imports (`import type { Foo } from '...'`) — count with weight 1 or skip? Current extractor counts them; keep behaviour for parity.
- Default imports resolved through a `export { default } from '...'` chain — the re-export graph handles this for symbols; verify the module-level fallback matches.

## Related

- FEAT-021 Part B (`0cf10ed`) — symbol-level collapse
- FEAT-025 — writer fan-out for `alternative_paths`
- GitHub issue #13 (closed) — originating proposal

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
