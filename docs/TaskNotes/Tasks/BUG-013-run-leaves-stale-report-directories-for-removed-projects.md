---
status: done
completed: 2026-04-13
priority: normal
timeEstimate: 90
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - cli
  - report
  - output
  - config
tags:
  - task
  - bug
  - output
  - dx
uid: bug-013
---

# fix(cli): `graphify run` leaves stale report directories for removed projects

## Description

When a project is removed from `graphify.toml`, `graphify run` refreshes only the configured projects but does not prune the old `report/<project>/` directory. The summary becomes correct, but stale per-project artifacts remain on disk and can mislead humans, scripts, and diff workflows.

## Evidence

Observed in the ToStudy monorepo on 2026-04-13:

- `course-builder` was removed from `graphify.toml`
- `graphify run --config graphify.toml` reported `total_projects = 16`
- `report/graphify-summary.json` no longer listed `course-builder`
- but `report/course-builder/` still existed with old `analysis.json`, `graph.json`, and `architecture_report.md`
- manual cleanup (`rm -rf report/course-builder`) was required to realign the output directory with the config

## Root Cause

The pipeline writes updated outputs for configured projects, but there is no reconciliation step that removes report directories belonging to projects no longer present in the current config.

## Fix Approach

1. Enumerate configured project names from `graphify.toml`
2. Enumerate existing `report/*/` project directories
3. Remove or optionally prune directories that are not in the active config
4. Keep safety rails:
   - only prune directories that contain Graphify-generated artifacts
   - consider a `--prune-stale` flag if default deletion feels too aggressive

## Affected Code

- `crates/graphify-cli/src/main.rs`
- report/output orchestration in the run/report pipeline

## Impact

- Stale reports can be mistaken for current analysis
- scripts that glob `report/*/architecture_report.md` may ingest removed projects
- architectural diffs become noisy or misleading after config changes
- users lose confidence in `report/` as a faithful mirror of `graphify.toml`

## Verification (2026-04-13)

- Added post-run stale output pruning for `run` and `report`
- Safety rail: only prune directories whose contents are recognized Graphify-generated artifacts
- Added regression test: `test_run_prunes_stale_project_output_directories`
- Verified with `cargo test --test integration_test` → 7 passed, 0 failed
