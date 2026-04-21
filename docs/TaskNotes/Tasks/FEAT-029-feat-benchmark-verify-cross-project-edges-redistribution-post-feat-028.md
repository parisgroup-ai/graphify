---
uid: feat-029
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 75
pomodoros: 0
timeSpent: 0
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- benchmark
- extract
- typescript
- workspace
- monorepo
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: med
  estimateTokens: 80000
  hintsInferred: false
---

# feat(benchmark): verify cross_project_edges redistribution on parisgroup-ai/cursos post-FEAT-028

FEAT-028's shipping task body claimed "2,165 inflated barrel edges should redistribute" across the `parisgroup-ai/cursos` monorepo once the workspace-wide `ReExportGraph` fan-out landed, but the quantitative before/after was never run. This task closes the loop: produce a post-FEAT-028 companion to the CHORE-003 benchmark report so the redistribution claim either holds up numerically or gets walked back publicly.

## Description

The CHORE-003 benchmark at `docs/benchmarks/2026-04-20-feat-021-025-cursos-regression.md` measured the combined FEAT-021/025/026 effect on `parisgroup-ai/cursos @ 8ff36cc1` — −17.1% nodes, 0 edge change, top hotspot −89%, zero new cycles, 1,923 canonical nodes carrying 2,321 alternative_paths across 14/16 projects. That report is explicitly pre-FEAT-028 (workspace-wide `ReExportGraph` had not landed yet).

FEAT-028's step 6 — "`graphify-summary.json` `cross_project_edges` regression (benchmark on `parisgroup-ai/cursos`)" — was deferred at the end of the 2026-04-20-1437 session. The follow-up note in the session log read: *"the 2,165 inflated barrel edges should redistribute" claim still unverified quantitatively.* Without a published number, the claim is handwaving.

## Motivation

- FEAT-028 is now shipped and released (`v0.11.0`, `d0f1a3f`). Downstream consumers (skills, `/gf-analyze`, any reference-monorepo analysis) will start seeing cross-project fan-out in their outputs. If the redistribution is smaller than the FEAT-028 task body claimed, or if it introduces unexpected new edges (e.g. spurious fan-outs across sibling projects that never actually shared types), users will notice before we do.
- CHORE-003's benchmark methodology is already in-repo and reproducible — same corpus, same commit pin, same tool invocation. The marginal cost of re-running it is low (one `graphify run` pass at v0.10.x, one at v0.11.0).
- The feature-gate decision in FEAT-030 needs this number as input. If the redistribution is tiny (say <5% of cross_project_edges affected), a cheap opt-in flag is enough; if it's >25%, the flag question becomes moot and we document always-on behavior.

## Likely scope

1. Pin the corpus: `parisgroup-ai/cursos @ 8ff36cc1` — same commit CHORE-003 used. Confirm the repo is still checked out locally (CHORE-003 mentions the clone path; if not, clone fresh).
2. Measure pre-FEAT-028 baseline: check out `graphify` at the commit immediately before FEAT-028's first step (`0fe862b`'s parent) or use a tagged release if one exists. Run `graphify run --config graphify.toml` against the cursos corpus. Capture `graphify-summary.json` → rename to `summary-pre-feat-028.json`.
3. Measure post-FEAT-028 HEAD (`v0.11.0` tag, commit `d0f1a3f`). Same `graphify run` invocation on the same cursos corpus. Capture `graphify-summary.json` → `summary-post-feat-028.json`.
4. Diff the two `graphify-summary.json` payloads focusing on:
   - Total `cross_project_edges` count (the 2,165 claim).
   - Per-source-project distribution of fan-out targets — before FEAT-028, barrel consumers land on raw alias strings (one target per barrel); after, they land on canonical modules across the sibling project. Histogram by source project.
   - Any NEW cycles introduced by cross-project edges (cycle count / SCC size at the workspace level).
   - `alternative_paths` population growth (FEAT-025 writers) for cross-project-collapsed nodes specifically.
5. Write the findings up in a new benchmark report under `docs/benchmarks/` named by date + topic (not FEAT-028-specific, since the report's audience is "anyone evaluating cross-project fan-out"). Cross-reference CHORE-003's report. Include the raw summary JSONs as attachments next to the markdown.
6. If the numbers disagree with the FEAT-028 task body claim by more than ±20%, open a follow-up task to revisit the extractor logic OR edit the FEAT-028 task body / CLAUDE.md note to reflect reality.

## Boundaries / non-goals for v1

- Does NOT run the benchmark against any corpus other than `parisgroup-ai/cursos @ 8ff36cc1`. Generalization to other TS monorepos is a separate task.
- Does NOT add a CI job that runs this benchmark on every PR. Manual one-shot.
- Does NOT change any graphify code. Pure measurement + writeup.
- Does NOT require running FEAT-030's feature flag (if that lands in parallel, run the benchmark with the flag forced on so the post-FEAT-028 number represents the always-on path).

## Acceptance criteria

- A new benchmark report exists under `docs/benchmarks/` with date-prefixed filename, comparing pre-FEAT-028 vs post-FEAT-028 `cross_project_edges` counts on `parisgroup-ai/cursos @ 8ff36cc1`.
- The report cites a concrete number for "edges redistributed" and a histogram or table showing per-project distribution.
- The FEAT-028 session note in `CLAUDE.md` (the paragraph starting "Follow-ups tracked on the FEAT-028 task body...") is updated to reference this benchmark and replace "still unverified quantitatively" with the measured result.
- If the result disagrees materially with the 2,165 claim, a short amendment is added to the FEAT-028 task file under a `## Amendment (post-ship)` heading.

## Related

- [[sprint]] — Current sprint
- [[activeContext]] — Active context
- FEAT-028 — workspace-wide ReExportGraph (parent feature, step 6 deferred to here)
- CHORE-003 — benchmark methodology + pre-FEAT-028 baseline report
- FEAT-021 / FEAT-025 / FEAT-026 — ancestor features benchmarked together in CHORE-003
- FEAT-030 — feature-gate decision, consumes this benchmark's number as input
