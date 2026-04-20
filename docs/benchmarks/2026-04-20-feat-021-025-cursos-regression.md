# FEAT-021 Part B + FEAT-025 regression — `parisgroup-ai/cursos` monorepo

**Run date:** 2026-04-20
**Task:** [CHORE-003](../TaskNotes/Tasks/CHORE-003-run-feat-021-025-regression-on-reference-monorepo-and-record-headline-deltas.md)
**Originating issue:** [GH #13](https://github.com/parisgroup-ai/cursos/issues/13)
**Monorepo under test:** `parisgroup-ai/cursos` at `8ff36cc1a57ca288f11d3bfff91ae99a3a85a877`
**Graphify before:** `v0.9.0` (commit `f05af85`, built fresh in `/tmp/graphify-benchmark/v0.9.0`)
**Graphify after:** `v0.10.0` (current `main`, commit `b39937f` — includes FEAT-021 Part B `0cf10ed` + FEAT-025 `f0292ef`)

## What changed between the two binaries

- **FEAT-020** (`25eabc8`) — `[consolidation]` allowlist section in `graphify.toml` (not ancestor of v0.9.0, so the "before" run used a stripped `graphify-v0.9.0.toml` without that section)
- **FEAT-021 Part A** (`e082c6a`) — TS barrel re-export capture + `alternative_paths` scaffold
- **FEAT-021 Part B** (`0cf10ed`) — barrel collapse to canonical nodes + `is_package` fix in TS relative resolver
- **FEAT-022** (`1be5225`) — `graphify consolidation` subcommand (no v0.9.0 equivalent)
- **FEAT-023** (`700b5ce`) — `[consolidation.intentional_mirrors]` drift suppression
- **FEAT-024** (`d6f916e`) — `pr-summary` annotation of allowlisted / intentional-mirror tails
- **FEAT-025** (`f0292ef`) — `alternative_paths` fan-out through all 6 remaining report writers (CSV, MD, HTML, Neo4j, GraphML, Obsidian)

## Method

```bash
# 1. "After" run (v0.10.0 / current main)
cd /Users/cleitonparis/www/pg/apps/cursos
/Users/cleitonparis/ai/graphify/target/release/graphify run --config graphify.toml --force
# → captured as /tmp/graphify-bench-out/after-report/

# 2. Build v0.9.0 in an isolated worktree (no repo-state changes)
git worktree add /tmp/graphify-benchmark/v0.9.0 f05af85
cd /tmp/graphify-benchmark/v0.9.0 && cargo build --release -p graphify-cli

# 3. "Before" run (v0.9.0 against the same monorepo HEAD)
#    — used a stripped graphify-v0.9.0.toml without the [consolidation] section
#      because v0.9.0 predates FEAT-020
cd /Users/cleitonparis/www/pg/apps/cursos
/tmp/graphify-benchmark/v0.9.0/target/release/graphify run --config graphify-v0.9.0.toml --force
# → captured as /tmp/graphify-bench-out/before-report/

# 4. Drift diff on the highest-impact project (pkg-api)
graphify diff \
  --before /tmp/graphify-bench-out/before-report/pkg-api/analysis.json \
  --after  /tmp/graphify-bench-out/after-report/pkg-api/analysis.json
```

## Headline deltas

### Workload size (all 16 projects aggregated)

| Metric                            | v0.9.0 (before) | v0.10.0 (after) | Delta          |
|-----------------------------------|-----------------|-----------------|----------------|
| **Total nodes**                   | 23,512          | 19,488          | **−4,024 (−17.1%)** |
| Total edges (sum of per-project)  | 43,662          | 43,661          | −1 (~0%)       |
| Total cycles                      | 0               | 0               | 0              |
| Cross-project edges (all kinds)   | 22,252          | 22,117          | −135 (−0.6%)   |
| Cross-project `imports` edges     | 13,718          | 13,602          | −116 (−0.8%)   |

**Read:** FEAT-021 Part B dropped 4,024 barrel nodes (−17.1% of the graph) while preserving edge counts almost exactly — consistent with the design, which rewrites edges to canonical targets rather than deleting them. The tiny cross-project delta (−0.6%) is the edges that now dead-end at a canonical symbol instead of threading through a re-exported barrel alias.

### Per-project barrel collapse

| Project          | Nodes v0.9.0 | Nodes v0.10.0 | Δ        | Edges v0.9.0 | Edges v0.10.0 |
|------------------|-------------:|--------------:|---------:|-------------:|--------------:|
| pkg-resilience   |          159 |            97 | **−39.0%** |          148 |           148 |
| pkg-llm-costs    |           53 |            36 | **−32.1%** |           52 |            52 |
| pkg-api          |       11,143 |         8,126 | **−27.1%** |       15,401 |        15,401 |
| pkg-database     |          682 |           507 | **−25.7%** |        1,190 |         1,190 |
| pkg-validators   |          284 |           217 | −23.6%   |          283 |           283 |
| pkg-logger       |          260 |           204 | −21.5%   |          266 |           266 |
| pkg-types        |          190 |           152 | −20.0%   |          174 |           174 |
| tostudy-core     |          468 |           375 | −19.9%   |          605 |           605 |
| pkg-email        |          374 |           280 | −25.1%   |          723 |           723 |
| mcp-server       |          948 |           830 | −12.4%   |        1,421 |         1,421 |
| pkg-jobs         |          564 |           497 | −11.9%   |        1,053 |         1,053 |
| api              |           34 |            31 | −8.8%    |           42 |            42 |
| web              |        5,873 |         5,658 | −3.7%    |       14,872 |        14,871 |
| tostudy-cli      |          296 |           294 | −0.7%    |          585 |           585 |
| ana-service      |        1,877 |         1,877 | 0.0%     |        5,871 |         5,871 |
| mobile           |          307 |           307 | 0.0%     |          976 |           976 |

The TS-heavy packages with explicit barrel files (`index.ts` re-exporting subdirectories) show the biggest drops; Python (`ana-service`) is untouched as expected.

### `alternative_paths` fan-out (FEAT-025 evidence)

- **1,923 canonical nodes** across 14/16 projects now carry `alternative_paths` (9.9% of the final graph).
- **2,321 aliased paths** were dropped and recorded — every one of these is an import string that previously materialized a distinct graph node.
- `pkg-resilience` (44.3% of nodes carry aliases) and `pkg-llm-costs` (36.1%) are the most concentrated barrel-heavy packages; `pkg-api` carries the largest absolute count at 1,619 aliases across 1,247 canonical nodes.
- Verified via `graph.json` (FEAT-025 emits the field on node_link_data records with serde `skip_serializing_if = Vec::is_empty`).

### Top-20 hotspot score deltas in `pkg-api` (highest-impact project)

Produced by `graphify diff` on `pkg-api/analysis.json`. The drift output reports **27 rising, 10 falling, 9 new hotspots, 9 removed hotspots**.

**Rising (centrality concentrated onto canonical symbols previously hidden by barrels):**

| Node id                                                                  | Before   | After    | Δ          |
|--------------------------------------------------------------------------|---------:|---------:|-----------:|
| `src.modules.modules`                                                    |  0.00154 |  0.24842 | **+0.2469** |
| `src.modules.mentorship.domain.value-objects`                            |  0.00168 |  0.24511 | **+0.2434** |
| `src.modules.mentorship-payments.infrastructure`                         |  0.00000 |  0.20208 | +0.2021    |
| `src.modules.mentorship-payments.infrastructure.adapters.MentorshipSplitCalculator` | 0.00000 | 0.15092 | +0.1509 |
| `src.modules.system-config`                                              |  0.00259 |  0.13633 | +0.1337    |

**Falling (scores previously inflated by barrel-aggregation, now redistributed to true canonical symbols):**

| Node id                                                                  | Before   | After    | Δ          |
|--------------------------------------------------------------------------|---------:|---------:|-----------:|
| `src.shared.domain.errors.DomainError`                                   |  0.36358 |  0.03871 | **−0.3249 (−89%)** |
| `src.modules.course-proposals.infrastructure.services.AnaServiceClient`  |  0.18784 |  0.00255 | **−0.1853 (−98.6%)** |
| `src.shared.domain.errors`                                               |  0.68108 |  0.57496 | −0.1061 (−15.6%)   |
| `src.modules.mentorship.domain.entities.refund`                          |  0.07505 |  0.00443 | −0.0706 (−94.1%)   |
| `src.modules.spark-chat.application.dtos.SparkChatDto`                   |  0.09450 |  0.02731 | −0.0672 (−71.1%)   |
| `src.modules.course-projects.application.services.VariantFlowAccessService` | 0.06862 | 0.00384 | −0.0648 (−94.4%) |
| `src.modules.llm-analytics`                                              |  0.06873 |  0.00639 | −0.0623 (−90.7%)   |
| `src.services.refund-policy`                                             |  0.06191 |  0.00000 | −0.0619            |
| `src.modules.course-proposals.domain.entities.CourseProposal`            |  0.06263 |  0.00485 | −0.0578 (−92.3%)   |
| `src.shared.domain.errors.infrastructure-errors`                         |  0.05072 |  0.00000 | −0.0507            |

**Read:** The `src.shared.domain.errors` barrel used to concentrate ~70% of the project's hotspot score mass on a single aggregator module. FEAT-021 redistributes that onto the actual domain-module canonical nodes (`modules`, `value-objects`, `system-config`) — which is exactly what "hotspot" is meant to capture.

## Consolidation candidates (v0.10.0 only)

`graphify consolidation` (FEAT-022) has no v0.9.0 counterpart, so there is no direct before/after pair. Current-HEAD numbers as a baseline for future runs:

- **Per-project groups:** 3,376 total across the 16 projects (breakdown below)
- **Cross-project aggregate groups:** 780
- **Allowlisted hits:** 0 (the only configured allowlist entry is `logger`, which currently doesn't match any leaf symbol because barrel collapse already rewrote logger references onto `createLogger` / `@repo/logger` canonical nodes — evidence that FEAT-021 resolved the original issue #13 motivator for the allowlist)

| Project          | Candidate groups |
|------------------|-----------------:|
| pkg-api          | 1,665            |
| web              | 940              |
| ana-service      | 164              |
| mcp-server       | 118              |
| pkg-email        | 101              |
| tostudy-core     | 89               |
| pkg-database     | 73               |
| pkg-jobs         | 66               |
| mobile           | 60               |
| pkg-logger       | 46               |
| tostudy-cli      | 43               |
| api              | 6                |
| pkg-resilience   | 2                |
| pkg-llm-costs    | 1                |
| pkg-types        | 1                |
| pkg-validators   | 1                |

### Comparison to issue #13 baseline

Issue #13 reported **1,912 raw consolidation symbols** with **74 (~4%) suppressed** by a pre-FEAT-020 local `.consolidation-ignore` workaround. The schema has since changed (it now counts *groups* of shared leaf names, not raw symbol occurrences), but the same-ballpark numbers are:

- 2026-04-20 / v0.10.0: **780 cross-project groups** (aggregate file)
- 2026-04-20 / v0.10.0: **3,376 per-project groups** (sum)

The issue's "1,912 raw" ≈ roughly between these two in magnitude. The 4% suppression rate no longer applies because the `.consolidation-ignore` workaround isn't used in the current cursos checkout — the `[consolidation]` allowlist in `graphify.toml` is the sanctioned replacement.

## Cycles

Zero new cycles in either run, across all 16 projects. FEAT-021 Part B's `Cycle` handling (log to stderr, leave participants in place, no node drop) was exercised zero times on this workload.

## Unresolved re-export chains

v0.10.0 stderr logged `Info: unresolved re-export chain` warnings on `tostudy-core` and `pkg-jobs` for ~20 symbols originating from barrel chains ending outside the local project (mostly `@repo/*` cross-project re-exports). These are expected per FEAT-021 Part B's `Unresolved` policy: leave the barrel node, no confidence downgrade. They do not affect the headline numbers but are candidates for the planned **FEAT-026** (TS named-import edges to canonical modules).

## Stretch goal — skipped

The task body mentioned optionally stripping the `.consolidation-ignore` workaround from `cursos` to confirm allowlist parity. Not applicable: the current cursos checkout at `8ff36cc1` **has no `.consolidation-ignore` file** — it was already removed in favour of the `[consolidation] allowlist = ["logger"]` section in `graphify.toml`. Parity is the status quo.

## Takeaways

1. **Barrel collapse works as designed.** 17.1% node reduction with edges preserved is the clearest possible signal.
2. **Hotspot scores are now meaningful.** `DomainError` dropping from 0.364 → 0.039 is the single clearest "ROI" data point — previously the score was dominated by barrel-aggregation artifact, not true architectural centrality.
3. **Zero regressions detected** — no new cycles, no exploding edge counts, no crashes on real TypeScript.
4. **FEAT-025's fan-out is real and broadly exercised** — 2,321 aliased paths captured across 14/16 projects, meaning every downstream report consumer (CSV / MD / HTML / Neo4j / GraphML / Obsidian) now carries the canonical-path breadcrumbs on nearly 10% of nodes.

## Reproduction

Full method is the `Method` section above. Intermediate artifacts are in `/tmp/graphify-bench-out/` on the machine this was run on:

- `before-report/` — full v0.9.0 output
- `after-report/` — full v0.10.0 output
- `drift-report.json` / `drift-report.md` — `pkg-api` diff
- `graphify-v0.9.0.toml` — the stripped config used for the before-run

The `/tmp/graphify-benchmark/v0.9.0/` git worktree can be removed with `git worktree remove /tmp/graphify-benchmark/v0.9.0` once the numbers are archived.
