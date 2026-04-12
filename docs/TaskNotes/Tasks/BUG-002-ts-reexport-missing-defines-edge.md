---
status: done
completed: 2026-04-12
priority: normal
timeEstimate: 120
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - extract
  - typescript
tags:
  - task
  - bug
  - typescript
  - extract
uid: bug-002
---

# fix(extract): TS re-export missing Defines edge for exported symbol

## Description

TypeScript re-exports like `export { foo } from './bar'` create an `Imports` edge from the re-exporting module to `bar`, but do NOT create a `Defines` edge for `foo` in the re-exporting module's namespace. This means downstream consumers that import `foo` from the re-exporting module may not have a correct edge to the actual definition.

## Impact

- Incomplete dependency tracking for barrel files (`index.ts`) that re-export
- Potentially missing hotspot detection for heavily re-exported symbols

## Affected Code

- `crates/graphify-extract/src/typescript.rs` — `ExportNamedDeclaration` with `source` field handling
