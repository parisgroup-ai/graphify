# FEAT-006: Graph Query Interface — Design Spec

**Date:** 2026-04-12
**Status:** Approved
**Tracks:** FEAT-006 in `docs/TaskNotes/Tasks/sprint.md`

## Summary

Add four CLI subcommands (`query`, `path`, `explain`, `shell`) that let users search nodes, trace dependency paths, understand module impact, and explore the graph interactively. All commands re-extract from source on every invocation (always-fresh data). A `QueryEngine` struct in `graphify-core` encapsulates all query logic, keeping it reusable for FEAT-007 (MCP server).

## CLI Surface

### `graphify query <pattern>`

Search nodes by glob pattern against node IDs.

```bash
graphify query "app.services.*"                # glob match
graphify query "llm" --kind module             # filter by NodeKind
graphify query "llm" --sort score              # sort by metric (default: score desc)
graphify query "llm" --project api             # single project only
graphify query "llm" --json                    # machine-readable output
```

Output (human):
```
Matches (4 nodes):

  app.services.llm         Module   score=0.847  community=2  ●cycle
  app.services.auth        Module   score=0.623  community=1
  app.services.billing     Module   score=0.412  community=1
  app.services.cache       Module   score=0.201  community=3
```

### `graphify path <source> <target>`

Find dependency paths between two nodes.

```bash
graphify path app.main app.services.llm                  # shortest path
graphify path app.main app.services.llm --all             # all paths
graphify path app.main app.services.llm --max-depth 10    # limit depth
graphify path app.main app.services.llm --max-paths 20    # limit count (default: 20)
graphify path app.main app.services.llm --json
```

Output (human):
```
app.main ─[Imports]→ app.routes.api ─[Imports]→ app.services.llm

  3 hops, 2 edges
```

Paths are found within a single project's graph only. Cross-project paths are not supported (separate dependency graphs).

### `graphify explain <module>`

Profile card + impact analysis for a single node.

```bash
graphify explain app.services.llm
graphify explain app.services.llm --json
```

Output (human):
```
═══ app.services.llm ═══════════════════════════════════

  Kind:        Module
  File:        app/services/llm.py
  Language:    Python
  Community:   2
  In cycle:    yes (with: app.services.auth, app.services.cache)

  ── Metrics ──
  Score:         0.847
  Betweenness:   12.500
  PageRank:      0.034
  In-degree:     8
  Out-degree:    3

  ── Dependencies (3) ──
  → app.config           Imports
  → app.models.prompt    Imports
  → app.utils.retry      Imports

  ── Dependents (8) ──
  ← app.routes.api       Imports
  ← app.routes.chat      Imports
  ← app.services.auth    Imports
  ... and 5 more

  ── Impact ──
  Transitive dependents: 14 modules
  Blast radius: 58% of local codebase
```

### `graphify shell`

Interactive REPL for iterative exploration.

```bash
graphify shell --config graphify.toml
```

```
graphify> query app.services.*
graphify> path app.main app.services.llm
graphify> explain app.services.llm
graphify> stats
graphify> help
graphify> exit
```

Design constraints:
- Runs extract+analyze once on startup; graph is frozen for the session.
- No readline/rustyline dependency. Plain `stdin` line reading.
- Same output formatting as one-shot commands (human-readable only, no `--json`).
- Invalid commands print helpful messages and return to prompt; never crash.

## Architecture

### QueryEngine (`graphify-core/src/query.rs`)

```rust
pub struct QueryEngine {
    graph: CodeGraph,
    metrics: Vec<NodeMetrics>,
    communities: Vec<Community>,
    cycles: Vec<CycleGroup>,
}
```

**Construction:** `QueryEngine::from_analyzed(graph, metrics, communities, cycles)` takes ownership of fully-computed data. The CLI runs extract+analyze, then passes results to `QueryEngine`.

### Methods

| Method | Signature | Description |
|---|---|---|
| `search` | `(&self, pattern: &str, filters: SearchFilters) -> Vec<QueryMatch>` | Glob match on node IDs with optional filters |
| `shortest_path` | `(&self, from: &str, to: &str) -> Option<Vec<PathStep>>` | BFS shortest path |
| `all_paths` | `(&self, from: &str, to: &str, max_depth: usize, max_paths: usize) -> Vec<Vec<PathStep>>` | DFS all paths with caps |
| `explain` | `(&self, node_id: &str) -> Option<ExplainReport>` | Profile card + impact analysis |
| `dependents` | `(&self, node_id: &str) -> Vec<(&str, &EdgeKind)>` | Direct incoming neighbors with edge kind |
| `dependencies` | `(&self, node_id: &str) -> Vec<(&str, &EdgeKind)>` | Direct outgoing neighbors with edge kind |
| `transitive_dependents` | `(&self, node_id: &str, max_depth: usize) -> Vec<(String, usize)>` | Transitive closure with depth |
| `suggest` | `(&self, input: &str) -> Vec<&str>` | Substring fuzzy suggestions (up to 3) |
| `stats` | `(&self) -> GraphStats` | Summary counts |

### Result Types

```rust
pub struct QueryMatch {
    pub node_id: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub score: f64,
    pub community_id: usize,
    pub in_cycle: bool,
}

pub struct PathStep {
    pub node_id: String,
    pub edge_kind: Option<EdgeKind>,  // None for the last node in the path
    pub weight: u32,                   // 0 for the last node
}

pub struct ExplainReport {
    pub node_id: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub language: Language,
    pub metrics: ExplainMetrics,
    pub community_id: usize,
    pub in_cycle: bool,
    pub cycle_peers: Vec<String>,
    pub direct_dependents: Vec<String>,
    pub direct_dependencies: Vec<String>,
    pub transitive_dependent_count: usize,
    pub top_transitive_dependents: Vec<String>,
}

pub struct ExplainMetrics {
    pub score: f64,
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: usize,
    pub out_degree: usize,
}

pub struct SearchFilters {
    pub kind: Option<NodeKind>,
    pub sort_by: SortField,
    pub local_only: bool,
}

pub enum SortField {
    Score,
    Name,
    InDegree,
}

pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub local_node_count: usize,
    pub community_count: usize,
    pub cycle_count: usize,
}
```

All result types derive `Serialize` for `--json` output.

### Responsibility Split

| Layer | Responsibility |
|---|---|
| `graphify-core/src/query.rs` | `QueryEngine` struct, all query logic, result types |
| `graphify-cli/src/main.rs` | CLI argument parsing, pipeline orchestration, output formatting, REPL loop |

Formatting lives in the CLI, not in core. This keeps core reusable for FEAT-007 (MCP server).

## Multi-Project Behavior

When `graphify.toml` has multiple `[[project]]` entries:

- **`query`** and **`explain`**: operate across all projects by default. `--project <name>` filters to one.
- **`path`**: finds paths within a single project's graph only. If `--project` is not specified, tries all projects and reports the first match. Cross-project paths are not supported.
- **`shell`**: loads all projects. `stats` shows per-project breakdown.

For single-project configs, `--project` is unnecessary and ignored.

## Node Resolution & Error Handling

All commands that accept a node ID:
1. Try exact match first.
2. If not found, run substring matching against all node IDs.
3. Show up to 3 fuzzy suggestions.

```
Error: node "app.service.llm" not found.
Did you mean: app.services.llm?
```

Other error cases:
- `query` with no matches: "No nodes matching `<pattern>`"
- `path` with no route: "No path from `<A>` to `<B>`"
- `explain` on external (non-local) node: works, but metrics are minimal
- Config not found: error message + exit code 1 (same as existing commands)
- `--all` path cap: default 20 paths max, overridable with `--max-paths <n>`

## Testing Plan

### Unit tests (`graphify-core/src/query.rs`)

| Test | Validates |
|---|---|
| `search_glob_matches` | Glob pattern returns expected nodes |
| `search_no_matches` | Unknown pattern returns empty vec |
| `search_filter_by_kind` | Kind filter works correctly |
| `shortest_path_direct` | Direct edge A→B |
| `shortest_path_transitive` | Multi-hop A→B→C |
| `shortest_path_no_route` | Returns `None` for disconnected nodes |
| `all_paths_respects_max_depth` | Depth cap enforced |
| `all_paths_capped_count` | Path count cap enforced |
| `explain_known_node` | Complete `ExplainReport` returned |
| `explain_unknown_node` | Returns `None` |
| `explain_cycle_peers` | Cycle co-members listed correctly |
| `dependents_returns_incoming` | Direct incoming neighbors |
| `dependencies_returns_outgoing` | Direct outgoing neighbors |
| `transitive_dependents_depth` | Depth tracking correct |
| `transitive_dependents_cap` | Max depth respected |
| `fuzzy_suggest_substring` | Substring suggestions work |

### Integration tests (`tests/`)

| Test | Validates |
|---|---|
| `query_command_runs` | CLI exits 0, produces output |
| `path_command_json` | `--json` produces valid JSON |
| `explain_command_output` | Contains expected sections |
| `shell_command_exits` | REPL handles `exit` cleanly |

Estimated: ~20 new tests, total from 150 to ~170.

## Dependencies

No new external crates. Glob matching uses simple wildcard-to-regex conversion (manual, not a crate). BFS/DFS use petgraph's existing graph traversal. REPL uses `std::io::BufRead`.

## Future Extensibility

- **FEAT-007 (MCP server)**: imports `QueryEngine` directly from `graphify-core`.
- **Readline/history**: swap `stdin` reader for `rustyline` if demand exists.
- **Re-extract in REPL**: add a `reload` command that re-runs the pipeline.
- **Output formats**: add `--format table|csv|json` if needed beyond `--json`.
