---
title: "graphify analyze"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - pipeline
related:
  - "[[CLI Reference]]"
  - "[[extract]]"
  - "[[report]]"
---

# `graphify analyze`

Run extraction **plus** analysis: betweenness centrality, PageRank, community detection (Louvain + Label Propagation fallback), cycle detection (Tarjan SCC + simple cycles). Writes `analysis.json` and CSV files per project.

## Synopsis

```bash
graphify analyze [--config <path>] [--output <dir>] [--weights <floats>] [--force]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--output <dir>` | `[settings].output` | Override output directory |
| `--weights <list>` | from config or `0.4,0.2,0.2,0.2` | Comma-separated unified-score weights: `betweenness,pagerank,in_degree,in_cycle` |
| `--force` | `false` | Bypass SHA256 extraction cache |

## Examples

```bash
# Default weights
graphify analyze

# Replicate the Python "hotspot" score (no in_cycle weight)
graphify analyze --weights 0.33,0.33,0.33,0.0

# Replicate the Python "risk" score
graphify analyze --weights 0.4,0.0,0.4,0.2

# Use a different config + output
graphify analyze --config dev.toml --output ./out
```

## Output

Per project under `<output>/<project>/`:

| File | Description |
|---|---|
| `graph.json` | (carried over from extract) |
| `analysis.json` | Metrics + communities + cycles + confidence summary |
| `graph_nodes.csv` | Per-node metrics (id, kind, file, scores) |
| `graph_edges.csv` | Per-edge data (source, target, kind, weight, line, confidence, confidence_kind) |
| `.graphify-cache.json` | SHA256 extraction cache |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Analysis succeeded for all projects |
| 1 | Config error or fatal IO error |

## Gotchas

- **Weights must sum to ~1.0** for a meaningful unified score, but Graphify doesn't enforce it.
- **Cycle enumeration is capped at 500 simple cycles per SCC** — large dense graphs may report cycles partially.
- **Communities are not stable across runs.** Louvain assigns IDs based on processing order. Use [[diff]] for cross-version comparison; it handles ID renumbering via max-overlap matching.

## See also

- [[extract]] — extraction-only stage
- [[report]] — generates Markdown/HTML/etc. on top of analysis output
- [[run]] — full pipeline alias
- [[Glossary - Terms]] — definitions of every metric
