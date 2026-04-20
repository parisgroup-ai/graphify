---
uid: feat-027
status: open
priority: low
scheduled: 2026-04-20
timeEstimate: 120
pomodoros: 0
contexts:
- extract
- typescript
- barrels
- spike
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: high
  estimateTokens: 50000
  hintsInferred: false
---

# spike(extract): resolve `tsconfig.json` paths that traverse barrels to canonical modules

Open question left on FEAT-025: when a `tsconfig.json` `paths` alias points at a barrel (`@scope/pkg` → `packages/pkg/src/index.ts`), FEAT-021 Part B collapses *through* the barrel at the symbol layer, but the alias itself is resolved to the barrel's module id. Decide whether this should be folded into the re-export walker, left as a no-op, or handled specially.

## Description

`CLAUDE.md` notes the unresolved design point:

> tsconfig-paths-through-barrels open question

Today the TS resolver (`resolve_ts_relative` + workspace-alias path) lowers an alias import like `import { Foo } from '@repo/core'` to a module id using the `paths` mapping; that target is typically a `packages/core/src/index.ts` barrel. FEAT-021 Part B's re-export walker then rewrites any *symbol* edge onto that barrel to the canonical declaration, so symbol-level data is correct. But workspace-alias-driven *module-level* `Imports` edges still land on the barrel.

This is a strict subset of the FEAT-026 problem — but with an extra twist: the alias target may cross project boundaries (one `[[project]]` importing from another via workspace alias), which interacts with the cross-project edge bookkeeping used by `graphify-summary.json`.

## Motivation

- If FEAT-026 ships and walks named-import specifiers through the re-export graph, this may already be covered — at which point the task resolves to verification + closing.
- If not, the cross-project aliasing case needs an explicit decision: walk through, annotate with `alternative_paths`, or stop at the barrel and document why.
- Consumers of `graphify-summary.json` (notably the `code-consolidation` skill) count cross-project edges to locate shared-kernel candidates; inflated counts here misrank those candidates.

## Likely scope

Timeboxed spike — 1–2 hours, output is a decision note + a follow-up task (or a "close as no-op" note), not code:

1. Build a synthetic two-project fixture: `project-a` imports `@repo/core/Foo` where the alias resolves to a barrel that re-exports from a deeper module.
2. Run current HEAD against it. Capture the module-level edge (`project-a.something -> core`) and confirm whether FEAT-026 (if landed) fans it out to `core.foo`.
3. Decide:
   - **Covered**: mark this task done, add a regression test that pins the behaviour.
   - **Not covered**: draft FEAT-029 describing the delta (likely: apply the re-export walker to alias-resolved module ids with a cross-project flag), link this task as the source, close this one.
   - **Intentional no-op**: document why in the extractor module (alias barrels are user-declared API surface; walking through them loses that intent). Update README.

## Related

- FEAT-026 — named-import module-level fan-out (may subsume this)
- FEAT-025 — writer fan-out (already covers the symbol layer)
- GitHub issue #13 (closed) — originating proposal, Ask B context

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
