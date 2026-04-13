# Graphify

Architectural analysis of codebases. Extracts dependencies via tree-sitter AST parsing, builds knowledge graphs with petgraph, and generates structured reports identifying hotspots, circular dependencies, and community clusters.

Distributed as a single static binary — no runtime dependencies. macOS + Linux.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/parisgroup-ai/graphify/main/install.sh | sh
```

Or download from [Releases](https://github.com/parisgroup-ai/graphify/releases).

Build from source:

```bash
cargo install --path crates/graphify-cli
```

## Quick Start

```bash
graphify init          # generate graphify.toml
# edit graphify.toml to point at your project(s)
graphify run           # extract → analyze → report
```

## Configuration

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]  # betweenness, pagerank, in_degree, in_cycle
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests"]
format = ["json", "csv", "md", "html"]  # also: neo4j, graphml, obsidian

[[project]]
name = "my-app"
repo = "./apps/my-app"
lang = ["python"]
local_prefix = "app"
```

Multiple `[[project]]` sections enable monorepo analysis. Each project gets its own output subdirectory.

## Commands

| Command | Description |
|---------|-------------|
| `graphify init` | Generate a `graphify.toml` template |
| `graphify extract` | Extract dependency graph (produces `graph.json`) |
| `graphify analyze` | Extract + compute metrics (produces `analysis.json`, CSV) |
| `graphify report` | Full pipeline with all outputs |
| `graphify run` | Alias for `report` |
| `graphify query "pattern"` | Search nodes by glob pattern |
| `graphify explain <node>` | Module profile card + impact analysis |
| `graphify path <source> <target>` | Find dependency paths between modules |
| `graphify diff` | Detect architectural drift between snapshots |
| `graphify check` | Validate quality gates for CI (max cycles, hotspot score) |
| `graphify watch` | Auto-rebuild on file changes (300ms debounce) |
| `graphify shell` | Interactive graph exploration REPL |

### Flags

| Flag | Applies to | Description |
|------|-----------|-------------|
| `--config <path>` | all | Path to `graphify.toml` (default: `./graphify.toml`) |
| `--force` | extract, analyze, report, run, check | Bypass extraction cache, full rebuild |
| `--json` | query, explain, check | Output as JSON |
| `--project <name>` | query, explain, path, diff, check | Filter to a specific project |
| `--all` | path | Show all paths (not just shortest) |
| `--max-depth <n>` | path | Limit path search depth |
| `--threshold <f>` | diff | Minimum score delta to report (default: 0.05) |

## Output Formats

Each project produces a subdirectory under the configured output path:

| File | Format | Description |
|------|--------|-------------|
| `graph.json` | JSON | Dependency graph (NetworkX `node_link_data` format) |
| `analysis.json` | JSON | Metrics, communities, cycles, confidence summary |
| `graph_nodes.csv` | CSV | Node metrics (betweenness, PageRank, score, community) |
| `graph_edges.csv` | CSV | Edge list with weights and confidence |
| `architecture_report.md` | Markdown | Human-readable report with hotspots, cycles, communities |
| `architecture_graph.html` | HTML | Interactive D3.js force-directed graph visualization |
| `graph.cypher` | Cypher | Neo4j import script (`CREATE` nodes + relationships) |
| `graph.graphml` | GraphML | XML export (compatible with yEd, Gephi) |
| `obsidian_vault/` | Markdown | Obsidian vault with one `.md` per node and `[[wikilinks]]` |
| `drift-report.json` | JSON | Drift detection results (via `graphify diff`) |
| `drift-report.md` | Markdown | Drift detection report |

When 2+ projects are configured, a `graphify-summary.json` with aggregate stats is also generated.

## Incremental Builds

Graphify caches extraction results per file using SHA256 content hashing. On subsequent runs, only changed files are re-parsed.

```bash
graphify run              # first run: full extraction
graphify run              # second run: cache hit, skips unchanged files
graphify run --force      # bypass cache, full rebuild
```

The cache is stored as `.graphify-cache.json` in each project's output directory. It auto-invalidates on version upgrades or `local_prefix` changes.

## Drift Detection

Compare analysis snapshots to detect architectural drift:

```bash
# File vs file
graphify diff --before report/v1/analysis.json --after report/v2/analysis.json

# Baseline vs live project
graphify diff --baseline report/baseline/analysis.json --config graphify.toml --project my-app
```

Detects changes across 5 dimensions: node additions/removals, hotspot score shifts, cycle introduction/resolution, community membership moves, and degree changes.

## Quality Gates (CI)

Use `graphify check` in CI pipelines to enforce architectural constraints:

```bash
graphify check --config graphify.toml --max-cycles 0 --max-hotspot-score 0.5
graphify check --config graphify.toml --max-cycles 0 --json  # machine-readable output
```

Exit code 0 = all checks pass, non-zero = violations found.

## MCP Server

Graphify includes an MCP server (`graphify-mcp`) that exposes graph queries to AI assistants like Claude:

```bash
cargo install --path crates/graphify-mcp
```

Add to your Claude Code MCP config:

```json
{
  "mcpServers": {
    "graphify": {
      "command": "graphify-mcp",
      "args": ["--config", "graphify.toml"]
    }
  }
}
```

Exposes 9 tools: `graphify_stats`, `graphify_search`, `graphify_explain`, `graphify_dependents`, `graphify_dependencies`, `graphify_shortest_path`, `graphify_all_paths`, `graphify_suggest`, `graphify_hotspots`.

## Confidence Scoring

Every edge carries a confidence score (0.0–1.0) indicating extraction certainty:

| Source | Confidence | Kind |
|--------|-----------|------|
| Direct import | 1.0 | Extracted |
| Python relative import | 0.9 | Extracted |
| TS relative import | 0.9 | Extracted |
| TS path alias | 0.85 | Extracted |
| Bare function call | 0.7 | Inferred |
| Non-local target | ≤0.5 | Ambiguous |

Use `--json` with `query` or `explain` to see confidence data. The MCP server supports `min_confidence` filtering.

## Common Monorepo Recipes

### 1. Refresh the full graph before research

```bash
graphify run --config graphify.toml
```

Use this after config changes, before architecture review, or after a large refactor.

### 2. Find a namespace, route group, or bounded context

```bash
graphify query 'src.app.*study-chat*' --config graphify.toml --project web
graphify query 'app.api.*' --config graphify.toml --project api --json
```

Start with `query` when you know roughly what area you want but not the exact node ID.

### 3. Investigate a hotspot before refactoring it

```bash
graphify explain 'src.shared.domain.errors' --config graphify.toml --project pkg-api
graphify explain 'src.shared.domain.errors' --config graphify.toml --project pkg-api --json
```

Use `explain` before touching a high fan-in module to assess blast radius.

### 4. Trace why one module depends on another

```bash
graphify path 'src.hooks' 'src.trpc.react' --config graphify.toml --project web
graphify path 'src.hooks' 'src.trpc.react' --config graphify.toml --project web --all --max-depth 6
```

Use the default shortest path first. Rerun with `--all` to inspect alternate routes.

### 5. Compare drift before and after a refactor

```bash
cp report/web/analysis.json /tmp/web-before.json
graphify run --config graphify.toml
graphify diff --baseline /tmp/web-before.json --config graphify.toml --project web
```

### 6. Watch mode during development

```bash
graphify watch --config graphify.toml
```

Monitors source files and auto-rebuilds only affected projects on changes (300ms debounce). Useful during active refactoring.

### 7. CI quality gate

```yaml
# .github/workflows/arch.yml
- run: graphify check --config graphify.toml --max-cycles 0 --max-hotspot-score 0.8
```

Fails the build if architectural constraints are violated.

## Supported Languages

- Python
- TypeScript / JavaScript

## Architecture

Cargo workspace with 5 crates:

| Crate | Role |
|---|---|
| `graphify-core` | Graph model, metrics, community detection, cycles, query engine, diff |
| `graphify-extract` | tree-sitter AST parsing, file discovery, module resolution, caching |
| `graphify-report` | JSON, CSV, Markdown, HTML, Neo4j, GraphML, Obsidian output |
| `graphify-cli` | CLI, config parsing, pipeline orchestration, watch mode |
| `graphify-mcp` | MCP server for AI assistant integration |

---

## AI Assistant Instructions (copy to your CLAUDE.md)

Copy the block below into your project's `CLAUDE.md` to make Claude Code use Graphify as the primary source for codebase research.

````markdown
## Architectural Research with Graphify

This project uses [Graphify](https://github.com/parisgroup-ai/graphify) for architectural analysis. **Use Graphify as the primary source for understanding the codebase structure before reading individual files.**

### Setup (run once)

If `graphify.toml` does not exist yet:

```bash
graphify init
# Then edit graphify.toml with the correct project settings
graphify run --config graphify.toml
```

### Workflow: graph-first, code-second

When you need to understand the architecture or the impact of a change, **always query the graph before reading source files**:

1. **Find modules** — `graphify query "app.services.*" --config graphify.toml`
2. **Understand a module** — `graphify explain app.services.llm --config graphify.toml --json`
3. **Trace dependencies** — `graphify path app.main app.services.llm --config graphify.toml`
4. **Check drift after changes** — `graphify diff --baseline report/analysis.json --config graphify.toml`

### When to use each command

| Situation | Command |
|-----------|---------|
| "What modules exist in this area?" | `graphify query "pattern.*" --config graphify.toml` |
| "What depends on this module? What will break if I change it?" | `graphify explain <module> --config graphify.toml --json` |
| "How does module A reach module B?" | `graphify path <A> <B> --config graphify.toml` |
| "What are the architectural hotspots / God modules?" | Read `report/<project>/architecture_report.md` or `analysis.json` |
| "Are there circular dependencies?" | Read `analysis.json` → `cycles` array |
| "Did my changes introduce drift?" | `graphify diff --baseline report/analysis.json --config graphify.toml` |
| "Does this pass CI quality gates?" | `graphify check --config graphify.toml --max-cycles 0` |

### Rules

- Before modifying a module with high fan-in (in_degree > 20), run `graphify explain <module>` to understand the blast radius.
- After significant refactoring, run `graphify run --config graphify.toml` to regenerate the analysis and check for new cycles or hotspot changes.
- Use `--json` flag when you need structured data for programmatic reasoning.
- Module IDs use dot notation matching the file path: `app/services/llm/base.py` → `app.services.llm.base`.
- The `analysis.json` file contains pre-computed metrics (betweenness, PageRank, community clusters, cycles) — read it directly for aggregate questions instead of re-running the pipeline.
````

## License

MIT
