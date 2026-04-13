# Graphify

Architectural analysis of codebases. Extracts dependencies via tree-sitter, builds knowledge graphs, identifies hotspots, circular dependencies, and community clusters.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/parisgroup/graphify/main/install.sh | sh
```

Or download from [Releases](https://github.com/parisgroup/graphify/releases).

## Quick Start

```bash
graphify init
# Edit graphify.toml
graphify report
```

## Configuration

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests"]
format = ["json", "csv", "md"]

[[project]]
name = "my-app"
repo = "./apps/my-app"
lang = ["python"]
local_prefix = "app."
```

## Commands

| Command | Description |
|---------|-------------|
| `graphify init` | Generate graphify.toml |
| `graphify extract` | Extract dependency graph |
| `graphify analyze` | Extract + compute metrics |
| `graphify report` | Full pipeline with all outputs |
| `graphify run` | Alias for report |
| `graphify query "pattern"` | Search nodes by glob pattern |
| `graphify explain <node>` | Module profile card + impact analysis |
| `graphify path <source> <target>` | Find dependency paths between modules |
| `graphify diff` | Detect architectural drift between snapshots |
| `graphify watch` | Auto-rebuild on file changes |
| `graphify shell` | Interactive graph exploration REPL |

## Common Monorepo Recipes

These examples show how the subcommands fit together in day-to-day architecture work on a multi-project repo.

### 1. Refresh the full graph before research

```bash
graphify run --config graphify.toml
```

Use this after config changes, before architecture review, or after a large refactor. It regenerates `graph.json`, `analysis.json`, CSV exports, and the Markdown report for every configured project.

### 2. Find a namespace, route group, or bounded context quickly

```bash
graphify query 'src.app.*study-chat*' --config graphify.toml --project web
graphify query 'app.api.*' --config graphify.toml --project api --json
```

Start with `query` when you know roughly what area you want but not the exact node ID. Add `--json` when another tool or script needs the results.

### 3. Investigate a hotspot before refactoring it

```bash
graphify explain 'src.shared.domain.errors' --config graphify.toml --project pkg-api
graphify explain 'src.shared.domain.errors' --config graphify.toml --project pkg-api --json
```

Use `explain` before touching a high fan-in module. It gives you a focused profile for a single node so you can assess blast radius before making changes.

### 4. Trace why one module depends on another

```bash
graphify path 'src.hooks' 'src.trpc.react' --config graphify.toml --project web
graphify path 'src.hooks' 'src.trpc.react' --config graphify.toml --project web --all --max-depth 6
```

Use the default shortest path first. If the dependency looks surprising, rerun with `--all` to inspect alternate routes up to a controlled depth.

### 5. Compare drift before and after a refactor

```bash
cp report/web/analysis.json /tmp/web-before.json
graphify run --config graphify.toml
graphify diff --baseline /tmp/web-before.json --config graphify.toml --project web
```

This is the fastest way to answer "what changed architecturally?" after a branch of work. Keep the baseline snapshot before the refactor, rerun Graphify, then diff against the live project.

### 6. Review a project as an architecture workflow, not isolated commands

```bash
graphify run --config graphify.toml
graphify query 'src.app.*study-chat*' --config graphify.toml --project web
graphify explain 'src.shared.domain.errors' --config graphify.toml --project pkg-api
graphify path 'src.hooks' 'src.trpc.react' --config graphify.toml --project web
```

This sequence works well for real monorepos:
- refresh the graph
- narrow the search space with `query`
- inspect risky modules with `explain`
- confirm dependency chains with `path`

## Output

Each project produces:
- `graph.json` — dependency graph (NetworkX node_link_data format)
- `analysis.json` — metrics, communities, cycles
- `graph_nodes.csv` — node metrics
- `graph_edges.csv` — edge list
- `architecture_report.md` — human-readable report

## Supported Languages

- Python
- TypeScript

---

## AI Assistant Instructions (copy to your CLAUDE.md)

Copy the block below into your project's `CLAUDE.md` to make Claude Code use Graphify as the primary source for codebase research.

````markdown
## Architectural Research with Graphify

This project uses [Graphify](https://github.com/parisgroup/graphify) for architectural analysis. **Use Graphify as the primary source for understanding the codebase structure before reading individual files.**

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

### Rules

- Before modifying a module with high fan-in (in_degree > 20), run `graphify explain <module>` to understand the blast radius.
- After significant refactoring, run `graphify run --config graphify.toml` to regenerate the analysis and check for new cycles or hotspot changes.
- Use `--json` flag when you need structured data for programmatic reasoning.
- Module IDs use dot notation matching the file path: `app/services/llm/base.py` → `app.services.llm.base`.
- The `analysis.json` file contains pre-computed metrics (betweenness, PageRank, community clusters, cycles) — read it directly for aggregate questions instead of re-running the pipeline.
````

## License

MIT
