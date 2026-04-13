---
uid: feat-014
status: open
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

- [ ] Define which metrics are worth storing historically
- [ ] Choose snapshot retention and file layout
- [ ] Implement aggregation across snapshots
- [ ] Expose trend summaries in CLI and reports
- [ ] Add regression fixtures covering metric changes over time
- [ ] Document the operational workflow for teams

## Notes

This work should stay focused on practical trend questions rather than becoming a full observability product. It likely pairs well with drift detection and later with policy rule history.
