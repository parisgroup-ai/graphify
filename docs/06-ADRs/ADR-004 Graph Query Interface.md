---
title: "ADR-004: Graph Query Interface (QueryEngine + 4 CLI subcommands)"
created: 2026-04-12
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-006"
tags:
  - type/adr
  - status/accepted
  - cli
  - core
supersedes:
superseded_by:
---

# ADR-004: Graph Query Interface

## Status

**Accepted** — 2026-04-12

## Context

Reading static JSON/Markdown reports is fine for high-level overviews but bad for "what depends on X?" or "how does A reach B?" questions. We needed an interactive surface to **search nodes**, **find dependency paths**, **explain a single module's role**, and **explore the graph in a REPL**. The query logic also had to be **reusable** so a future MCP server (FEAT-007) could expose the same operations to AI assistants.

## Decision

**Chosen option:** Add a `QueryEngine` struct in `graphify-core/src/query.rs` that owns analyzed data and exposes 9 methods (search, paths, explain, dependents, dependencies, transitive_dependents, suggest, stats). Surface it via four CLI subcommands: `query`, `path`, `explain`, `shell`. Formatting lives in the CLI; the engine returns structured types.

All commands always re-extract on invocation — cache is bypassed (the user wants fresh data when they ask a question).

## Consequences

### Positive

- Same engine consumed by CLI today and `graphify-mcp` later — zero duplication
- Structured result types (`QueryMatch`, `PathStep`, `ExplainReport`) are serde-ready for both `--json` and MCP
- Substring fuzzy suggestion ("did you mean…?") works uniformly across all commands
- REPL is built on `std::io::BufRead` — no `rustyline` dependency, no readline platform issues
- Single-project glob matching keeps semantics simple

### Negative

- Always-fresh extraction means each query pays the extract cost (~0.5–2s)
- No history/up-arrow in the REPL (no `rustyline`)
- Cross-project paths are not supported (separate graphs)
- `query` outputs human-formatted columns by default; large result sets need `--json` to be machine-friendly

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **QueryEngine in core + thin CLI** (chosen) | Reusable for MCP, testable, clean layering | Slight overhead designing the API up front |
| Inline query logic in CLI | Fastest to ship | Would need to be rewritten for MCP |
| External graph DB (Neo4j, sqlite) | Powerful queries (Cypher, SQL) | Adds runtime dep; loses the "single binary" promise |
| Separate `graphify-query` crate | Strong isolation | Extra crate boundary for a thin layer |

## Plan de Rollback

**Triggers:** API churn breaks `graphify-mcp` consumers; or fuzzy suggestions become misleading at scale.

**Steps:**
1. Remove the four CLI subcommands from `graphify-cli`
2. Keep `QueryEngine` available in core (used internally by other features like `explain` in reports)
3. If structural: revert FEAT-007 (MCP) accordingly

**Validation:** `graphify --help` no longer lists `query`/`path`/`explain`/`shell`. Pipeline commands (`run`, `extract`, `analyze`, `report`) unaffected.

## Links

- Spec: `docs/superpowers/specs/2026-04-12-graph-query-interface-design.md`
- Plan: `docs/superpowers/plans/2026-04-12-feat-006-graph-query-interface.md`
- Task: `[[FEAT-006-graph-query-interface]]`
- Related ADRs: [[ADR-001 Rust Rewrite]], [[ADR-005 MCP Server]] (downstream consumer)
