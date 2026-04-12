---
title: Sprint
created: 2026-04-12
updated: 2026-04-12
---

# Graphify — Issues

| ID      | Status      | Priority | Est    | Title                                                     |
| ------- | ----------- | -------- | ------ | --------------------------------------------------------- |
| BUG-001 | **done**    | high     | 4h     | Python relative import misresolution (false cycles)       |
| BUG-002 | **done**    | normal   | 2h     | TS re-export missing Defines edge                         |
| BUG-003 | **done**    | normal   | 3h     | Cross-project summary is a stub                           |
| BUG-004 | **done**    | low      | 1h     | Placeholder nodes always tagged Language::Python           |
| BUG-005 | **done**    | low      | 30m    | CSV nodes file missing kind, file_path, language cols     |
| BUG-006 | **done**    | high     | 1h     | Walker excludes miss .test.ts/.spec.ts files              |
| BUG-007 | **done**    | critical | 3h     | TS workspace alias resolution mangles node IDs            |
| BUG-008 | **done**    | normal   | 2h     | Louvain community detection degenerates on sparse graphs  |
| BUG-009 | **done**    | normal   | 1h     | Walker silently produces empty graph for missing src/     |
| BUG-010 | **done**    | low      | 1h     | Summary JSON includes full edge list (9.6MB bloat)        |

## Backlog

| ID       | Status   | Priority | Est    | Title                                                |
| -------- | -------- | -------- | ------ | ---------------------------------------------------- |
| FEAT-001 | **done** | high     | 16h    | Interactive HTML graph visualization                 |
| FEAT-002 | **open** | normal   | 8h     | Architectural drift detection                        |
| FEAT-003 | **open** | low      | 16h    | New language support (Go, Rust)                      |
| FEAT-004 | **done** | normal   | 4h     | CI quality gates                                     |
| FEAT-005 | **open** | high     | 16h    | Incremental builds with SHA256 cache                 |
| FEAT-006 | **done** | high     | 16h    | Graph query interface (query, path, explain)         |
| FEAT-007 | **open** | normal   | 16h    | MCP server for graph queries                         |
| FEAT-008 | **open** | normal   | 8h     | Edge confidence scoring                              |
| FEAT-009 | **open** | low      | 12h    | Additional export formats (Neo4j, GraphML, Obsidian) |
| FEAT-010 | **open** | low      | 8h     | Watch mode for auto-rebuild                          |

## Done

- [[BUG-001-python-relative-import-misresolution-creates-false-positive-cycles]] - Fixed `is_package` resolution (2026-04-12)
- [[BUG-002-ts-reexport-missing-defines-edge]] - Already implemented: Defines edges for re-exported symbols (confirmed 2026-04-12)
- [[BUG-004-placeholder-nodes-always-tagged-python]] - Already implemented: `set_default_language` in pipeline (confirmed 2026-04-12)
- [[BUG-003-cross-project-summary-is-stub]] - Implemented full summary: per-project stats, aggregates, top hotspots, cross-deps (2026-04-12)
- [[BUG-005-csv-nodes-missing-columns]] - Already implemented: CSV includes kind, file_path, language (confirmed 2026-04-12)
- [[FEAT-001-interactive-html-visualization]] - Implemented: self-contained HTML with D3.js force graph, SVG/Canvas auto-switch, full explorer (2026-04-12)
