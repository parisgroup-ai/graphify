---
status: done
completed: 2026-04-12
priority: normal
timeEstimate: 180
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - report
  - cli
tags:
  - task
  - bug
  - report
uid: bug-003
---

# feat(report): cross-project summary is a stub — only writes project names

## Description

The cross-project summary file (`graphify-summary.json`) currently only lists project names without any meaningful aggregate metrics. It should include:

- Total nodes/edges across all projects
- Cross-project coupling (shared module count between project pairs)
- Aggregated cycle count
- Top hotspots across all projects

## Affected Code

- `crates/graphify-report/` — summary generation
- `crates/graphify-cli/src/main.rs` — pipeline orchestration (where summary is written)
