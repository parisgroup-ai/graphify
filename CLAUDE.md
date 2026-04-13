# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Graphify

Graphify is a Rust CLI tool for architectural analysis of codebases. It extracts dependencies from Python and TypeScript source code using tree-sitter AST parsing, builds knowledge graphs with petgraph, and generates structured reports identifying architectural hotspots, circular dependencies, and community clusters.

Distributed as a standalone binary (no runtime dependencies). Targets macOS + Linux.

## Running Graphify

```bash
# Generate config
graphify init

# Full pipeline: extract → analyze → report
graphify run --config graphify.toml

# Individual stages
graphify extract --config graphify.toml    # sources → graph.json per project
graphify analyze --config graphify.toml    # metrics → analysis.json + CSV
graphify report  --config graphify.toml    # all outputs including markdown

# Query the graph
graphify query "app.services.*" --config graphify.toml
graphify path app.main app.services.llm --config graphify.toml
graphify explain app.services.llm --config graphify.toml
graphify shell --config graphify.toml

# Build from source
cargo build --release -p graphify-cli
# Binary at target/release/graphify

# Tests
cargo test --workspace                     # all 137 tests
cargo test -p graphify-extract             # single crate
```

## Configuration

Multi-project analysis via `graphify.toml`:

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]  # betweenness, pagerank, in_degree, in_cycle
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format = ["json", "csv", "md", "html"]

[[project]]
name = "ana-service"
repo = "./apps/ana-service"
lang = ["python"]
local_prefix = "app"
```

## Architecture

Cargo workspace with 5 crates:

| Crate | Role | Key deps |
|---|---|---|
| `graphify-core` | Graph model, metrics, community detection, cycles | petgraph, serde, rand |
| `graphify-extract` | tree-sitter AST parsing, file discovery, module resolution | tree-sitter, tree-sitter-python, tree-sitter-typescript, rayon |
| `graphify-report` | JSON, CSV, Markdown, HTML output generation | serde_json, csv |
| `graphify-cli` | CLI (clap), config parsing, pipeline orchestration | clap, toml, rayon |
| `graphify-mcp` | MCP server exposing graph queries to AI assistants | rmcp, tokio, clap |

### Data flow

```
graphify.toml (project definitions)
    ↓
For each [[project]]:
    Walker: discover files (.py, .ts, .tsx)
        ↓ parallel via rayon
    Extractors: tree-sitter AST → nodes + edges
        ↓
    Resolver: normalize module refs (Python relative, TS path aliases)
        ↓
    CodeGraph (petgraph DiGraph)
        ↓
    Analysis:
        ├── Betweenness centrality (Brandes, sampled k=min(200,n))
        ├── PageRank (iterative, damping=0.85)
        ├── Community detection (Louvain + Label Propagation fallback)
        ├── Cycle detection (Tarjan SCC + DFS simple cycles, cap 500)
        └── Unified scoring (configurable weights)
        ↓
    Report generation:
        ├── graph.json (node_link_data format)
        ├── analysis.json (metrics + communities + cycles)
        ├── graph_nodes.csv / graph_edges.csv
        ├── architecture_report.md
        └── architecture_graph.html
```

### Key modules

| File | Role |
|---|---|
| `crates/graphify-core/src/types.rs` | Node, Edge, Language, NodeKind, EdgeKind |
| `crates/graphify-core/src/graph.rs` | CodeGraph — petgraph wrapper with dedup + weight increment |
| `crates/graphify-core/src/metrics.rs` | Betweenness, PageRank, unified scoring |
| `crates/graphify-core/src/community.rs` | Louvain + Label Propagation |
| `crates/graphify-core/src/cycles.rs` | Tarjan SCC + simple cycles |
| `crates/graphify-core/src/query.rs` | QueryEngine — search, path, explain, stats |
| `crates/graphify-extract/src/python.rs` | Python extractor (imports, defs, calls) |
| `crates/graphify-extract/src/typescript.rs` | TypeScript extractor (imports, exports, require, calls) |
| `crates/graphify-extract/src/resolver.rs` | Module resolver (Python relative w/ `is_package`, TS path aliases) |
| `crates/graphify-extract/src/walker.rs` | File discovery + dir exclusion + `is_package` detection |
| `crates/graphify-report/src/html.rs` | Interactive HTML visualization (D3.js force graph, self-contained) |
| `crates/graphify-cli/src/main.rs` | CLI, config parsing, pipeline |
| `crates/graphify-mcp/src/main.rs` | MCP server entry point, config parsing, extraction pipeline |
| `crates/graphify-mcp/src/server.rs` | GraphifyServer struct, 9 MCP tool handlers, ServerHandler impl |

### Graph representation

- **Nodes**: modules, functions, classes — with attributes: `id`, `kind`, `file_path`, `language`, `line`, `is_local`
- **Edge types**: `Imports` (module→module), `Defines` (module→symbol), `Calls` (module→symbol)
- **Weight tracking**: repeated calls increment `Edge.weight` instead of creating duplicate edges
- **Module naming**: file paths normalized to dot notation (`app/services/llm.py` → `app.services.llm`), `__init__.py`/`index.ts` collapsed to parent
- **Package detection**: `DiscoveredFile.is_package` tracks `__init__.py`/`index.ts` files; resolver uses this to correctly resolve relative imports from package entry points

## Conventions

- CLI uses `clap` with derive macros
- Config via `graphify.toml` (TOML format, serde Deserialize)
- Extraction parallelized with `rayon::par_iter`
- Each `extract_file` call creates a fresh tree-sitter Parser (Parser is not Send)
- Excluded directories: `__pycache__`, `node_modules`, `.git`, `dist`, `tests`, `__tests__`, `.next`, `build`, `.venv`, `venv`
- Excluded test files (built-in): `*.test.{ts,tsx,js,jsx}`, `*.spec.{ts,tsx,js,jsx}`, `*.test.py`, `*_test.py`
- Output: one subdirectory per project under the configured output path
- Graph serialization compatible with NetworkX `node_link_data` JSON format
- Cross-project summary (`graphify-summary.json`) only generated when 2+ projects configured; contains aggregate stats only (no full edge list)
- TS workspace aliases (`@repo/*` → `../../packages/*`) preserve the original import string as node ID when target path traverses outside the project
- Louvain Phase 2 merges singleton communities: connected singletons → best neighbor, isolated singletons → grouped together
- Walker warns via `eprintln!` when a project discovers ≤1 file (misconfigured `local_prefix`)
- MCP server uses `rmcp` v0.1 with `#[tool(tool_box)]` macro (not `#[tool_router]` — API differs from docs)
- MCP server config is duplicated from CLI (small, stable structs — extract if a third consumer appears)
- MCP extraction is eager on startup; all diagnostic output on stderr (stdout reserved for JSON-RPC)
- MCP server wraps `QueryEngine` in `Arc` (ServerHandler requires Clone)
- Tests: 196 unit + integration tests (`cargo test --workspace`)

## Build & Release

- Rust 2021 edition (check current version in root `Cargo.toml`)
- CI: GitHub Actions on tag push (`v*`), builds 4 targets (macOS Intel/ARM, Linux x86/ARM)
- Static binaries for Linux (MUSL), universal binaries for macOS
- Release binary ~3.5MB

### Version bump

All crates use `version.workspace = true` — bump only `[workspace.package].version` in root `Cargo.toml`:
```bash
# Edit Cargo.toml, then:
cargo build --release -p graphify-cli  # rebuilds with new version
git add Cargo.toml Cargo.lock
git commit -m "fix: bump version to X.Y.Z"
git tag vX.Y.Z
git push origin main --tags            # triggers CI release
```

## Design docs

- **Spec**: `docs/superpowers/specs/2026-04-12-graphify-rust-rewrite-design.md`
- **Plan**: `docs/superpowers/plans/2026-04-12-graphify-rust-rewrite.md`
- **BUG-001 design**: `docs/plans/2026-04-12-bug-001-python-relative-import-design.md`

## Task tracking

- Sprint board: `docs/TaskNotes/Tasks/sprint.md`
- Task files: `docs/TaskNotes/Tasks/BUG-*.md` (TaskNotes format with YAML frontmatter)
- Always cross-reference task status against actual codebase — tasks may be stale
