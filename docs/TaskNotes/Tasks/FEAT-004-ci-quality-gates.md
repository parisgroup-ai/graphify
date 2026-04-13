---
uid: feat-004
status: done
priority: normal
timeEstimate: 240
completed: 2026-04-13
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

## Verification

- Added `graphify check` with `--max-cycles`, `--max-hotspot-score`, `--project`, `--json`, and `--force`
- Exit code is non-zero when any project violates a configured gate
- JSON output includes root `ok`/`violations` plus per-project `summary`, `limits`, and `violations`
- Verified with `cargo test --test integration_test`, `cargo build -p graphify-cli --bin graphify`, and `cargo test -p graphify-cli`
