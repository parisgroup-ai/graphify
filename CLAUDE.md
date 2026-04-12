# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Graphify

Graphify is a Python CLI tool for architectural analysis of codebases. It extracts dependencies from Python source code using tree-sitter AST parsing, builds knowledge graphs with NetworkX, and generates reports identifying architectural hotspots, circular dependencies, and community clusters. Currently targets Python codebases (primary target: ana-service from the ToStudy monorepo).

## Running Graphify

```bash
# Full pipeline: extract → analyze → report
python graphify.py run --repo <path-to-python-app> --output ./report

# Individual stages
python graphify.py extract --repo ./app --output ./report    # imports → graph.json
python graphify.py analyze --repo ./app --output ./report    # metrics → analysis.json
python graphify.py report  --repo ./app --output ./report    # markdown + PNG

# Symbol-level pipeline (imports + definitions + calls → CSV + quality report)
python pipeline.py --target-dir <path> --output-dir ./output

# Standalone analysis with community detection narrative
python analyze.py --target-dir <path>                        # full pipeline (symbols + calls)
python analyze.py --target-dir <path> --use-imports          # imports only

# Risk scoring
python risk_report.py --target-dir <path>

# Single-file AST test
python extractor.py <file.py>
```

## Dependencies

No requirements.txt or pyproject.toml — install manually:

```bash
# Core (required)
pip install networkx tree-sitter tree-sitter-python matplotlib

# Community detection (Leiden preferred, Louvain fallback)
pip install igraph leidenalg python-louvain
```

Tree-sitter requires a C/C++ toolchain. On macOS: `xcode-select --install`.

## Architecture

There are **two parallel extraction pipelines**:

1. **Imports-only** (`graphify_extract.py`): Parses `import` / `from...import` statements via tree-sitter. Used by the unified CLI `graphify.py`. Produces module-level graph with `IMPORTS` edges.

2. **Full symbols** (`extractor.py` + `pipeline.py`): Extracts function/class definitions AND call sites. Produces a finer-grained graph with `defines` and `calls` edges, plus weight tracking for repeated calls. Used by `analyze.py` (default) and `risk_report.py`.

### Data flow

```
Source files (.py)
    ↓ tree-sitter AST parsing
Extraction (graphify_extract.py OR extractor.py)
    ↓ NetworkX DiGraph
Analysis (graphify.py:cmd_analyze OR analyze.py)
    ├── Centrality metrics (betweenness, PageRank, in/out degree)
    ├── Community detection (Leiden → Louvain fallback)
    ├── Cycle detection (strongly connected components)
    └── Hotspot scoring
    ↓
Report generation
    ├── architecture_report.md (markdown with tables)
    ├── analysis.json (structured metrics)
    ├── graph_communities.png (matplotlib visualization)
    └── CSV exports (graph_nodes.csv, graph_edges.csv)
```

### Key modules

| File | Role |
|---|---|
| `graphify.py` | Unified CLI with subcommands (extract, analyze, report, run) |
| `graphify_extract.py` | Import extraction via tree-sitter, graph construction, JSON export |
| `extractor.py` | Low-level AST extraction: `extract_symbols()` and `extract_calls()` |
| `pipeline.py` | Full symbol pipeline: definitions + calls → graph + CSV + quality report |
| `analyze.py` | Standalone analysis: metrics, Leiden/Louvain communities, narrative |
| `risk_report.py` | Risk scoring: `0.4*betweenness + 0.4*in_degree + 0.2*in_cycle` |
| `build_graph.py` | Example/reference: hardcoded ana-service imports with node metadata |

### Graph representation

- **Nodes**: modules, functions, classes — with attributes: `file_path`, `kind`, `line`, `is_local`
- **Edge types**: `IMPORTS` (module→module), `defines` (module→symbol), `calls` (module→symbol)
- **Local filtering**: `is_local(node)` checks for `app.` prefix to separate project code from stdlib/external
- **Module naming**: file paths normalized to dot notation (`app/services/llm.py` → `app.services.llm`), `__init__.py` collapsed to package name

### Analysis algorithms

- **Hotspot scoring**: normalized composite of betweenness + PageRank + in-degree (equal weights, /3)
- **Risk scoring** (risk_report.py): weighted `0.4*betweenness + 0.4*in_degree + 0.2*in_cycle`
- **Community detection**: Leiden via igraph (ModularityVertexPartition), Louvain fallback, flat fallback
- **Cycle detection**: `nx.strongly_connected_components` (SCCs > 1 node) in graphify.py; `nx.simple_cycles` (capped at 500) in risk_report.py
- **Betweenness**: sampled with `k=min(200, n)` for performance on large graphs

## Conventions

- All CLI scripts use `argparse` (not click, despite scope.md mentioning it)
- Logging via stdlib `logging` at INFO level
- matplotlib uses `Agg` backend (no GUI required)
- Excluded directories during file discovery: `__pycache__`, `node_modules`, `.git`, `dist`, `.next`, `tests`, `__tests__`
- Output defaults: `report/` for graphify.py, `output/` for pipeline.py
- Graph serialization uses `networkx.readwrite.json_graph.node_link_data`

## Learning context

This repo doubles as a ToStudy course workspace ("Graphify: Mapeamento Arquitetural de Codebases com IA e Knowledge Graphs"). The `.cursor/rules/` and `.claude/commands/` contain tutor personas for the course, not Graphify development instructions.
