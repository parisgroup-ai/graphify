---
status: done
completed: 2026-04-13
priority: high
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
uid: bug-011
---

# fix(extract): Workspace alias imports still mangled when local_prefix is set

## Description

BUG-007 fixed the `..` prefix artifacts in workspace alias node IDs, but the resolver still prepends the project's `local_prefix` to external package imports. This produces corrupted node IDs like `srcrepo.logger` instead of `@repo/logger`.

## Evidence

ToStudy monorepo, Graphify v0.2.0, 16 projects — **108 mangled nodes** across 4 projects:

| Project | Mangled | Total | % | Example |
|---------|--------:|------:|--:|---------|
| web | 100 | 7,708 | 1.3% | `srcparisgroup-ai.pageshell.primitives` |
| mobile | 5 | 392 | 1.3% | `.parisgroup-ai.pageshell-native.composites` |
| pkg-types | 2 | 156 | 1.3% | `srcrepo.validators.mentorship` |
| pkg-database | 1 | 712 | 0.1% | `srcrepo.types` |

Two mangling patterns observed:

**Pattern A — with `local_prefix = "src"`:**
```
@repo/logger         → srcrepo.logger           (expected: @repo/logger)
@parisgroup-ai/pageshell/primitives → srcparisgroup-ai.pageshell.primitives
@dnd-kit/core        → srcdnd-kit.core
@stripe/stripe-js    → srcstripe.stripe-js
```

**Pattern B — without `local_prefix` (mobile):**
```
@parisgroup-ai/pageshell-native/composites → .parisgroup-ai.pageshell-native.composites
```

Leading `.` artifact replaces the `@` and prefix, but no `src` pollution.

## Root Cause

In `crates/graphify-extract/src/resolver.rs`, `resolve_ts_import()`:

1. Import `from "@repo/logger"` arrives
2. The `@` is stripped and `/` converted to `.` → `repo.logger`
3. The `local_prefix` (`src`) is unconditionally prepended → `srcrepo.logger`
4. No check distinguishes "local file resolved via alias" from "external workspace package"

The fix in BUG-007 removed the `..` path traversal artifact but didn't address the `local_prefix` prepend to external references.

## Fix Approach

1. **Before prepending `local_prefix`**, check if the resolved path is an external workspace alias:
   - Starts with `@` in the original import
   - Is not under `{repo}/{local_prefix}/` after resolution
2. If external: preserve the original package name as node ID (e.g., `@repo/logger`, `@parisgroup-ai/pageshell/primitives`)
3. If local: apply `local_prefix` as today

This should also fix Pattern B (mobile without prefix) — the leading `.` comes from the same normalization path.

## Affected Code

- `crates/graphify-extract/src/resolver.rs` — `resolve_ts_import()`, `apply_ts_alias()`, `normalize_to_dot_notation()`

## Impact

- **108 nodes misidentified** across 4 projects in a 16-project monorepo
- Cross-project coupling stats in `graphify-summary.json` may undercount shared modules (an external ref like `@repo/logger` and a local `src.utils.logger` won't be recognized as the same module)
- Hotspot scores for external packages are unreliable (they're mixed with local namespace)
- Regression from BUG-007 fix — same root cause area, incomplete resolution

## Verification (2026-04-13)

- Fixed TypeScript alias wildcard matching so `@/*` only matches imports that start with `@/`
- Added regression test: `resolve_ts_internal_alias_does_not_capture_scoped_package_imports`
- Verified with `cargo test -p graphify-extract resolver -- --nocapture` → 37 passed
- Verified with `cargo build -p graphify-cli --bin graphify` and `cargo test --test integration_test` → 7 passed
