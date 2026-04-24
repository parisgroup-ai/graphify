---
uid: feat-041
status: done
priority: normal
scheduled: 2026-04-24
completed: 2026-04-24
timeEstimate: 240
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

## Goal

Add a first-class `graphify compare` CLI surface for head-to-head architecture comparison between two existing Graphify outputs, aimed at comparing PR artifacts or branch snapshots without forcing users to think in baseline/live terminology.

## MVP Scope

- Accept two inputs that may be either `analysis.json` files or directories containing `analysis.json`.
- Let callers label both sides, e.g. `--left-label PR-123 --right-label PR-456`.
- Reuse the existing `graphify_core::diff::compute_diff_with_config` engine rather than duplicating diff logic.
- Write compare-oriented outputs (`compare-report.json` and `compare-report.md`) and print a concise stdout summary.
- Preserve the existing `graphify diff` behavior unchanged.

## Acceptance Criteria

- `graphify compare <left> <right>` works for file inputs and directory inputs.
- Missing or invalid inputs produce actionable errors.
- Output labels appear in Markdown/stdout so PR-vs-PR comparisons are readable.
- Unit or integration coverage exercises file input, directory input, and one validation failure.

## Notes

This is the most tractable next territory from the previous session brief: small CLI surface, clear user value, and it builds on existing diff/pr-summary infrastructure.
