---
uid: chore-003
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 120
pomodoros: 0
timeSpent: 12
timeEntries:
- date: 2026-04-20
  minutes: 12
  note: source=<usage>
  type: manual
  executor: claude-solo
  tokens: 62642
contexts:
- benchmark
- regression
- consolidation
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  estimateTokens: 60000
  hintsInferred: false
---

# chore(regression): run FEAT-021/025 on the reference monorepo + record headline deltas

Regression pass on the `parisgroup-ai/cursos` monorepo — the reference workload that motivated GitHub issue #13. Run Graphify before/after FEAT-021 Part B + FEAT-025 land in a published release, record the headline deltas (hotspot scores, cross-project edge counts, consolidation candidate count), and publish the numbers so the ROI story is concrete.

## Description

`CLAUDE.md` lists this as an outstanding follow-up to FEAT-025:

> regression on the reference monorepo

Issue #13 quoted raw workload stats: ~24k LOC, 16 projects, 23k nodes, 43k edges, 1,912 raw consolidation symbols, 74 (~4%) suppressed by the local ignore-file workaround. With FEAT-021 Part B + FEAT-025 shipped, those numbers should move — but nobody has measured how much.

## Motivation

- Validates the end-to-end fix instead of trusting unit tests only.
- Supplies concrete numbers for a release-note / blog-post / README paragraph.
- Detects any regressions that the synthetic fixtures miss (e.g., real-world re-export cycle shapes).

## Likely scope

1. Pin the monorepo at a fixed commit (the same one used for the pre-FEAT-021 measurements, if recoverable; otherwise a fresh snapshot documented in the report).
2. Produce two Graphify runs against the *same* snapshot: one from the pre-FEAT-021 binary (prior tagged release), one from the current release. Use `graphify run` with identical `graphify.toml` in both runs.
3. Diff the two `analysis.json` outputs via `graphify diff` and capture:
   - Top-20 hotspot score delta (expect drops on symbols reachable through barrels).
   - Cross-project edge count delta from `graphify-summary.json`.
   - Consolidation candidate count delta from `graphify consolidation`.
   - Any new cycles introduced (should be zero).
4. Write the numbers into `docs/benchmarks/` (new directory) as a dated markdown report, linked from the README's monorepo recipe.
5. Optional: strip the local `.consolidation-ignore` workaround from `cursos` and confirm the allowlist + alternative_paths path gives parity coverage — closes the loop on issue #13's Band-Aid paragraph.

## Related

- FEAT-021, FEAT-025 — the changes being measured
- GitHub issue #13 (closed) — originating workload and baseline numbers
- DOC-001 — README migration note that this report can cite

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
