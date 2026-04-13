---
status: done
completed: 2026-04-13
priority: low
timeEstimate: 30
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - cli
  - report
tags:
  - task
  - bug
  - report
uid: bug-012
---

# fix(report): graphify-summary.json missing communities count per project

## Description

The `projects[]` array in `graphify-summary.json` omits the `communities` count. Each project entry only contains `{cycles, edges, name, nodes, top_hotspot}` — no community cluster count.

This forces consumers to parse individual `architecture_report.md` files (or `analysis.json`) to get the community count, defeating the purpose of a summary file.

## Evidence

Current `graphify-summary.json` project schema:
```json
{
  "cycles": 0,
  "edges": 15995,
  "name": "pkg-api",
  "nodes": 11449,
  "top_hotspot": {
    "id": "src.shared.domain.errors",
    "score": 0.69
  }
}
```

Expected (adding `communities`):
```json
{
  "communities": 514,
  "cycles": 0,
  "edges": 15995,
  "name": "pkg-api",
  "nodes": 11449,
  "top_hotspot": {
    "id": "src.shared.domain.errors",
    "score": 0.69
  }
}
```

The community count IS computed and written to each `architecture_report.md`, so the data exists — it's just not propagated to the summary.

## Fix Approach

In `write_summary()`, include the community count from each project's Louvain analysis result when building the per-project stats object.

## Affected Code

- `crates/graphify-cli/src/main.rs` — `write_summary()` (project stats serialization)
- Possibly `crates/graphify-analysis/src/lib.rs` — ensure community count is returned in the analysis result struct

## Impact

- Low severity — workaround exists (read individual reports)
- Affects documentation workflows that generate architecture tables from the summary
- Confirmed in ToStudy monorepo (Graphify v0.2.0, 16 projects)

## Verification (2026-04-13)

- Added `communities` to each `projects[]` entry in `graphify-summary.json`
- Added regression test: `test_multi_project_summary_includes_communities_per_project`
- Verified with `cargo test --test integration_test` → 6 passed, 0 failed
