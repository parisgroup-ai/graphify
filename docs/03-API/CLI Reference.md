---
title: CLI Reference
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/reference
  - type/cli
  - type/index
related:
  - "[[🏠 Home]]"
  - "[[🔍 Quick Reference]]"
  - "[[Configuration]]"
---

# CLI Reference

Per-command reference for the `graphify` binary. Each command page documents arguments, flags, output, exit codes, and gotchas. For a one-page cheat sheet, see [[🔍 Quick Reference]].

## Convention

- All errors exit **1** (uniform across the CLI — see [[ADR-012 PR Summary CLI]]).
- `--config` defaults to `./graphify.toml`.
- `--output` defaults to `./report` (or `[settings].output` from config).
- All commands respect the SHA256 extraction cache by default; pass `--force` to bypass.

## Commands

### Pipeline

| Command | Purpose | Page |
|---|---|---|
| `graphify init` | Generate a starter `graphify.toml` | [[init]] |
| `graphify extract` | Discovery + AST → `graph.json` per project | [[extract]] |
| `graphify analyze` | Extract + metrics → `analysis.json` + CSV | [[analyze]] |
| `graphify report` | Full pipeline + every configured format | [[report]] |
| `graphify run` | Alias of `report` (kept for back-compat) | [[run]] |
| `graphify watch` | Re-run pipeline on file change | [[watch]] |

### Quality & drift

| Command | Purpose | Page |
|---|---|---|
| `graphify check` | Evaluate CI gates (cycles, hotspots, policy, contracts) | [[check]] |
| `graphify diff` | Compare two analysis snapshots | [[diff]] |
| `graphify trend` | Aggregate historical snapshots → trend report | [[trend]] |
| `graphify pr-summary <DIR>` | Render PR-ready Markdown from JSON artifacts | [[pr-summary]] |

### Query / exploration

| Command | Purpose | Page |
|---|---|---|
| `graphify query <pat>` | Glob-search nodes | [[query]] |
| `graphify explain <node>` | Profile + impact for a single node | [[explain]] |
| `graphify path <a> <b>` | Find dependency paths between nodes | [[path]] |
| `graphify shell` | Interactive REPL | [[shell]] |

## Companion binary

| Command | Purpose | Page |
|---|---|---|
| `graphify-mcp` | MCP server exposing graph queries to AI assistants | [[MCP Server]] |

## Output file map

Where each command writes its output (per project, under `<output>/<project>/` unless noted):

| File | Producer |
|---|---|
| `graph.json` | `extract` (NetworkX `node_link_data` schema) |
| `graph_nodes.csv`, `graph_edges.csv` | `analyze` |
| `analysis.json` | `analyze` |
| `architecture_report.md` | `report` |
| `architecture_graph.html` | `report` (D3.js, self-contained) |
| `graph.cypher` | `report` (Neo4j import) |
| `graph.graphml` | `report` (yEd / Gephi) |
| `obsidian_vault/` | `report` (Obsidian markdown notes) |
| `check-report.json` | `check` (always written, regardless of `--json`) |
| `drift-report.{json,md}` | `diff` (current dir or `--output`) |
| `trend-report.{json,md}` | `trend` |
| `graphify-summary.json` | `run` (only when 2+ projects) |
| `<stdout>` | `pr-summary`, `query --json`, etc. |

## Cache behavior at a glance

| Command | Uses cache? |
|---|---|
| `extract`, `analyze`, `report`, `run`, `check`, `watch`, `diff` (baseline mode) | yes — bypass with `--force` |
| `query`, `path`, `explain`, `shell`, `pr-summary`, `init` | no — fresh extraction or no extraction at all |

## Related

- [[🏠 Home]]
- [[🔍 Quick Reference]] — cheat sheet
- [[Configuration]] — `graphify.toml` reference
- [[Crate - graphify-cli]] — internal architecture
- [[ADR-Index]] — design decisions behind these commands
