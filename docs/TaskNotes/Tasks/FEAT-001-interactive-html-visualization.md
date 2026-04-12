---
uid: feat-001
status: open
priority: high
timeEstimate: 960
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - visualization
  - report
---

# Interactive HTML graph visualization

Self-contained HTML file (no server) that renders the dependency graph interactively.

## Goals

- Nodes colored by community
- Node size proportional to hotspot score
- Cycles highlighted visually
- Filters by community/language
- Hover with node metrics (betweenness, PageRank, degree)
- Zero runtime dependencies — single HTML file with embedded JS/CSS

## Subtasks

- [ ] Design visualization spec (brainstorm session)
- [ ] Implement HTML report generator in graphify-report
- [ ] Embed D3.js or similar layout engine
- [ ] Add `html` format option to config
- [ ] Integration tests
- [ ] Manual visual QA

## Notes

This is the highest-impact feature for v0.2 — transforms raw data into immediate architectural insight.
