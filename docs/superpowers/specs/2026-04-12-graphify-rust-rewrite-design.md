# Graphify v2 — Rust Rewrite Design Spec

**Date:** 2026-04-12
**Status:** Draft
**Author:** Cleiton Paris + Claude

## Overview

Rewrite Graphify from Python to Rust. The tool analyzes codebase architecture by extracting dependencies via tree-sitter AST parsing, building knowledge graphs, and generating structured reports identifying hotspots, circular dependencies, and community clusters.

### Goals

- Standalone binary (no runtime dependency — no Python, no Node)
- Support Python + TypeScript extraction at the same depth (imports, definitions, calls)
- Multi-project analysis via config file (`graphify.toml`)
- Structured data output (JSON, CSV, Markdown) — no visualization (consumers handle that)
- Distribute via GitHub Releases for macOS + Linux

### Non-goals

- GUI or interactive visualization (consumers like Obsidian, D3.js, Gephi handle that)
- Windows support (v1 targets macOS + Linux)
- Python bindings or PyO3 interop
- Languages beyond Python and TypeScript in v1

## Project Structure

```
graphify/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── graphify-core/          # graph, metrics, shared types
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── graph.rs        # CodeGraph (petgraph DiGraph wrapper)
│   │       ├── metrics.rs      # betweenness, PageRank, hotspot, risk
│   │       ├── community.rs    # community detection (Louvain, Label Propagation fallback)
│   │       ├── cycles.rs       # SCCs (Tarjan), simple cycles (Johnson, cap 500)
│   │       └── types.rs        # Node, Edge, EdgeKind, Language enum
│   │
│   ├── graphify-extract/       # tree-sitter extraction
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── lang.rs         # trait LanguageExtractor
│   │       ├── python.rs       # Python extractor implementation
│   │       ├── typescript.rs   # TypeScript extractor implementation
│   │       ├── resolver.rs     # module/path resolution
│   │       └── walker.rs       # file discovery + directory exclusion
│   │
│   ├── graphify-report/        # output generation
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── json.rs         # node_link_data-compatible JSON
│   │       ├── csv.rs          # nodes.csv, edges.csv
│   │       └── markdown.rs     # architecture_report.md with tables
│   │
│   └── graphify-cli/           # CLI binary
│       └── src/
│           └── main.rs         # clap subcommands, orchestration
│
├── tests/                      # integration tests
└── .github/
    └── workflows/
        └── release.yml         # CI: build macOS + Linux, GitHub Release
```

### Crate Responsibilities

| Crate | Depends on | Role |
|---|---|---|
| `graphify-core` | `petgraph`, `serde` | Graph model, analysis algorithms, scoring. Has no knowledge of tree-sitter or file formats. |
| `graphify-extract` | `graphify-core`, `tree-sitter`, `tree-sitter-python`, `tree-sitter-typescript`, `rayon` | File discovery, AST parsing, extraction. Only crate that knows about tree-sitter grammars. |
| `graphify-report` | `graphify-core`, `serde_json`, `csv` | Serializes analysis results to JSON, CSV, and Markdown. |
| `graphify-cli` | all crates above, `clap`, `toml` | Thin orchestration layer. Reads config, runs pipeline, writes output. |

## Core Types

### Graph Model (`graphify-core/types.rs`)

```rust
enum Language {
    Python,
    TypeScript,
}

enum NodeKind {
    Module,
    Function,
    Class,
    Method,
}

struct Node {
    id: String,           // dot-notation: "app.services.llm" or "src.lib.api"
    kind: NodeKind,
    file_path: PathBuf,
    language: Language,
    line: usize,
    is_local: bool,       // project code vs stdlib/external
}

enum EdgeKind {
    Imports,              // module → module
    Defines,              // module → symbol (function/class)
    Calls,                // module → symbol (call site)
}

struct Edge {
    kind: EdgeKind,
    weight: u32,          // repeated call count
    line: usize,          // source line of import/call
}

// Note: Edge source/target are implicit in petgraph's DiGraph<Node, Edge>.
// Edges are stored as (NodeIndex, NodeIndex, Edge) — the struct above is the edge weight.
```

### Extraction Trait (`graphify-extract/lang.rs`)

```rust
trait LanguageExtractor {
    /// File extensions this extractor handles
    fn extensions(&self) -> &[&str];

    /// Extract nodes and edges from a single file
    fn extract_file(
        &self,
        path: &Path,
        source: &[u8],
        module_name: &str,
    ) -> ExtractionResult;
}

struct ExtractionResult {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}
```

### Analysis Result (`graphify-core`)

```rust
struct ScoringWeights {
    betweenness: f64,   // default 0.4
    pagerank: f64,      // default 0.2
    in_degree: f64,     // default 0.2
    in_cycle: f64,      // default 0.2
}

struct NodeMetrics {
    node: Node,
    betweenness: f64,
    pagerank: f64,
    in_degree: usize,
    out_degree: usize,
    score: f64,           // weighted composite
    community_id: usize,
    in_cycle: bool,
}

struct Community {
    id: usize,
    members: Vec<String>, // node IDs
}

struct AnalysisResult {
    nodes: Vec<NodeMetrics>,
    communities: Vec<Community>,
    cycles: Vec<Vec<String>>,
    summary: Summary,
}

struct Summary {
    total_nodes: usize,
    total_edges: usize,
    total_communities: usize,
    total_cycles: usize,
    top_hotspots: Vec<(String, f64)>, // (node_id, score) top 20
    languages: Vec<(Language, usize)>, // (language, file_count)
}
```

## Extraction Pipeline

### Flow

```
graphify.toml (project definitions)
    ↓
For each [[project]]:
    Walker: discover files by extensions (.py, .ts, .tsx)
        ↓ filter excluded dirs (__pycache__, node_modules, .git, dist, tests, __tests__)
        ↓ detect language by extension
        ↓
    For each file (parallel via rayon):
        LanguageExtractor::extract_file(path, source, module_name)
            ↓ tree-sitter parse → AST
            ↓ walk tree → collect imports, definitions, call sites
            ↓
        ExtractionResult { nodes, edges }
    ↓
    Merge all ExtractionResults → CodeGraph (petgraph DiGraph)
    ↓
    Resolver: normalize module references, connect cross-module edges
    ↓
    CodeGraph ready for analysis
```

### Python Extractor — Captured Patterns

| Pattern | Example | Edge |
|---|---|---|
| `import x` | `import os` | `Imports` → `os` |
| `from x import y` | `from app.services.llm import call_llm` | `Imports` → `app.services.llm` + `Calls` → `call_llm` |
| `from . import y` | `from . import utils` | `Imports` → resolved relative to package |
| Function definition | `def call_llm():` | `Defines` → `call_llm` (NodeKind::Function) |
| Class definition | `class LLMGateway:` | `Defines` → `LLMGateway` (NodeKind::Class) |
| Call site | `call_llm(prompt)` | `Calls` → `call_llm`, weight++ on repeat |

### TypeScript Extractor — Captured Patterns

| Pattern | Example | Edge |
|---|---|---|
| Named import | `import { api } from '@/lib/api'` | `Imports` → resolved path alias |
| Default import | `import React from 'react'` | `Imports` → `react` |
| `export function` | `export function createUser()` | `Defines` → `createUser` |
| `export class` | `export class UserService` | `Defines` → `UserService` |
| `require()` | `const x = require('./util')` | `Imports` → resolved relative |
| Re-export | `export { foo } from './bar'` | `Imports` → `bar` + `Defines` → `foo` |
| Call site | `createUser(data)` | `Calls` → `createUser`, weight++ |

### Module Resolver

The resolver normalizes raw module references from extractors into canonical `Node.id` values:

- **Python:** `__init__.py` collapses to parent package name. Relative imports (`.`, `..`) resolve based on file path within the project. Module naming: `app/services/llm.py` → `app.services.llm`.
- **TypeScript:** Path aliases (`@/`, `~/`, custom paths in `tsconfig.json`) are resolved by reading `tsconfig.json` at project root. `index.ts` collapses like `__init__.py`. Module naming: `src/lib/api.ts` → `src.lib.api`.
- **External detection:** If the resolver cannot map a module to a file in the project tree, the node is marked `is_local: false`.

### Parallelism

File extraction is embarrassingly parallel. Each file is independent — no shared mutable state during extraction. `rayon` parallel iterator processes files across all available cores. Merge into the graph happens sequentially after extraction completes.

## Analysis Engine

### Metrics

| Metric | Algorithm | Implementation |
|---|---|---|
| Betweenness centrality | Brandes, sampled `k=min(200, n)` | Custom on petgraph (same sampling as Python version) |
| PageRank | Iterative, damping=0.85, max 100 iterations, epsilon=1e-6 | Custom implementation (~40 lines) |
| In/Out degree | Direct count | petgraph native |
| Composite score | Weighted sum of normalized metrics | `metrics.rs` |

### Unified Scoring

The Python version has two separate scoring systems (hotspot and risk). Graphify v2 unifies them into a single configurable scorer:

```rust
struct ScoringWeights {
    betweenness: f64,   // default 0.4
    pagerank: f64,      // default 0.2
    in_degree: f64,     // default 0.2
    in_cycle: f64,      // default 0.2
}
```

Each metric is min-max normalized to [0, 1] before weighting. Users can customize weights via CLI (`--weights 0.4,0.2,0.2,0.2`) or in `graphify.toml`.

To replicate the Python hotspot score: `--weights 0.33,0.33,0.33,0.0`
To replicate the Python risk score: `--weights 0.4,0.0,0.4,0.2`

### Community Detection

- **Primary:** Louvain algorithm (modularity optimization on undirected projection of the directed graph)
- **Fallback:** Label Propagation (petgraph native, simpler but less precise)
- **v2 candidate:** Leiden algorithm (better quality than Louvain, but no mature Rust crate exists today)

For code graphs at the scale of hundreds to low thousands of nodes, Louvain and Leiden produce near-identical results.

### Cycle Detection

| Analysis | Algorithm | Source |
|---|---|---|
| Strongly connected components | Tarjan (petgraph `tarjan_scc`) | Identifies circular dependency groups |
| Simple cycles | Johnson's algorithm, capped at 500 | Lists individual circular paths |

Nodes participating in any SCC > 1 node are flagged `in_cycle: true` for scoring.

## CLI Interface

### Configuration File (`graphify.toml`)

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format = ["json", "csv", "md"]

[[project]]
name = "ana-service"
repo = "./apps/ana-service"
lang = ["python"]
local_prefix = "app."

[[project]]
name = "api"
repo = "./apps/api"
lang = ["typescript"]
local_prefix = "src/"

[[project]]
name = "web"
repo = "./apps/web"
lang = ["typescript"]
local_prefix = "src/"
```

### Subcommands

```
graphify init
    → Interactive config generator. Asks for projects, languages, prefixes.
      Writes graphify.toml to current directory.

graphify extract [--config graphify.toml] [--output ./report]
    → Extraction only. Saves graph.json per project.

graphify analyze [--config graphify.toml] [--output ./report] [--weights 0.4,0.2,0.2,0.2]
    → Extraction + analysis. Saves graph.json + analysis.json per project.

graphify report [--config graphify.toml] [--output ./report] [--format json,csv,md]
    → Full pipeline. Saves all output files per project.

graphify run [--config graphify.toml] [--output ./report]
    → Alias for report (backward compatibility with Python CLI).
```

`--config` defaults to `./graphify.toml` in the current directory.

### Output Structure

```
report/
├── ana-service/
│   ├── graph.json              # node_link_data format (compatible with Python version)
│   ├── analysis.json           # metrics, communities, cycles, summary
│   ├── graph_nodes.csv         # id, kind, file_path, language, scores...
│   ├── graph_edges.csv         # source, target, kind, weight, line
│   └── architecture_report.md  # markdown report with tables
├── api/
│   ├── graph.json
│   ├── analysis.json
│   ├── graph_nodes.csv
│   ├── graph_edges.csv
│   └── architecture_report.md
└── graphify-summary.json       # cross-project dependency map
```

### JSON Compatibility

`graph.json` reproduces the same schema as the Python version's `networkx.readwrite.json_graph.node_link_data`:

```json
{
  "directed": true,
  "multigraph": false,
  "nodes": [
    { "id": "app.services.llm", "kind": "Module", "file_path": "app/services/llm.py", "language": "Python", "line": 1, "is_local": true }
  ],
  "links": [
    { "source": "app.routers.chat", "target": "app.services.llm", "kind": "Imports", "weight": 1, "line": 3 }
  ]
}
```

### Cross-Project Summary (`graphify-summary.json`)

When multiple projects import the same external packages or when one project's `local_prefix` appears in another project's imports, the summary maps those connections. This is detected post-analysis by comparing each project's external (non-local) import set:

```json
{
  "projects": ["ana-service", "api", "web"],
  "cross_dependencies": [
    { "from_project": "api", "to_project": "database", "shared_modules": ["packages.database.client"] }
  ],
  "shared_externals": {
    "react": ["web"],
    "fastapi": ["ana-service"]
  }
}
```

## Build and Distribution

### Compilation Targets

| Platform | Target | CI Runner |
|---|---|---|
| macOS Intel | `x86_64-apple-darwin` | `macos-13` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-14` |
| Linux x86_64 | `x86_64-unknown-linux-musl` | `ubuntu-latest` |
| Linux ARM | `aarch64-unknown-linux-musl` | `ubuntu-latest` (cross-compile via `cross`) |

MUSL for Linux produces fully static binaries — zero system dependencies.

### CI/CD Pipeline (GitHub Actions)

Triggered on tag push (`v*`):

1. `cargo test` — all crates, all targets
2. `cargo build --release` — 4 targets in parallel jobs
3. Compress binaries (`tar.gz` per platform)
4. Create GitHub Release with 4 artifacts
5. Update SHA checksums in install script

### Installation

```bash
# Auto-detect OS + architecture
curl -fsSL https://raw.githubusercontent.com/parisgroup/graphify/main/install.sh | sh

# Copies binary to /usr/local/bin/graphify
```

### Versioning

Semantic versioning. Tags: `v0.1.0`, `v0.2.0`, etc. The binary exposes `graphify --version`.

### Repository

`github.com/parisgroup/graphify` — public repo under the parisgroup org. Current Python code moves to a `legacy/python` branch.

## Dependencies Summary

| Crate | Dependency | Version | Purpose |
|---|---|---|---|
| `graphify-core` | `petgraph` | latest | Directed graph, Tarjan SCC, topological sort |
| `graphify-core` | `serde` | 1.x | Serialization derive |
| `graphify-extract` | `tree-sitter` | 0.24+ | AST parsing runtime |
| `graphify-extract` | `tree-sitter-python` | latest | Python grammar |
| `graphify-extract` | `tree-sitter-typescript` | latest | TypeScript grammar |
| `graphify-extract` | `rayon` | 1.x | Parallel file extraction |
| `graphify-report` | `serde_json` | 1.x | JSON output |
| `graphify-report` | `csv` | 1.x | CSV output |
| `graphify-cli` | `clap` | 4.x | CLI argument parsing |
| `graphify-cli` | `toml` | 0.8+ | Config file parsing |

## Migration Path

1. Python code moves to `legacy/python` branch (preserved, not deleted)
2. Rust project starts on `main` branch
3. Output format compatibility maintained — same JSON schema, same CSV columns, same report structure
4. CLI subcommands preserved (`extract`, `analyze`, `report`, `run`)
5. `graphify.toml` is new — replaces `--repo` flag with config-driven multi-project support
