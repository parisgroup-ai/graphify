---
title: Tech Stack
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/architecture
  - dependencies
related:
  - "[[System Overview]]"
  - "[[Data Flow]]"
  - "[[ADR-001 Rust Rewrite]]"
---

# Tech Stack

Every dependency in the workspace, what it does, why it's here.

## Workspace baseline

| Concern | Choice |
|---|---|
| Language | Rust 2021 edition |
| Build | Cargo workspace, 5 member crates |
| MSRV | tracks current stable; no explicit pinning |
| Distribution | static binaries via GitHub Releases (4 targets) |
| CI | GitHub Actions on tag push (`v*`) |
| License | MIT |

## Per-crate dependency map

### `graphify-core`

| Dep | Version | Why |
|---|---|---|
| `petgraph` | 0.7 | Directed graph + Tarjan SCC + base algorithms |
| `rand` | 0.8 | Sampling for Brandes betweenness; Louvain randomization |
| `serde` | 1 (`derive`) | Serialization for snapshots, history, contracts |
| `serde_json` | 1 | JSON read/write for diff/history/contract pipelines |

### `graphify-extract`

| Dep | Version | Why |
|---|---|---|
| `graphify-core` | path | Shared types (`Node`, `Edge`, `Confidence`) |
| `tree-sitter` | 0.25 | AST parser runtime |
| `tree-sitter-python` | 0.25 | Python grammar |
| `tree-sitter-typescript` | 0.23 | TS/TSX grammar |
| `tree-sitter-go` | 0.25 | Go grammar (FEAT-003) |
| `tree-sitter-rust` | 0.24 | Rust grammar (FEAT-003) |
| `rayon` | 1 | Parallel per-file extraction |
| `sha2` | 0.10 | SHA256 for the extraction cache ([[ADR-003 SHA256 Extraction Cache]]) |
| `serde` / `serde_json` | 1 | Cache serialization, contract IO |

> [!info] Grammar version drift
> Tree-sitter runtime is 0.25 but the grammars span 0.23–0.25. The runtime API is stable across these — works in practice. Worth pinning consistent versions when bumping next.

### `graphify-report`

| Dep | Version | Why |
|---|---|---|
| `graphify-core` | path | `Node`, `Edge`, `Community`, `NodeMetrics`, snapshot types |
| `serde` / `serde_json` | 1 | All JSON outputs (graph/analysis/diff/check/trend/contract) |
| `csv` | 1 | `graph_nodes.csv` + `graph_edges.csv` |

> [!tip] D3.js is bundled, not depended on
> The interactive HTML visualization ([[ADR-002 Interactive HTML Visualization]]) embeds D3 v7 via `include_str!`. It's a build-time asset, not a Cargo dep.

### `graphify-cli`

| Dep | Version | Why |
|---|---|---|
| `graphify-core` / `graphify-extract` / `graphify-report` | path | Workspace deps |
| `clap` | 4 (`derive`) | CLI parsing for ~14 subcommands |
| `toml` | 0.8 | Parse `graphify.toml` |
| `serde` / `serde_json` | 1 | Config + JSON I/O |
| `rayon` | 1 | Parallel per-project pipeline |
| `notify` | 7 | File watcher ([[ADR-009 Watch Mode]]) |
| `notify-debouncer-mini` | 0.5 | 300ms debounce window |

Binary name: `graphify` (configured via `[[bin]]` in `Cargo.toml`).

### `graphify-mcp`

| Dep | Version | Why |
|---|---|---|
| `graphify-core` / `graphify-extract` | path | Same pipeline as CLI |
| `rmcp` | 0.1 (`server`, `transport-io`, `macros`) | MCP protocol over stdio |
| `tokio` | 1 (`full`) | Async runtime required by rmcp |
| `clap` | 4 (`derive`) | `--config` flag |
| `toml` | 0.8 | Config parse (duplicated from CLI per [[ADR-005 MCP Server]]) |
| `schemars` | 0.8 | JSON Schema for MCP tool definitions |
| `serde` / `serde_json` | 1 | Tool I/O |
| `rayon` | 1 | Parallel per-project extraction at startup |

Binary name: `graphify-mcp`.

## Cross-cutting choices

### Concurrency

- **Extraction**: `rayon::par_iter` across files within a project; tree-sitter `Parser` is `!Send` so each thread owns a fresh parser
- **Per-project**: CLI runs project pipelines in parallel via `rayon`
- **Watch mode**: `notify` uses an OS thread; debouncer + mpsc channel feed the rebuild loop
- **MCP**: `tokio` async runtime (required by `rmcp`); query handlers are sync (graph is in-memory + `Arc`-shared)

### Serialization conventions

- Every persisted type implements `Serialize` + `Deserialize`
- `Edge` uses manual `Eq` (because `f64` confidence is not `Eq`) — implemented via `f64::to_bits()` for bitwise-exact equality (relevant for cache lookups and tests)
- Graph JSON follows NetworkX `node_link_data` shape for compatibility with downstream Python tooling
- Cache JSON is versioned — incompatible schema changes discard the cache wholesale

### What we deliberately didn't pick

| Skipped | Why |
|---|---|
| `tonic` / gRPC | MCP uses JSON-RPC; no remote/networked surface needed |
| `axum` / HTTP server | Out of scope for v1; CLI + MCP cover the surfaces |
| `sqlx` / Diesel | No persistent state; reports go straight to disk |
| `tracing` | `eprintln!` covers diagnostics; `tracing` is overkill for short-lived CLI runs |
| `anyhow` / `thiserror` (extensively) | Used sparingly in CLI; core favors typed errors directly |
| `rustyline` for the REPL | Plain `stdin` keeps platform compatibility ([[ADR-004 Graph Query Interface]]) |
| `xxhash` for cache | `sha2` is fast enough; SHA256 is universally trusted |
| `gix` / `git2` | Graphify reads source files directly; git history is out of scope |

### Build & release

| Concern | Tool |
|---|---|
| Build | `cargo build --release -p graphify-cli` (or `-p graphify-mcp`) |
| Tests | `cargo test --workspace` |
| Targets | `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` |
| CI | GitHub Actions on tag push `v*` — see `.github/workflows/release.yml` |
| Binary size | ~3.5 MB stripped per binary |
| Linux static | MUSL — no glibc dependency |

## Versioning

All crates share `version.workspace = true`. Bump only `[workspace.package].version` in root `Cargo.toml`. Current: **v0.8.0**.

## Related

- [[System Overview]]
- [[Data Flow]]
- [[ADR-001 Rust Rewrite]] — rationale for the foundational tech choices
- [[Crate - graphify-core]] · [[Crate - graphify-extract]] · [[Crate - graphify-report]] · [[Crate - graphify-cli]] · [[Crate - graphify-mcp]]
