# Session Brief — 2026-04-12

## Last Session Summary

Bug fix sprint — fixed all 5 remaining open bugs (BUG-006 through BUG-010). Used work graph pattern: BUG-007 (critical, investigative) first solo, then BUG-006/009/010 dispatched as 3 parallel subagents, then BUG-008 (architectural) last. Test count went from 137 to 150. All pushed to origin/main.

## Current State

- Branch: `main`
- Last commit: `8cffb7e` — `fix(core): merge singleton communities on sparse graphs (BUG-008)`
- Tests: 150 passing (`cargo test --workspace`)
- Version: 0.1.1
- All 10 bugs (BUG-001 through BUG-010) are done
- No pending unstaged code changes

## Open Items

No bugs remain. Feature backlog:

| ID | Priority | Est | Title |
|---|---|---|---|
| FEAT-002 | normal | 8h | Architectural drift detection |
| FEAT-003 | low | 16h | New language support (Go, Rust) |
| FEAT-004 | normal | 4h | CI quality gates |
| FEAT-005 | high | 16h | Incremental builds with SHA256 cache |
| FEAT-006 | high | 16h | Graph query interface (query, path, explain) |
| FEAT-007 | normal | 16h | MCP server for graph queries |
| FEAT-008 | normal | 8h | Edge confidence scoring |
| FEAT-009 | low | 12h | Additional export formats (Neo4j, GraphML, Obsidian) |
| FEAT-010 | low | 8h | Watch mode for auto-rebuild |

## Decisions Made (don't re-debate)

- **Rust over Python** — for standalone binary distribution (3.5MB vs 50-80MB PyInstaller)
- **petgraph over custom graph** — mature, Tarjan/SCC built-in
- **Louvain over Leiden** — no mature Rust Leiden crate; Louvain sufficient for code graphs
- **`is_package` via boolean parameter** (not file path) — clean, testable, matches Python's own import model
- **tree-sitter per call** — Parser is not Send, so create fresh parser per extract_file call
- **D3.js v7 vendored** (not CDN) for full offline self-containment
- **Force-directed layout** (not hierarchical) — simpler, proven for dependency graphs
- **SVG/Canvas auto-switch at 300 nodes** — SVG for crisp interaction, Canvas for performance
- **Safe DOM construction** — createElement/textContent only, no innerHTML
- **Workspace alias preservation** (BUG-007) — when TS alias resolves to path with `..`, keep original import name as node ID
- **Singleton merging** (BUG-008) — post-Louvain Phase 2: absorb connected singletons into best neighbor's community, group isolated singletons together
- **Built-in test file exclusion** (BUG-006) — always active `is_test_file()` filter, not configurable

## Suggested Next Steps

1. **Version bump to 0.2.0** — FEAT-001 + 5 bug fixes is a meaningful minor release, then `git tag v0.2.0` to trigger CI release
2. **FEAT-005** (high) — Incremental builds with SHA256 cache would dramatically improve re-analysis speed
3. **FEAT-006** (high) — Graph query interface would make Graphify useful as a dev tool, not just a report generator
