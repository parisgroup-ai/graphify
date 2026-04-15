---
title: Glossary
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/glossary
  - reference
related:
  - "[[🏠 Home]]"
  - "[[System Overview]]"
---

# Glossary

Concepts and jargon used across Graphify docs and reports.

## Graph model

### Node
A vertex in the dependency graph. Represents a **module**, **function**, **class**, **enum**, or **trait**. Each node carries `id`, `kind`, `file_path`, `language`, `line`, and `is_local`.

### Edge
A directed relationship between two nodes. Three types:

| Kind | Meaning |
|---|---|
| `Imports` | Module → module (one source imports another) |
| `Defines` | Module → symbol (a module declares a function/class) |
| `Calls` | Module → symbol (a module invokes a callable) |

### Edge weight
Repeated occurrences of the same edge increment a `weight` counter rather than creating duplicates. A `Calls` edge with weight 5 means the source calls the target 5 times.

### Module name
File paths normalized to dot notation (`app/services/llm.py` → `app.services.llm`). Package entry points (`__init__.py` for Python, `index.ts` for TypeScript) collapse to their parent (`app/services/__init__.py` → `app.services`).

### `is_package`
Tracks whether a discovered file is a package entry point (`__init__.py` / `index.ts`). The resolver uses this to correctly resolve relative imports — fixing BUG-001.

### `local_prefix`
The module prefix that marks a node as **local** to the project (e.g., `app` in `app.services.llm`). Used to distinguish first-party code from dependencies. Auto-detected via `src/`/`app/` heuristic when omitted (FEAT-011).

## Confidence (FEAT-008)

### Confidence score
A `f64` from 0.0 to 1.0 attached to every edge, indicating how sure the extractor is that the edge is correct.

### `ConfidenceKind`
A label paired with the score:

| Variant | Meaning |
|---|---|
| `Extracted` | Direct AST evidence (highest confidence) |
| `Inferred` | Derived from incomplete evidence (e.g., bare call site) |
| `Ambiguous` | Pattern matched but target uncertain (e.g., non-local call) |

### Resolver confidence values

| Resolution path | Score |
|---|---|
| Direct match | 1.0 |
| Python relative import | 0.9 |
| TypeScript relative import | 0.9 |
| TypeScript path alias | 0.85 |
| Bare call site (unqualified) | 0.7 / Inferred |
| Non-local downgrade | min(score, 0.5) → Ambiguous |

When the same edge is observed multiple times, Graphify keeps the **maximum** confidence across observations.

## Metrics

### Betweenness centrality
How often a node sits on the shortest path between other nodes. High betweenness = bottleneck. Computed via Brandes algorithm with sampling (`k = min(200, n)`).

### PageRank
Adapted from web ranking. High PageRank = many nodes (especially other high-rank nodes) depend on this one. Iterative, damping = 0.85.

### In-degree
Raw count of incoming edges. Simple but informative — heavily depended-upon nodes have high in-degree.

### In-cycle
Whether the node participates in any directed cycle. Boolean → contributes 0 or 1 to the unified score.

### Unified score
Weighted combination of the four metrics above. Default weights:

```text
score = 0.4 × betweenness
      + 0.2 × pagerank
      + 0.2 × in_degree
      + 0.2 × in_cycle
```

Configurable via `[settings].weights`.

### Hotspot
A node with a unified score above the configured `hotspot_threshold` (default 0.05). Hotspots are the architectural risk surface — refactoring them has the largest blast radius.

## Graph structure

### Cycle (circular dependency)
A directed path that returns to its starting node. Detected via Tarjan's strongly connected components, then enumerated with DFS (capped at 500 simple cycles per SCC to bound runtime).

### SCC (strongly connected component)
A maximal subgraph where every node is reachable from every other. Every cycle lives inside an SCC. Reported by `graphify analyze`.

### Community
A cluster of nodes more densely connected to each other than to the rest of the graph. Detected via **Louvain** (default) with a **Label Propagation** fallback when Louvain degenerates on sparse graphs (BUG-008). Phase 2 of Louvain merges singleton communities to reduce noise.

## Pipeline / outputs

### Snapshot
A saved `analysis.json` representing the architecture at a point in time. The basis for [[#Drift]] detection and `graphify trend`.

### Drift
The difference between two snapshots, across 5 dimensions: summary, edges, cycles, hotspots, communities. Reported by `graphify diff` as `drift-report.{json,md}`.

### Cross-project summary
`graphify-summary.json` — aggregate stats across all configured projects, written only when 2+ projects are configured. Contains per-project stats and cross-project edges; **no full edge list** (BUG-010).

### Check report
`check-report.json` — unified machine-readable output from `graphify check`. Combines built-in gates (cycles, hotspots) with policy violations and contract drift. Written unconditionally per project so `graphify pr-summary` can consume it (FEAT-015).

### PR summary
Markdown rendering produced by `graphify pr-summary <DIR>`. Pure renderer over `analysis.json` (required) plus `drift-report.json` and `check-report.json` (optional). Designed to paste into a GitHub PR.

## Policy & contracts

### Policy rule (FEAT-013)
Declarative architecture rule in `graphify.toml`. Combines `policy.group` (selectors over module names) with `policy.rule` (allow/deny edges between groups). Evaluated by `graphify check`.

### Contract drift (FEAT-016)
Mismatch between an ORM schema (currently Drizzle) and a TypeScript type. Detected by comparing field names (snake_case ↔ camelCase normalized), types (built-in map + overrides), and relation cardinality.

## Caching & build

### Extraction cache
`.graphify-cache.json` written next to each project's report. SHA256-keyed per file. Bypassed by `--force` and by all query commands. Discarded entirely on Graphify version change or `local_prefix` change.

### Cache hit/miss stats
`graphify run` reports `cached: N/M` per project on stderr. Useful to confirm the cache is doing its job.

## CLI mechanics

### Watch mode (FEAT-010)
`graphify watch` — file-system observer (notify v7 + 300ms debounce) that re-runs only the affected projects on change. `--force` applies only to the initial build.

### MCP server (FEAT-007)
`graphify mcp` — JSON-RPC server over stdio that exposes 9 tools backed by `QueryEngine`. Built on `rmcp` with eager extraction at startup. Used by AI assistants to query the graph.

### Trend (FEAT-014)
`graphify trend` — aggregates a directory of historical snapshots into `trend-report.{json,md}`. Useful for tracking architectural health over weeks/months.

## Related

- [[🏠 Home]]
- [[System Overview]]
- [[🔍 Quick Reference]]
