---
uid: feat-007
status: done
completed: 2026-04-12
priority: normal
timeEstimate: 960
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - integration
  - mcp
---

# MCP server for graph queries

Expose the graph as an MCP (Model Context Protocol) server so AI assistants can query architecture data programmatically.

## Goals

- `graphify serve --port 3000` or stdio-based MCP server
- MCP tools: `query_graph`, `get_node`, `get_neighbors`, `shortest_path`, `list_communities`, `get_hotspots`
- Serve from existing graph.json + analysis.json
- Compatible with Claude Code, Codex, and other MCP-capable assistants

## Inspiration

safishamsi/graphify exposes graph.json as an MCP stdio server (`python -m graphify.serve graph.json`). Any MCP client can query nodes, edges, paths without manual JSON parsing. This makes the graph a live tool for AI-assisted development.

## Subtasks

- [x] Research MCP protocol requirements (stdio vs HTTP)
- [x] Choose Rust MCP library or implement minimal protocol
- [x] Implement graph loading from JSON
- [x] Define tool schemas (query, node, neighbors, path, communities, hotspots)
- [x] Implement each tool handler
- [x] Add `serve` subcommand to CLI
- [x] Integration tests with MCP client
- [x] Documentation: how to configure in Claude Code / Codex

## Notes

This is the highest-leverage AI integration feature. Instead of an assistant reading the raw report, it can programmatically traverse the graph. Depends on FEAT-006 (query interface) for shared graph query logic.
