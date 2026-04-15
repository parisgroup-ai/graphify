---
title: "graphify report"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - pipeline
related:
  - "[[CLI Reference]]"
  - "[[run]]"
  - "[[analyze]]"
---

# `graphify report`

Run the full pipeline (extract → analyze → report) and emit **all configured output formats** per project. This is the most commonly used command.

## Synopsis

```bash
graphify report [--config <path>] [--output <dir>] [--weights <floats>] [--format <list>] [--force]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--output <dir>` | `[settings].output` | Override output directory |
| `--weights <list>` | from config | Unified-score weights (see [[analyze]]) |
| `--format <list>` | `[settings].format` | Comma-separated formats: `json,csv,md,html,neo4j,graphml,obsidian` |
| `--force` | `false` | Bypass SHA256 extraction cache |

## Examples

```bash
# Use everything from graphify.toml
graphify report

# Override formats — Markdown + interactive HTML only
graphify report --format md,html

# Generate Neo4j Cypher script for graph DB import
graphify report --format neo4j

# Generate an Obsidian vault for note-style exploration
graphify report --format obsidian
```

## Output

Per project under `<output>/<project>/`. Subset depends on `--format`:

| File | Format value |
|---|---|
| `graph.json` | always |
| `analysis.json` | always |
| `graph_nodes.csv`, `graph_edges.csv` | `csv` |
| `architecture_report.md` | `md` |
| `architecture_graph.html` | `html` |
| `graph.cypher` | `neo4j` |
| `graph.graphml` | `graphml` |
| `obsidian_vault/` | `obsidian` |

When 2+ projects are configured, also writes:
- `<output>/graphify-summary.json` — aggregate per-project stats and cross-project edges (no full edge list)

> [!info] Stale-dir cleanup
> Output directories for projects no longer in `graphify.toml` are pruned automatically (BUG-013) — but only when they contain only Graphify-generated artifacts. Custom files are left alone.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Reports generated for all projects |
| 1 | Config error, write failure, or fatal IO error |

## Gotchas

- `report` does **not** evaluate gates. Use [[check]] in CI.
- The HTML file embeds D3.js (~260KB) every time — many projects = many copies. By design ([[ADR-002 Interactive HTML Visualization]]).
- Format selection is global (applies to every project). Per-project formats are not supported in v1.

## See also

- [[run]] — alias for `report` (same behavior)
- [[extract]], [[analyze]] — individual pipeline stages
- [[check]] — CI gate evaluation
- [[Configuration#Supported `format` values]]
