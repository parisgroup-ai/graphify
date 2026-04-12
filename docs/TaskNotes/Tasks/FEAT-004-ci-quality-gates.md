---
uid: feat-004
status: open
priority: normal
timeEstimate: 240
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - ci
  - analysis
---

# CI quality gates

CLI flags and exit codes for CI integration — fail the build if architectural quality degrades.

## Goals

- `graphify check --max-cycles 0 --max-hotspot-score 0.8`
- Non-zero exit code on violation
- JSON output for CI parsers
- GitHub Action wrapper (optional)

## Notes

Best paired with drift detection (FEAT-002). Depends on visualization (FEAT-001) to "sell" the value to teams.
