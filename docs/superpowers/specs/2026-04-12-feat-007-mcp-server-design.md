# FEAT-007: MCP Server for Graph Queries — Design Spec

**Date:** 2026-04-12
**Status:** Approved
**Feature:** FEAT-007

## Summary

Expose Graphify's graph analysis as an MCP (Model Context Protocol) server so AI assistants (Claude Code, Codex, etc.) can programmatically query architecture data. The server runs as a standalone binary (`graphify-mcp`) over stdio, mapping all 9 QueryEngine methods to MCP tools.

## Architecture

### Crate Structure

New workspace member `graphify-mcp` alongside existing crates:

```
crates/
  graphify-core/       # Graph model, metrics, QueryEngine (unchanged)
  graphify-extract/    # tree-sitter AST parsing (unchanged)
  graphify-report/     # Output generation (unchanged)
  graphify-cli/        # CLI binary (unchanged, no MCP deps)
  graphify-mcp/        # NEW: MCP server binary
    Cargo.toml
    src/
      main.rs          # CLI entry: parse args, load config, extract, start server
      server.rs        # GraphifyServer struct — holds HashMap<String, QueryEngine>
      tools.rs         # MCP tool implementations
```

### Dependencies

| Dependency | Purpose |
|---|---|
| `graphify-core` | QueryEngine, types, graph, metrics, community, cycles |
| `graphify-extract` | Walker, extractors (Python, TypeScript) |
| `rmcp` | MCP protocol — JSON-RPC 2.0 over stdio |
| `tokio` | Async runtime (required by rmcp) |
| `serde` + `serde_json` | Serialization |
| `clap` | CLI arg parsing (`--config`) |

### Design Decision: Separate Binary

The MCP server is a separate binary (`graphify-mcp`) rather than a subcommand of `graphify`. This avoids pulling `tokio` and `rmcp` into the lean synchronous CLI binary. Users who don't need MCP don't pay the dependency cost.

### Config Sharing

The config parsing structs (`Config`, `Settings`, `ProjectConfig`) and the extraction pipeline (`run_extract`, `run_analyze`) currently live in `graphify-cli/src/main.rs` as private items. The MCP crate needs the same logic. Two options:

1. **Duplicate the config structs** in `graphify-mcp/src/main.rs` — simpler, no refactoring of existing code.
2. **Extract shared config module** to `graphify-core` or a new `graphify-config` crate.

For now, use option 1 (duplicate). The config structs are small (~20 lines) and stable. If they diverge or grow, extract later. The extraction pipeline functions will also be duplicated — they're thin wrappers around graphify-extract and graphify-core APIs.

## Startup Flow

1. Parse CLI args: `graphify-mcp --config graphify.toml`
2. Read and parse `graphify.toml`
3. For each `[[project]]`:
   a. Discover files via Walker
   b. Extract AST data via tree-sitter (parallelized with rayon)
   c. Build CodeGraph
   d. Compute metrics (betweenness, PageRank, scoring)
   e. Detect communities (Louvain + Label Propagation fallback)
   f. Find cycles (Tarjan SCC + simple cycles)
   g. Create QueryEngine from analyzed data
4. Store all engines in `HashMap<String, QueryEngine>`
5. Set `default_project` to the first project name
6. Start rmcp stdio server

### Design Decision: Eager Extraction

The server extracts all projects on startup (not lazily). This matches the existing CLI pattern (re-extract on the fly for query/explain/path commands) and ensures tool responses are instant. Startup cost is ~1-3s for typical codebases, acceptable since MCP servers are long-lived processes spawned once per session.

## MCP Tool Definitions

All 9 QueryEngine methods are exposed as MCP tools. Every tool accepts an optional `project` parameter. If omitted, the default (first) project is used.

### graphify_stats

**Description:** Get high-level statistics about the dependency graph: node count, edge count, local modules, communities, and cycles.

**Input Schema:**
```json
{
  "type": "object",
  "properties": {
    "project": { "type": "string", "description": "Project name (optional, defaults to first project)" }
  }
}
```

**Returns:** GraphStats JSON — `{ node_count, edge_count, local_node_count, community_count, cycle_count }`

### graphify_search

**Description:** Search for code modules by glob pattern. Supports `*` (any chars) and `?` (single char) wildcards. Filter by kind, sort by score/name/in_degree.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["pattern"],
  "properties": {
    "pattern": { "type": "string", "description": "Glob pattern to match node IDs (e.g. 'app.services.*')" },
    "kind": { "type": "string", "enum": ["module", "function", "class", "method"], "description": "Filter by node kind" },
    "sort": { "type": "string", "enum": ["score", "name", "in_degree"], "default": "score", "description": "Sort results by field" },
    "local_only": { "type": "boolean", "default": false, "description": "Only return local (in-project) nodes" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of QueryMatch — `[{ node_id, kind, file_path, score, community_id, in_cycle }]`

### graphify_explain

**Description:** Get a detailed profile of a code module including metrics (betweenness, PageRank, score), community membership, cycle participation, and direct/transitive dependencies. Use this to understand a module's architectural role.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "node_id": { "type": "string", "description": "Full node ID (e.g. 'app.services.llm')" },
    "project": { "type": "string" }
  }
}
```

**Returns:** ExplainReport JSON — `{ node_id, kind, file_path, language, metrics: { score, betweenness, pagerank, in_degree, out_degree }, community_id, in_cycle, cycle_peers, direct_dependents, direct_dependencies, transitive_dependent_count, top_transitive_dependents }`

### graphify_path

**Description:** Find the shortest dependency path between two modules. Returns the step-by-step path with edge types (Imports/Defines/Calls) and weights.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["source", "target"],
  "properties": {
    "source": { "type": "string", "description": "Source node ID" },
    "target": { "type": "string", "description": "Target node ID" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of PathStep — `[{ node_id, edge_kind, weight }]` or null if no path exists.

### graphify_all_paths

**Description:** Find all dependency paths between two modules, bounded by depth and count limits. Use when you need to understand all possible dependency chains.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["source", "target"],
  "properties": {
    "source": { "type": "string", "description": "Source node ID" },
    "target": { "type": "string", "description": "Target node ID" },
    "max_depth": { "type": "integer", "default": 10, "description": "Maximum path length in edges" },
    "max_paths": { "type": "integer", "default": 20, "description": "Maximum number of paths to return" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of paths, each being an array of PathStep.

### graphify_dependents

**Description:** List modules that depend on (import/call) the given module. Shows who would be affected if this module changes.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "node_id": { "type": "string", "description": "Node ID to find dependents of" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of `[{ node_id, edge_kind }]`

### graphify_dependencies

**Description:** List modules that the given module depends on (imports/calls). Shows what this module needs to function.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "node_id": { "type": "string", "description": "Node ID to find dependencies of" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of `[{ node_id, edge_kind }]`

### graphify_suggest

**Description:** Autocomplete node IDs. Returns up to 3 suggestions matching the input as a case-insensitive substring. Use this before other tools when you're unsure of exact node names.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["input"],
  "properties": {
    "input": { "type": "string", "description": "Partial node ID to autocomplete" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of up to 3 node ID strings.

### graphify_transitive_dependents

**Description:** Find all transitive dependents of a module up to N hops away. Shows the full blast radius — everything that would be affected by changes to this module.

**Input Schema:**
```json
{
  "type": "object",
  "required": ["node_id"],
  "properties": {
    "node_id": { "type": "string", "description": "Node ID to find transitive dependents of" },
    "max_depth": { "type": "integer", "default": 5, "description": "Maximum number of hops to traverse" },
    "project": { "type": "string" }
  }
}
```

**Returns:** Array of `[{ node_id, depth }]` sorted by depth ascending.

## GraphifyServer Structure

```rust
struct GraphifyServer {
    engines: HashMap<String, QueryEngine>,
    default_project: String,
    project_names: Vec<String>,
}
```

### Project Resolution

Every tool call resolves the project parameter:

1. If `project` is provided and exists in `engines` → use it
2. If `project` is provided but doesn't exist → return error listing available projects
3. If `project` is omitted → use `default_project` (first in config)

This logic lives in a shared `resolve_engine(&self, project: Option<&str>) -> Result<&QueryEngine, String>` method.

## Error Handling

| Scenario | Behavior |
|---|---|
| Config file not found | Exit with error before starting MCP |
| Project extraction fails | Log warning to stderr, skip project, continue |
| All extractions fail | Exit with error (no point running empty server) |
| Unknown project in tool call | MCP error response listing available projects |
| Non-existent node_id | Return empty/null result (not an error) |
| Missing required parameter | JSON Schema validation by rmcp framework |

## Transport

**stdio only** — the server reads JSON-RPC 2.0 from stdin and writes responses to stdout. No HTTP, no port binding. This is the standard transport for Claude Code MCP servers.

Diagnostic messages (extraction progress, warnings) go to stderr so they don't interfere with the JSON-RPC protocol on stdout.

## Client Configuration

### Claude Code (`.mcp.json` or `~/.claude.json`)

```json
{
  "mcpServers": {
    "graphify": {
      "command": "graphify-mcp",
      "args": ["--config", "/path/to/graphify.toml"]
    }
  }
}
```

### Claude Desktop (`claude_desktop_config.json`)

Same schema under `mcpServers`.

## Testing

### Unit Tests (`src/tools.rs`)

- Build a small in-memory CodeGraph with known structure
- Create QueryEngine from it
- Call each tool handler directly
- Verify JSON output structure and content
- Test project resolution: default, explicit valid, explicit invalid
- Test parameter defaults: max_depth, max_paths, sort

### Integration Tests (`tests/integration.rs`)

- Create a temporary directory with Python/TypeScript fixture files
- Write a `graphify.toml` pointing at the fixtures
- Spawn `graphify-mcp` as a child process
- Send JSON-RPC messages to stdin:
  - `initialize` → verify server capabilities include tools
  - `tools/list` → verify all 9 tools declared with correct schemas
  - `tools/call` for each tool → verify valid JSON responses
- Verify stderr doesn't contain errors

### Manual Smoke Test

- Configure Claude Code to use `graphify-mcp` on the Graphify codebase itself
- Ask Claude to "explain the most critical module" → should invoke `graphify_stats` + `graphify_search` + `graphify_explain`
- Ask "what depends on graphify_core::query" → should invoke `graphify_dependents`

## Build & Release

- Add `graphify-mcp` to workspace members in root `Cargo.toml`
- Binary built alongside `graphify`: `cargo build --release -p graphify-mcp`
- CI: add `graphify-mcp` to the release workflow (builds for same 4 targets)
- Binary name: `graphify-mcp` (distinct from `graphify`)

## Future Extensions (out of scope)

- **MCP Resources**: expose graph.json and analysis.json as MCP resources (not tools)
- **MCP Prompts**: pre-built prompt templates for common analysis workflows
- **HTTP/SSE transport**: for remote/networked servers
- **Hot reload**: watch source files and re-extract when they change (pairs with FEAT-010)
- **Streaming**: stream large result sets instead of returning all at once
