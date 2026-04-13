# FEAT-010: Watch Mode for Auto-Rebuild — Design Spec

**Date:** 2026-04-13
**Status:** Approved
**Depends on:** FEAT-005 (Incremental Builds with SHA256 Cache)

## Problem

Graphify currently runs as a one-shot CLI command. During active development, users must manually re-run `graphify run` after every code change to see updated architecture reports. This creates friction in the feedback loop.

## Solution

Add a `graphify watch` subcommand that monitors source files for changes and automatically re-runs the full analysis pipeline, leveraging FEAT-005's extraction cache to skip unchanged files.

## Design Decisions

### Rebuild Strategy: Full Pipeline with Cache

On any file change, re-run the entire pipeline: extract → resolve → build graph → analyze → report. The extraction cache (`ExtractionCache`) already handles per-file SHA256 invalidation, making cached runs sub-second for typical projects. This avoids the complexity and bug surface of incremental graph patching.

**Rejected alternatives:**
- Incremental graph patching — complex, risk of stale state, marginal speed gain
- Two-tier (fast extract + lazy analysis) — adds UX complexity for negligible benefit

### File Watching: `notify` v7

Use the `notify` crate (cross-platform, mature) for filesystem event monitoring. Version 7.x provides the `RecommendedWatcher` abstraction that selects the best backend per platform (FSEvents on macOS, inotify on Linux).

### Debounce: 300ms Window

Use `notify-debouncer-mini` (or manual debounce with `std::time::Instant`) to batch rapid file changes within a 300ms window. This handles IDE auto-save, git checkout (many files change at once), and formatter-on-save scenarios.

### Subcommand Location: `graphify-cli`

The `watch` subcommand lives in `graphify-cli/src/main.rs` as a new `Commands::Watch` variant. It reuses the existing `run_pipeline_for_project` function (or equivalent) for each rebuild cycle. No new crate needed — this is CLI orchestration.

### Output

Regenerate all configured formats on each rebuild cycle, same as `graphify run`. Output goes to the same configured output directory.

### User Feedback

After each rebuild cycle, print to stderr:
- Cache stats (files cached / extracted / total)
- Pipeline timing
- Any warnings (e.g., empty graph, parse errors)

On startup, print:
- Which directories are being watched
- Number of projects configured
- "Press Ctrl+C to stop"

### File Filtering

Watch only files matching the configured languages (`.py`, `.ts`, `.tsx`). Respect the same exclude patterns as the walker (`__pycache__`, `node_modules`, `.git`, `dist`, etc.). Do NOT watch the output directory to avoid feedback loops.

### Error Handling

- Parse errors in a single file: print warning, continue with partial graph (same as one-shot mode)
- Config file changes: NOT watched — user must restart `graphify watch`
- Watcher errors (e.g., too many files): print error to stderr, exit with non-zero status

## CLI Interface

```
graphify watch [--config graphify.toml] [--output ./report] [--force] [--format json,md]
```

- `--config`: Path to config file (default: `graphify.toml`)
- `--output`: Override output directory
- `--force`: Force full rebuild on first run (ignore cache)
- `--format`: Override output formats

Behavior:
1. Parse config, validate projects
2. Run initial full pipeline (respects cache unless `--force`)
3. Start file watcher on all project source directories
4. On file change (after debounce): re-run full pipeline
5. Loop until Ctrl+C (SIGINT)

## Architecture

```
Commands::Watch
    ├── Parse config (reuse existing)
    ├── Initial pipeline run (reuse run_pipeline_for_project)
    ├── Setup notify watcher
    │   ├── Watch each project's repo directory recursively
    │   ├── Filter: only .py/.ts/.tsx files, exclude patterns
    │   └── Exclude output directory
    ├── Event loop (std::sync::mpsc channel)
    │   ├── Receive debounced events
    │   ├── Determine which projects are affected
    │   ├── Re-run pipeline for affected projects only
    │   └── Print stats + timing
    └── SIGINT handler → clean exit
```

## Dependencies

New dependencies for `graphify-cli`:
- `notify = "7"` — file system watcher
- `notify-debouncer-mini = "0.5"` — debounce wrapper (or manual debounce)

No new dependencies for other crates.

## Testing

- Unit test: debounce logic (if custom)
- Unit test: file filter matches correct extensions
- Unit test: output directory exclusion
- Integration test: CLI `watch` subcommand starts and responds to `--help`
- Manual test: create/modify/delete `.py` files, verify rebuild triggers

Note: Automated integration tests for file watching are inherently flaky (timing-dependent). Prefer unit tests for logic and manual verification for the watch loop.

## Scope Boundaries

**In scope:**
- `graphify watch` subcommand
- File watching with debounce
- Per-project rebuild (only affected projects re-run)
- Cache integration

**Out of scope:**
- Hot-reloading the HTML visualization (browser refresh)
- Watching config file changes
- WebSocket/SSE for live updates
- MCP server integration with watch mode
