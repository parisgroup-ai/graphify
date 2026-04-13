---
uid: feat-006
status: done
completed: 2026-04-13
priority: high
timeEstimate: 960
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - cli
  - graph
---

# Graph query interface

Add subcommands to query a previously-built graph without re-running extraction.

## Goals

- `graphify query "auth flow"` — keyword search across nodes/edges, return subgraph
- `graphify path <node-a> <node-b>` — shortest path between two modules
- `graphify explain <node>` — show a node's connections, metrics, community membership
- `graphify neighbors <node> --depth 2` — show N-hop neighborhood
- All queries operate on existing `graph.json` + `analysis.json` (no re-extraction)

## Inspiration

safishamsi/graphify has `query`, `path`, `explain` subcommands that operate on a persistent graph.json. Users can explore architecture without re-paying extraction cost. This turns the graph from a one-shot report into a reusable knowledge base.

## Subtasks

- [x] Design query subcommand structure in clap
- [x] Implement graph.json deserialization back into CodeGraph
- [x] `query` — fuzzy text search on node IDs, file paths, kinds
- [x] `path` — shortest path between modules
- [x] `explain` — node detail: degree, betweenness, community, edges
- [x] `neighbors` / multi-path traversal via query engine path APIs
- [x] Formatted terminal output (table or tree)
- [x] Tests for each subcommand

## Notes

This is high value — transforms Graphify from a "generate report" tool to an interactive exploration tool. Depends on stable graph.json serialization format (already NetworkX node_link_data compatible).

## Verification (2026-04-13)

- Verified CLI exposes `query`, `explain`, and `path` commands in `graphify --help`
- Verified query engine exists in `crates/graphify-core/src/query.rs`
- Verified CLI uses `QueryEngine::from_analyzed(...)` and path traversal methods
