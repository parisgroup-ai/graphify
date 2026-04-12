---
status: done
priority: critical
timeEstimate: 180
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - extract
  - resolver
tags:
  - task
  - bug
  - typescript
  - monorepo
  - resolver
uid: bug-007
---

# fix(extract): TypeScript workspace alias resolution produces mangled node IDs

## Description

When resolving monorepo workspace imports like `@repo/validators`, the resolver produces corrupted node identifiers with double-dot prefixes and concatenated path segments.

## Evidence

pkg-types analysis shows mangled node names:

| Node ID | Score | Expected |
|---------|-------|----------|
| `..srcrepo.validators.mentorship` | 0.400 | `@repo/validators/mentorship` or external ref |
| `..srcrepo.validators` | 0.300 | `@repo/validators` or external ref |

Source: `report/pkg-types/analysis.json` from ToStudy monorepo (2026-04-12).

The `..src` prefix is a path traversal artifact; `repo.validators` is the workspace alias `@repo/validators` with the `@` stripped and `/` converted to `.`.

## Root Cause

In `crates/graphify-extract/src/resolver.rs`, the `apply_ts_alias()` function (likely around lines 237-260) transforms workspace path aliases but produces invalid dot-notation identifiers.

Likely sequence:
1. Import `from "@repo/validators"` comes in
2. `apply_ts_alias()` resolves `@repo/*` → `../../packages/*/src`
3. The relative path `../../packages/validators/src` gets normalized to dot notation
4. The `..` path traversal becomes literal `..` in the node ID
5. The `src` directory gets concatenated: `..srcrepo.validators`

The resolver should either:
- Resolve the workspace alias to an absolute path before dot-notation conversion
- Or mark these as external references (not local modules) and tag them differently

## Fix Approach

Option A (recommended): When `is_local` is false and the import matches a workspace alias, keep the original `@repo/validators` name as the node ID (no path resolution).

Option B: Resolve workspace aliases to absolute paths first, then convert to dot notation from the project root.

## Affected Code

- `crates/graphify-extract/src/resolver.rs` — `apply_ts_alias()`, `resolve_ts_import()`
- May also need changes in `normalize_to_dot_notation()` to handle `..` properly

## Impact

- **Critical for monorepo analysis** — any project importing from `@repo/*` will have mangled cross-project references
- Breaks cross-project dependency tracking in `graphify-summary.json`
- Confirmed in: pkg-types. Likely affects ALL TypeScript packages in monorepos.
- Makes hotspot scores unreliable when external refs are miscounted
