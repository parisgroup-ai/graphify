---
uid: feat-014
status: done
completed: 2026-04-13
priority: normal
timeEstimate: 720
tags:
  - task
  - feature
  - analytics
  - reporting
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - analytics
  - reporting
  - history
---

# Historical architecture trend tracking

## Description

Track architecture metrics across snapshots so teams can see whether cycles, hotspots, graph size, and structural churn are improving or regressing over time.

## Motivation

Current drift detection compares two states well, but long-term adoption benefits from trend visibility. Teams need to know whether architectural health is gradually improving, staying flat, or degrading release after release.

## Proposed Outcome

Expose time-series or snapshot history for metrics such as:

1. node and edge growth
2. cycle count over time
3. hotspot score movement
4. community churn
5. rule violation count once policy rules exist

## Likely Scope

- snapshot storage strategy
- historical summary model
- CLI command or report extension for trend output
- markdown and JSON summaries
- optional HTML charts if lightweight enough

## Subtasks

- [x] Define which metrics are worth storing historically
- [x] Choose snapshot retention and file layout
- [x] Implement aggregation across snapshots
- [x] Expose trend summaries in CLI and reports
- [x] Add regression fixtures covering metric changes over time
- [x] Document the operational workflow for teams

## Notes

This work should stay focused on practical trend questions rather than becoming a full observability product. It likely pairs well with drift detection and later with policy rule history.

## Verification (2026-04-13)

- Implemented `crates/graphify-core/src/history.rs` (611 lines) — `HistoryStore`, `TrendSummary`, per-metric aggregation across snapshots.
- Added `graphify trend` CLI surface in `crates/graphify-cli/src/main.rs` with JSON + Markdown output.
- Added `crates/graphify-report/src/trend_json.rs` + `crates/graphify-report/src/trend_markdown.rs` for trend output formatting.
- Added regression coverage: `tests/history_trend_integration.rs` + `tests/trend_cli_integration.rs`.
- README updated with historical trend tracking section.
- Shipped in commit `8ac4215 feat(cli): add historical architecture trend tracking` on 2026-04-13.
- Verified with `cargo test --workspace` from workspace root.
