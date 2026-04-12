---
uid: feat-001
status: done
priority: high
timeEstimate: 960
completed: 2026-04-12
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - visualization
  - report
designDoc: "[[2026-04-12-interactive-html-visualization-design]]"
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

- [x] Design visualization spec (brainstorm session)
- [x] Implement HTML report generator in graphify-report
- [x] Embed D3.js or similar layout engine
- [x] Add `html` format option to config
- [x] Integration tests
- [x] Manual visual QA

## Notes

This is the highest-impact feature for v0.2 — transforms raw data into immediate architectural insight.

Implemented with D3.js v7 force-directed layout, SVG/Canvas auto-switch at 300 nodes, full explorer UI (sidebar with summary, filters, communities, cycles, force controls, search), marching ants cycle animation, PNG export, community collapse/expand.
