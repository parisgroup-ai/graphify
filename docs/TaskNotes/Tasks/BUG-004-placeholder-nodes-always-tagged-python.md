---
status: done
completed: 2026-04-12
priority: low
timeEstimate: 60
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - extract
  - resolver
tags:
  - task
  - bug
  - extract
  - resolver
uid: bug-004
---

# fix(extract): placeholder nodes for unresolved imports always tagged Language::Python

## Description

When Graphify encounters an import that cannot be resolved to a local file (external dependency or unresolved path), it creates a placeholder node. These placeholder nodes are always tagged with `Language::Python` regardless of the actual source language.

## Impact

- Incorrect language attribution in reports for TypeScript projects
- Misleading node counts per language in analysis output

## Affected Code

- `crates/graphify-extract/src/resolver.rs` — placeholder node creation
- `crates/graphify-core/src/types.rs` — `Node` struct, `Language` enum
