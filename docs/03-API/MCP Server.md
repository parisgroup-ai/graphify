---
title: "graphify-mcp"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - integration
  - mcp
related:
  - "[[CLI Reference]]"
  - "[[Crate - graphify-mcp]]"
  - "[[ADR-005 MCP Server]]"
---

# `graphify-mcp`

Companion binary that exposes Graphify's `QueryEngine` as **Model Context Protocol** tools. AI assistants (Claude Code, Codex, etc.) connect over **stdio** and call 9 tools to query the dependency graph during a coding session.

## Synopsis

```bash
graphify-mcp --config <path>
```

Not a subcommand of `graphify` — a separate binary in the same workspace, built independently:

```bash
cargo build --release -p graphify-mcp
# Binary at target/release/graphify-mcp
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |

## Transport

**stdio**, JSON-RPC 2.0. The server reads requests from stdin and writes responses to stdout. **Diagnostics go to stderr** — anything written to stdout that isn't valid JSON-RPC corrupts the protocol.

## Tools exposed

| Tool | Description |
|---|---|
| `graphify_stats` | Graph stats: node count, edge count, communities, cycles |
| `graphify_search` | Glob-search nodes with optional filters (`pattern`, `kind`, `sort`, `local_only`, `min_confidence`) |
| `graphify_explain` | Full `ExplainReport` for one node |
| `graphify_path` | Shortest dependency path between two nodes |
| `graphify_all_paths` | All paths up to `max_depth` / `max_paths` |
| `graphify_dependents` | Direct incoming neighbors |
| `graphify_dependencies` | Direct outgoing neighbors |
| `graphify_transitive_dependents` | Transitive closure with depth tracking |
| `graphify_suggest` | Up to 3 substring autocomplete suggestions |

Every tool accepts an optional `project` parameter — defaults to the **first** project in `graphify.toml`.

## Startup

1. Parse `--config`
2. For each `[[project]]`: discover files, extract via tree-sitter, build `CodeGraph`, compute metrics + communities + cycles
3. Wrap each result in `Arc<QueryEngine>`, store in `HashMap`
4. Begin reading JSON-RPC from stdin

Eager extraction takes 1–3s for typical codebases. Acceptable because MCP servers are long-lived (one per editor session).

## Client configuration

### Claude Code (`.mcp.json` or `~/.claude.json`)

```json
{
  "mcpServers": {
    "graphify": {
      "command": "graphify-mcp",
      "args": ["--config", "/absolute/path/to/graphify.toml"]
    }
  }
}
```

### Claude Desktop (`claude_desktop_config.json`)

Same schema under `mcpServers`.

## Example tool call (over JSON-RPC)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "graphify_search",
    "arguments": {
      "pattern": "app.services.*",
      "kind": "module",
      "sort": "score"
    }
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      { "type": "text", "text": "[{\"node_id\":\"app.services.llm\",\"kind\":\"Module\",\"score\":0.847,...}]" }
    ]
  }
}
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Clean exit (stdin closed by client) |
| 1 | Config error, all extractions failed, or fatal startup error |

## Gotchas

- **stdout pollution kills the protocol.** Don't wrap the binary in something that prints to stdout. Redirect stderr only.
- **Graph is frozen for the session.** Code edits aren't reflected. Restart the MCP server (most editor MCP integrations have a "reload" command).
- **Default project = first in config.** If you have a multi-project setup, always specify `project` in tool calls.
- **rmcp 0.1 is pre-1.0.** API may shift in future versions; pin the version.
- **No incremental refresh** is implemented yet — pairing MCP with file-watch is on the roadmap, not done.

## See also

- [[Crate - graphify-mcp]] — internal architecture
- [[ADR-005 MCP Server]] — design rationale (separate binary, duplicated config, eager extract)
- [[ADR-004 Graph Query Interface]] — `QueryEngine` is the shared backbone with the CLI
