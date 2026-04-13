---
title: Sprint
created: 2026-04-12
updated: 2026-04-13
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
| BUG-011 | **done**    | high     | 3h     | Workspace alias imports still mangled with local_prefix   |
| BUG-012 | **done**    | low      | 1h     | Summary JSON missing communities count                    |
| BUG-013 | **done**    | normal   | 1h30m  | `graphify run` leaves stale report directories         |

## Backlog

| ID       | Status   | Priority | Est    | Title                                                |
| -------- | -------- | -------- | ------ | ---------------------------------------------------- |
| FEAT-001 | **done** | high     | 16h    | Interactive HTML graph visualization                 |
| FEAT-002 | **done** | normal   | 8h     | Architectural drift detection                        |
| FEAT-003 | **done** | low      | 16h    | New language support (Go, Rust)                      |
| FEAT-004 | **done** | normal   | 4h     | CI quality gates                                     |
| FEAT-005 | **done** | high     | 16h    | Incremental builds with SHA256 cache                 |
| FEAT-006 | **done** | high     | 16h    | Graph query interface (query, path, explain)         |
| FEAT-007 | **done** | normal   | 16h    | MCP server for graph queries                         |
| FEAT-008 | **done** | normal   | 8h     | Edge confidence scoring                              |
| FEAT-009 | **done** | low      | 12h    | Additional export formats (Neo4j, GraphML, Obsidian) |
| FEAT-010 | **done** | low      | 8h     | Watch mode for auto-rebuild                          |
| FEAT-011 | **done** | medium   | 4h     | Auto-detect local_prefix                               |
| FEAT-012 | **done** | low      | 2h     | Recipe-based CLI examples for monorepo workflows      |

## Done

- [[BUG-001-python-relative-import-misresolution-creates-false-positive-cycles]] - Fixed `is_package` resolution (2026-04-12)
- [[BUG-002-ts-reexport-missing-defines-edge]] - Already implemented: Defines edges for re-exported symbols (confirmed 2026-04-12)
- [[BUG-004-placeholder-nodes-always-tagged-python]] - Already implemented: `set_default_language` in pipeline (confirmed 2026-04-12)
- [[BUG-003-cross-project-summary-is-stub]] - Implemented full summary: per-project stats, aggregates, top hotspots, cross-deps (2026-04-12)
- [[BUG-011-workspace-alias-still-mangled-with-local-prefix]] - Implemented: `@/*` alias matching no longer captures external scoped packages like `@repo/logger` or `@parisgroup-ai/...` (2026-04-13)
- [[BUG-012-summary-json-missing-communities-count]] - Implemented: `graphify-summary.json` now includes per-project `communities`, with regression coverage for multi-project summaries (2026-04-13)
- [[BUG-013-run-leaves-stale-report-directories-for-removed-projects]] - Implemented: `run`/`report` now prune stale project output directories when they contain only Graphify-generated artifacts (2026-04-13)
- [[BUG-005-csv-nodes-missing-columns]] - Already implemented: CSV includes kind, file_path, language (confirmed 2026-04-12)
- [[FEAT-001-interactive-html-visualization]] - Implemented: self-contained HTML with D3.js force graph, SVG/Canvas auto-switch, full explorer (2026-04-12)
- [[FEAT-007-mcp-server]] - Implemented: MCP server (rmcp over stdio) with 9 tools, per-project support, 15 tests (2026-04-12)
- [[FEAT-008-confidence-scoring]] - Implemented: ConfidenceKind enum (Extracted/Inferred/Ambiguous), confidence scoring on edges, resolver confidence, pipeline downgrade, all report outputs, query filtering, MCP integration, 24 new tests (2026-04-12)
- [[FEAT-009-export-formats]] - Implemented: Neo4j Cypher (graph.cypher), GraphML XML (graph.graphml), Obsidian vault (obsidian_vault/), 13 new tests (2026-04-13)
- [[FEAT-010-watch-mode]] - Implemented: `graphify watch` with notify v7, 300ms debounce, per-project rebuild, WatchFilter, 10 new tests (2026-04-13)
- [[FEAT-011-auto-detect-local-prefix]] - Implemented: runtime auto-detection for omitted `local_prefix`, with `src`/`app` dominance heuristic, root-level fallback, and explicit-prefix override coverage (2026-04-13)
- [[FEAT-004-ci-quality-gates]] - Implemented: `graphify check` with cycle/hotspot gates, human + JSON output, per-project evaluation, and CI-friendly non-zero exit code on violations (2026-04-13)
- [[FEAT-005-incremental-builds]] - Implemented: SHA256-based extraction cache, per-file ExtractionResult caching, --force flag, cache stats output, 16 new tests (2026-04-12)
- [[FEAT-002-architectural-drift-detection]] - Implemented: `graphify diff` with file-vs-file and baseline-vs-live modes, 5-dimension drift detection (summary, edges, cycles, hotspots, communities), JSON + Markdown output, 18 new tests (2026-04-13)
- [[FEAT-003-new-language-support]] - Implemented: Go and Rust extractors (tree-sitter), Go `go.mod` resolver, Rust `crate::`/`super::`/`self::` resolver, new NodeKind variants (Trait, Enum), walker updates for Go/Rust file discovery, 69 new tests (2026-04-13)
- [[FEAT-012-recipe-based-cli-examples-for-monorepo-workflows]] - Implemented: README recipes for full refresh, query, explain, path, drift diff, and end-to-end monorepo investigation workflow (2026-04-13)
