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

[[policy.group]]
name = "feature"
match = ["src.features.*"]
partition_by = "segment:2"

[[policy.group]]
name = "infra"
match = ["src.infra.*", "app.infra.*"]

[[policy.rule]]
name = "no-cross-feature-imports"
kind = "deny"
from = ["group:feature"]
to = ["group:feature"]
allow_same_partition = true

[[policy.rule]]
name = "infra-is-restricted"
kind = "deny"
from = ["project:*"]
to = ["group:infra"]
except_from = ["group:app", "group:bootstrap"]
```

Multiple `[[project]]` sections enable monorepo analysis. Each project gets its own output subdirectory.

Policy selectors support:
- `group:<name>` for named namespace groups
- `project:<glob>` for configured project names

`partition_by = "segment:N"` lets a group derive peer partitions from dotted node IDs, so rules like feature-to-feature isolation can allow imports within the same feature while blocking cross-feature imports.

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
| `graphify trend` | Aggregate historical trends from stored snapshots |
| `graphify check` | Validate CI quality gates and declarative policy rules |
| `graphify pr-summary` | Render a PR-ready Markdown summary of architectural change |
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
| `check-report.json` | JSON | Unified check result (rules + contract drift); written by `graphify check` |
| `history/*.json` | JSON | Per-run historical snapshots used by `graphify trend` |
| `trend-report.json` | JSON | Aggregated trend report across stored snapshots |
| `trend-report.md` | Markdown | Human-readable architecture trend report |

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

## Historical Trends

`graphify run` and `graphify report` now persist a compact historical snapshot for each project under `report/<project>/history/`.

Aggregate those snapshots into a trend report with:

```bash
graphify trend --config graphify.toml --project my-app
graphify trend --config graphify.toml --project my-app --limit 10 --json
```

V1 trend reports include:
- node, edge, community, and cycle totals over time
- hotspot entrants/exits and score movement between adjacent snapshots
- community churn between adjacent snapshots
- JSON and Markdown outputs written next to the project report directory by default

## Contract Drift (v0.5.0+)

Detects structural drift between ORM schemas and TypeScript contract types across a monorepo — e.g. a Drizzle table in `packages/db` vs the DTO interface the API layer exports in `packages/api`. Graphify normalizes both sides into a shared `Contract` model, aligns fields (with snake_case <-> camelCase handling), and flags missing fields, type mismatches, nullability differences, and relation cardinality drift.

Declare one or more `[[contract.pair]]` entries in `graphify.toml`:

```toml
[[project]]
name = "db"
repo = "./packages/db"
lang = ["typescript"]

[[project]]
name = "api"
repo = "./packages/api"
lang = ["typescript"]

[[contract.pair]]
name = "user"
orm  = { source = "drizzle", file = "packages/db/src/schema/user.ts", table = "users" }
ts   = { file   = "packages/api/src/types/user.ts", export = "UserDto" }

[[contract.pair]]
name = "post"
orm  = { source = "drizzle", file = "packages/db/src/schema/post.ts", table = "posts" }
ts   = { file   = "packages/api/src/types/post.ts", export = "PostDto" }
```

Then run the standard gate command:

```bash
graphify check --config graphify.toml
```

The contract drift gate runs automatically when one or more `[[contract.pair]]` entries are declared. Opt-out with `--no-contracts`; promote warnings to hard failures with `--contracts-warnings-as-errors`. The gate is included in both human and `--json` output of `graphify check`.

Supported in v1:
- Drizzle ORM (Postgres, MySQL, SQLite variants) on the ORM side
- TypeScript `interface` and `type` declarations on the TS side

V1 limitations:
- Prisma schemas are not yet supported
- Zod schemas and tRPC router inputs/outputs are not supported
- `target_contract` on the ORM side is accepted in config but not compared (advisory for now)
- Relation nullability comparison is deferred
- Pair-level `line` in JSON output is hardcoded to `1` (editor integration will address this in FEAT-015)

See [`docs/TaskNotes/Tasks/FEAT-016-contract-drift-detection-between-orm-and-typescript.md`](docs/TaskNotes/Tasks/FEAT-016-contract-drift-detection-between-orm-and-typescript.md) for the full task record.

## Quality Gates (CI)

Use `graphify check` in CI pipelines to enforce architectural constraints:

```bash
graphify check --config graphify.toml --max-cycles 0 --max-hotspot-score 0.5
graphify check --config graphify.toml --max-cycles 0 --json  # machine-readable output
graphify check --config graphify.toml --json                 # policy-only checks
```

Exit code 0 = all checks pass, non-zero = violations found.

Example policy recipes:

```toml
[[policy.group]]
name = "feature"
match = ["src.features.*"]
partition_by = "segment:2"

[[policy.rule]]
name = "no-cross-feature-imports"
kind = "deny"
from = ["group:feature"]
to = ["group:feature"]
allow_same_partition = true

[[policy.group]]
name = "config"
match = ["app.config*", "src.config*"]

[[policy.rule]]
name = "config-is-restricted"
kind = "deny"
from = ["project:*"]
to = ["group:config"]
except_from = ["group:app", "group:bootstrap"]
```

`graphify check --json` now returns both limit violations and policy violations. Policy entries include `type = "policy"`, `rule`, `source_node`, `target_node`, `source_project`, and `target_project`.

### Render a PR summary for GitHub Actions

After `graphify run` + `graphify diff` + `graphify check` populate the project output directory, append a concise Markdown summary to the GitHub Actions job summary:

```yaml
- run: graphify run --config graphify.toml
- run: graphify diff --baseline ./baseline/analysis.json --config graphify.toml --project my-app
- run: graphify check --config graphify.toml || true
- run: graphify pr-summary ./report/my-app >> "$GITHUB_STEP_SUMMARY"
```

`graphify pr-summary <DIR>` is a pure renderer: it reads existing JSON artifacts (`analysis.json` required; `drift-report.json` and `check-report.json` optional) and prints Markdown to stdout. Exit code is 0 regardless of findings — gate with `graphify check` separately if you want CI to fail on violations.

Output is optimized for solo-dev + AI-authored PR review: each finding carries an inline `graphify explain` / `graphify path` hint so the next investigation step is one copy-paste away.

## Hotspot Classification (v0.7+)

Every top-20 hotspot in `architecture_report.md`, `analysis.json`, and `pr-summary` is tagged with a classification that dictates which refactor fits best:

| Type | Signal | Recommended fix |
|------|--------|-----------------|
| **hub** | `in_degree > hub_threshold` (default: 50) | Split the module into submodules, or invert the dependency on its largest consumers. |
| **bridge** | `betweenness / max(in_degree, 1) > bridge_ratio` (default: 3000) | Inject the cross-layer dependency instead of calling through. Reduces chokepoints. |
| **mixed** | Both thresholds fire, or neither | Human judgment: inspect the call graph before choosing. |

Tune per-repo via CLI or config:

```bash
graphify run --config graphify.toml --hub-threshold 80 --bridge-ratio 5000
graphify check --config graphify.toml --hub-threshold 20 --bridge-ratio 1500
```

```toml
[hotspots]
hub_threshold = 80
bridge_ratio = 5000
```

Why classify at all? Two nodes can share a composite score of 0.6 for completely different reasons — a 200-module hub needs a different refactor than an 80-line chokepoint that bridges four layers. See FEAT-017 for the motivating evidence.

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

## AI Integrations

Graphify ships ready-to-install integrations for Claude Code and Codex.

### Install

```bash
graphify install-integrations
```

Auto-detects `~/.claude/` and `~/.agents/skills/`. Use `--claude-code` / `--codex` to target explicitly, `--project-local` to install into the current repo's `.claude/`, `--skip-mcp` to opt out of MCP registration, `--dry-run` to preview, `--uninstall` to reverse.

### What gets installed

| Kind | Name | Invocation |
|---|---|---|
| Agent | `graphify-analyst` | Polyvalent analyst (MCP-preferred) |
| Agent | `graphify-ci-guardian` | Deterministic CI gate |
| Skill | `graphify-onboarding` | Architecture tour |
| Skill | `graphify-refactor-plan` | Phased refactor plan |
| Skill | `graphify-drift-check` | CI drift gate |
| Command | `/gf-analyze` | Full-pipeline summary |
| Command | `/gf-onboard` | Invoke onboarding skill |
| Command | `/gf-refactor-plan` | Invoke refactor plan skill |
| Command | `/gf-drift-check` | Invoke drift check skill |

### CI usage (drift gate)

```yaml
- run: graphify install-integrations --claude-code
- run: graphify run --config graphify.toml
- run: graphify check --config graphify.toml --json
- run: graphify diff --before report/baseline/analysis.json --after report/<project>/analysis.json
- run: graphify pr-summary report/<project> >> $GITHUB_STEP_SUMMARY
```

For interactive flows, invoke `/gf-onboard` or `/gf-refactor-plan` in Claude Code / Codex.

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
