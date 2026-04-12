---
status: open
priority: high
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
  - typescript
  - tests
uid: bug-006
---

# fix(extract): Walker exclude patterns miss .test.ts and .spec.ts files

## Description

The walker's directory-based exclude patterns (`tests`, `__tests__`) don't catch test files that live alongside production code (e.g., `src/resilience.test.ts`). This pollutes the graph with test framework artifacts.

## Evidence

pkg-resilience analysis shows top 4 hotspots are **test globals**, not modules:

| Rank | Node | Score | What it is |
|------|------|-------|------------|
| 1 | `vitest` | 0.400 | Test framework import |
| 2 | `expect` | 0.400 | Test assertion global |
| 3 | `it` | 0.400 | Test block function |
| 4 | `describe` | 0.400 | Test suite function |

Source: `report/pkg-resilience/analysis.json` from ToStudy monorepo (2026-04-12).

## Root Cause

`crates/graphify-extract/src/walker.rs` exclude logic is directory-based only. It checks if a path segment matches the exclude list, but doesn't filter by file name patterns.

Files like `src/circuit-breaker.test.ts` or `src/retry.spec.ts` are NOT in a `tests/` directory, so they pass the exclude check.

## Fix Approach

In `walker.rs`, add file-level exclusion patterns:

1. Skip files matching `*.test.{ts,tsx,js,jsx}` and `*.spec.{ts,tsx,js,jsx}`
2. Also skip `*.test.py` and `*_test.py` (Python convention)
3. Make this configurable via `graphify.toml` (optional: `exclude_patterns = ["*.test.*", "*.spec.*"]`)

Alternatively, add built-in "test file" detection that's always active unless overridden.

## Affected Code

- `crates/graphify-extract/src/walker.rs` — `walk_directory()` or equivalent file filter
- `crates/graphify-cli/src/main.rs` — config parsing (if adding `exclude_patterns`)

## Impact

- Affects ALL TypeScript packages that have co-located test files
- Confirmed in: pkg-resilience, likely also in pkg-validators, pkg-logger, tostudy-core
- Inflates community count (test globals create isolated nodes)
- Corrupts hotspot rankings (test artifacts score higher than real modules)
