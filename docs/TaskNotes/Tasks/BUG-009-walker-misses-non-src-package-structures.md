---
status: done
priority: normal
timeEstimate: 60
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - extract
  - walker
tags:
  - task
  - bug
  - walker
  - config
uid: bug-009
---

# fix(extract): Walker produces empty graph for packages without src/ directory

## Description

When a project's `local_prefix` points to a non-existent directory structure, the walker silently produces a near-empty graph (1 node) instead of reporting an error.

## Evidence

course-builder analysis:

| Metric | Value | Expected |
|--------|-------|----------|
| Nodes | 1 | 10+ |
| Edges | 0 | 10+ |
| Communities | 1 | — |

The only node found: `src.tutorials.first-exercise` (score 0.000).

Config in `graphify.toml`:
```toml
[[project]]
name = "course-builder"
repo = "./packages/course-builder"
lang = ["typescript"]
local_prefix = "src"
```

The package likely has a flat structure or different entry point, not matching the `src/` convention.

## Root Cause

In `crates/graphify-extract/src/walker.rs`, `walk_directory()`:
1. Scans `{repo}/` recursively for `.ts`/`.tsx` files
2. If `local_prefix = "src"`, it expects files under `{repo}/src/`
3. When the directory structure doesn't match, only stray files outside the expected path are found
4. **No warning or error is logged** when the result is near-empty

## Fix Approach

1. **Warn on empty/minimal results:** If walker finds ≤1 files for a project, emit a warning: `"Project '{name}' found only {n} file(s). Check repo path and local_prefix."`
2. **Validate directory exists:** Before walking, verify `{repo}/{local_prefix}/` exists. If not, warn.
3. **Optional:** Auto-detect `local_prefix` when omitted (scan for most common top-level directory with source files)

## Affected Code

- `crates/graphify-extract/src/walker.rs` — `walk_directory()` entry point
- `crates/graphify-cli/src/main.rs` — pipeline orchestration (for validation before running)

## Impact

- Silent failure: user gets a valid-looking report with 0 useful data
- Confirmed in: course-builder (ToStudy monorepo)
- Any monorepo project with non-standard structure will silently produce empty analysis
