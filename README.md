# Graphify

Architectural analysis of codebases. Extracts dependencies via tree-sitter, builds knowledge graphs, identifies hotspots, circular dependencies, and community clusters.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/parisgroup/graphify/main/install.sh | sh
```

Or download from [Releases](https://github.com/parisgroup/graphify/releases).

## Quick Start

```bash
graphify init
# Edit graphify.toml
graphify report
```

## Configuration

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests"]
format = ["json", "csv", "md"]

[[project]]
name = "my-app"
repo = "./apps/my-app"
lang = ["python"]
local_prefix = "app."
```

## Commands

| Command | Description |
|---------|-------------|
| `graphify init` | Generate graphify.toml |
| `graphify extract` | Extract dependency graph |
| `graphify analyze` | Extract + compute metrics |
| `graphify report` | Full pipeline with all outputs |
| `graphify run` | Alias for report |

## Output

Each project produces:
- `graph.json` — dependency graph (NetworkX node_link_data format)
- `analysis.json` — metrics, communities, cycles
- `graph_nodes.csv` — node metrics
- `graph_edges.csv` — edge list
- `architecture_report.md` — human-readable report

## Supported Languages

- Python
- TypeScript

## License

MIT
