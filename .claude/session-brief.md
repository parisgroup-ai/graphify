# Session Brief — 2026-04-12 (Session 3)

## Last Session Summary

Designed and implemented FEAT-007: MCP server for graph queries. New `graphify-mcp` crate exposes all 9 QueryEngine methods as MCP tools over stdio using `rmcp`. Full brainstorm → design spec → implementation plan → subagent-driven development pipeline. 8 commits, 196 tests (15 new), pushed to origin/main.

## Current State

- Branch: `main`
- Last commit: `dce10f3` — `docs: update architecture and sprint board for FEAT-007 MCP server`
- Tests: 196 passing (`cargo test --workspace`)
- Version: 0.2.0
- CI: GitHub Actions quality gates (fmt + clippy + tests) on every push to main
- No pending unstaged code changes (only build artifacts and deleted command files)

## Open Items

No bugs remain. Feature backlog (5 done, 5 open):

| ID | Priority | Est | Title |
|---|---|---|---|
| FEAT-002 | normal | 8h | Architectural drift detection |
| FEAT-003 | low | 16h | New language support (Go, Rust) |
| FEAT-005 | high | 16h | Incremental builds with SHA256 cache |
| FEAT-008 | normal | 8h | Edge confidence scoring |
| FEAT-009 | low | 12h | Additional export formats (Neo4j, GraphML, Obsidian) |
| FEAT-010 | low | 8h | Watch mode for auto-rebuild (blocked on FEAT-005) |

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
- **Separate binary for MCP** (FEAT-007) — keeps graphify-cli free of tokio/rmcp deps
- **Eager extraction on startup** (FEAT-007) — matches CLI pattern, instant tool responses
- **Config duplication** (FEAT-007) — small stable structs, extract later if third consumer
- **rmcp `#[tool(tool_box)]` macro** (FEAT-007) — actual API differs from docs, `tool_router` doesn't exist in v0.1
- **Arc wrapping for QueryEngine** (FEAT-007) — ServerHandler requires Clone, Arc is zero-copy
- **Per-project parameter on all tools** (FEAT-007) — optional, defaults to first project

## Suggested Next Steps

1. **FEAT-008** (normal) — Edge confidence scoring — well-scoped, improves analysis quality
2. **FEAT-005** (high) — Incremental builds with SHA256 cache — performance for large codebases
3. **FEAT-002** (normal) — Architectural drift detection — requires adoption first
