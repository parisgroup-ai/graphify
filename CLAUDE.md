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

# Build from source
cargo build --release -p graphify-cli
# Binary at target/release/graphify
```

## Configuration

Multi-project analysis via `graphify.toml`:

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]  # betweenness, pagerank, in_degree, in_cycle
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format = ["json", "csv", "md"]

[[project]]
name = "ana-service"
repo = "./apps/ana-service"
lang = ["python"]
local_prefix = "app."
```

## Architecture

Cargo workspace with 4 crates:

| Crate | Role | Key deps |
|---|---|---|
| `graphify-core` | Graph model, metrics, community detection, cycles | petgraph, serde, rand |
| `graphify-extract` | tree-sitter AST parsing, file discovery, module resolution | tree-sitter, tree-sitter-python, tree-sitter-typescript, rayon |
| `graphify-report` | JSON, CSV, Markdown output generation | serde_json, csv |
| `graphify-cli` | CLI (clap), config parsing, pipeline orchestration | clap, toml, rayon |

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
        └── architecture_report.md
```

### Key modules

| File | Role |
|---|---|
| `crates/graphify-core/src/types.rs` | Node, Edge, Language, NodeKind, EdgeKind |
| `crates/graphify-core/src/graph.rs` | CodeGraph — petgraph wrapper with dedup + weight increment |
| `crates/graphify-core/src/metrics.rs` | Betweenness, PageRank, unified scoring |
| `crates/graphify-core/src/community.rs` | Louvain + Label Propagation |
| `crates/graphify-core/src/cycles.rs` | Tarjan SCC + simple cycles |
| `crates/graphify-extract/src/python.rs` | Python extractor (imports, defs, calls) |
| `crates/graphify-extract/src/typescript.rs` | TypeScript extractor (imports, exports, require, calls) |
| `crates/graphify-extract/src/resolver.rs` | Module resolver (Python relative, TS path aliases) |
| `crates/graphify-extract/src/walker.rs` | File discovery + dir exclusion |
| `crates/graphify-cli/src/main.rs` | CLI, config parsing, pipeline |

### Graph representation

- **Nodes**: modules, functions, classes — with attributes: `id`, `kind`, `file_path`, `language`, `line`, `is_local`
- **Edge types**: `Imports` (module→module), `Defines` (module→symbol), `Calls` (module→symbol)
- **Weight tracking**: repeated calls increment `Edge.weight` instead of creating duplicate edges
- **Module naming**: file paths normalized to dot notation (`app/services/llm.py` → `app.services.llm`), `__init__.py`/`index.ts` collapsed to parent

## Conventions

- CLI uses `clap` with derive macros
- Config via `graphify.toml` (TOML format, serde Deserialize)
- Extraction parallelized with `rayon::par_iter`
- Each `extract_file` call creates a fresh tree-sitter Parser (Parser is not Send)
- Excluded directories: `__pycache__`, `node_modules`, `.git`, `dist`, `tests`, `__tests__`, `.next`, `build`, `.venv`, `venv`
- Output: one subdirectory per project under the configured output path
- Graph serialization compatible with NetworkX `node_link_data` JSON format
- Tests: 122 unit + integration tests (`cargo test --workspace`)

## Build & Release

- CI: GitHub Actions on tag push (`v*`), builds 4 targets (macOS Intel/ARM, Linux x86/ARM)
- Install: `curl -fsSL .../install.sh | sh`
- Static binaries for Linux (MUSL), universal binaries for macOS
- Release binary ~3.5MB

## Known Issues (open)

- TS re-export (`export { foo } from './bar'`) missing Defines edge for exported symbol
- Cross-project summary (`graphify-summary.json`) is a stub — only writes project names
- Placeholder nodes for unresolved imports always tagged `Language::Python`
- CSV nodes file missing `kind`, `file_path`, `language` columns

## Learning context

This repo doubles as a ToStudy course workspace ("Graphify: Mapeamento Arquitetural de Codebases com IA e Knowledge Graphs"). The `.cursor/rules/` and `.claude/commands/` contain tutor personas for the course, not Graphify development instructions.

## Design docs

- **Spec**: `docs/superpowers/specs/2026-04-12-graphify-rust-rewrite-design.md`
- **Plan**: `docs/superpowers/plans/2026-04-12-graphify-rust-rewrite.md`
