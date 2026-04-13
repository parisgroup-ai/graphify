---
uid: feat-010
status: done
completed: 2026-04-13
priority: low
timeEstimate: 480
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - cli
  - performance
---

# Watch mode for auto-rebuild on file changes

Add `--watch` flag that monitors source directories and automatically re-runs extraction + analysis when files change.

## Goals

- `graphify run --config graphify.toml --watch` — start watching configured project directories
- On file change: re-extract changed files, merge into existing graph, re-run analysis
- Debounce rapid changes (e.g., 500ms delay after last change)
- Terminal output shows what changed and what was re-analyzed
- Depends on FEAT-005 (incremental builds) for efficient re-extraction

## Inspiration

safishamsi/graphify has `--watch` mode using Python's watchdog. Code changes trigger instant AST-only rebuild. Doc changes notify user to re-run with LLM. In our case (pure AST, no LLM), watch mode can be fully automatic.

## Subtasks

- [x] Research Rust file watching libraries (notify crate)
- [x] Implement file watcher with debouncing
- [x] Integrate with incremental extraction pipeline
- [x] Terminal UI: show changed files + re-analysis results
- [x] Add `--watch` CLI flag
- [x] Tests: file change detection, debounce behavior
- [x] Documentation update

## Notes

This pairs naturally with FEAT-005 (incremental builds). Without caching, watch mode would do full rebuilds on every save — too slow. With caching, it only re-processes the changed file. Low priority but great DX.

## Verification (2026-04-13)

- Verified CLI exposes `watch` command in `graphify --help`
- Verified `cmd_watch(...)`, `WatchFilter`, and affected-project rebuild logic exist in `crates/graphify-cli`
- Verified sprint history records implementation with 10 tests on 2026-04-13
