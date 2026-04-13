# Session Brief — 2026-04-12 (Session 5)

## Last Session Summary

Committed FEAT-008 leftover code (bare call confidence + confidence summary in analysis JSON). Designed, planned, and fully implemented FEAT-005: Incremental Builds with SHA256 Cache. 11 commits, 236 total tests (16 new), all pushed to origin/main.

## Current State

- Branch: `main`
- Last commit: `f0b59be` — `docs: update CLAUDE.md and sprint board for FEAT-005 incremental builds`
- Tests: 236 passing (`cargo test --workspace`)
- Version: 0.2.0
- CI: GitHub Actions quality gates (fmt + clippy + tests) on every push to main
- Pushed to origin/main

## Open Items

No bugs remain. Feature backlog (7 done, 3 open):

| ID | Priority | Est | Title |
|---|---|---|---|
| FEAT-002 | normal | 8h | Architectural drift detection |
| FEAT-003 | low | 16h | New language support (Go, Rust) |
| FEAT-009 | low | 12h | Additional export formats (Neo4j, GraphML, Obsidian) |
| FEAT-010 | low | 8h | Watch mode for auto-rebuild (blocked on FEAT-005 — now unblocked!) |

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
- **QueryEngine in graphify-core** (FEAT-006) — reusable for MCP
- **Re-extract on the fly** (FEAT-006) — always fresh data
- **No readline crate for REPL** (FEAT-006) — plain stdin, keep binary lean
- **GlobMatcher without external crate** (FEAT-006) — simple recursive byte matching
- **CI: strict clippy** (FEAT-004) — `-D warnings` fails the build on any lint
- **Separate binary for MCP** (FEAT-007) — keeps graphify-cli free of tokio/rmcp deps
- **Eager extraction on startup** (FEAT-007) — matches CLI pattern, instant tool responses
- **Config duplication** (FEAT-007) — small stable structs, extract later if third consumer
- **rmcp `#[tool(tool_box)]` macro** (FEAT-007) — actual API differs from docs
- **Arc wrapping for QueryEngine** (FEAT-007) — ServerHandler requires Clone
- **Per-project parameter on all tools** (FEAT-007) — optional, defaults to first project
- **Manual PartialEq/Eq for Edge** (FEAT-008) — f64::to_bits() for exact equality
- **Resolver returns confidence** (FEAT-008) — (String, bool, f64) tuple, never upgrade past extractor
- **Bare calls at 0.7/Inferred** (FEAT-008) — unqualified names are uncertain
- **Non-local downgrade to 0.5/Ambiguous** (FEAT-008) — external edges capped
- **Edge merge keeps max confidence** (FEAT-008) — most confident observation wins
- **Cache on by default** (FEAT-005) — `--force` to bypass, matches cargo/esbuild conventions
- **Per-file ExtractionResult caching** (FEAT-005) — resolution always re-runs (depends on full module set)
- **Cache in output directory** (FEAT-005) — `.graphify-cache.json` per project, discoverable
- **sha2 crate** (FEAT-005) — pure Rust, no system deps
- **No MCP caching** (FEAT-005) — MCP server is short-lived, deferred

## Suggested Next Steps

1. **FEAT-010** (low, 8h) — Watch mode for auto-rebuild — now unblocked by FEAT-005's caching layer
2. **FEAT-009** (low, 12h) — Additional export formats — mechanical, extends existing report pattern
3. **FEAT-002** (normal, 8h) — Architectural drift detection — requires adoption data first
