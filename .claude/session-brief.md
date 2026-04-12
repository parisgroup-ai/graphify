# Session Brief — 2026-04-12 (Session 2)

## Last Session Summary

Shipped v0.2.0 release + FEAT-006 (graph query interface with 4 new CLI commands) + FEAT-004 (CI quality gates). Fixed all cargo fmt + clippy warnings for CI compliance. 15 commits, 150→181 tests.

## Current State

- Branch: `main`
- Last commit: `c68b200` — `fix: apply cargo fmt + fix clippy warnings for CI compliance`
- Tests: 181 passing (`cargo test --workspace`)
- Version: 0.2.0
- CI: GitHub Actions quality gates (fmt + clippy + tests) on every push to main
- All code passes `cargo fmt --check` + `cargo clippy -- -D warnings`
- No pending unstaged code changes

## Open Items

No bugs remain. Feature backlog (3 done, 7 open):

| ID | Priority | Est | Title |
|---|---|---|---|
| FEAT-002 | normal | 8h | Architectural drift detection |
| FEAT-003 | low | 16h | New language support (Go, Rust) |
| FEAT-005 | high | 16h | Incremental builds with SHA256 cache |
| FEAT-007 | normal | 16h | MCP server for graph queries (unblocked by FEAT-006) |
| FEAT-008 | normal | 8h | Edge confidence scoring |
| FEAT-009 | low | 12h | Additional export formats (Neo4j, GraphML, Obsidian) |
| FEAT-010 | low | 8h | Watch mode for auto-rebuild |

## Decisions Made (don't re-debate)

- **Rust over Python** — standalone binary distribution (3.5MB vs 50-80MB)
- **petgraph over custom graph** — mature, Tarjan/SCC built-in
- **Louvain over Leiden** — no mature Rust Leiden crate
- **`is_package` via boolean parameter** — clean, testable
- **tree-sitter per call** — Parser is not Send
- **D3.js v7 vendored** — full offline self-containment
- **Force-directed layout** — simpler, proven for dependency graphs
- **SVG/Canvas auto-switch at 300 nodes**
- **Safe DOM construction** — createElement/textContent only
- **Workspace alias preservation** (BUG-007)
- **Singleton merging** (BUG-008)
- **Built-in test file exclusion** (BUG-006)
- **QueryEngine in graphify-core** (FEAT-006) — reusable for FEAT-007 MCP server
- **Re-extract on the fly** (FEAT-006) — always fresh data, no stale graph files
- **No readline crate for REPL** (FEAT-006) — plain stdin, keep binary lean
- **GlobMatcher without external crate** (FEAT-006) — simple recursive byte matching
- **CI: strict clippy** (FEAT-004) — `-D warnings` fails the build on any lint

## Suggested Next Steps

1. **FEAT-007** (now unblocked) — MCP server importing QueryEngine from graphify-core
2. **FEAT-005** (high) — Incremental builds with SHA256 cache
3. **FEAT-002** (normal) — Architectural drift detection
