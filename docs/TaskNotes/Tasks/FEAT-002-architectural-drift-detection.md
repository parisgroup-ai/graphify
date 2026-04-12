---
uid: feat-002
status: open
priority: normal
timeEstimate: 480
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - analysis
  - ci
---

# Architectural drift detection

Compare two analysis snapshots over time to surface what changed: new dependencies, new cycles, hotspot movements, community shifts.

## Goals

- `graphify diff --before analysis-a.json --after analysis-b.json`
- Detect new/removed edges, new/resolved cycles
- Track hotspot score changes over time
- Output structured diff (JSON + human-readable summary)

## Notes

Requires adoption first — users need to run Graphify regularly. Best after visualization makes the tool compelling.
