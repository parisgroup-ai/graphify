---
title: Quick Reference
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/reference
  - type/cli
related:
  - "[[🏠 Home]]"
  - "[[Configuration]]"
  - "[[First Steps]]"
---

# 🔍 Quick Reference

One-page CLI cheat sheet. For deeper context see [[Configuration]] and [[First Steps]].

## Pipeline

```bash
graphify init                              # write a starter graphify.toml
graphify run     --config graphify.toml    # extract → analyze → report (full)
graphify extract --config graphify.toml    # AST → graph.json per project
graphify analyze --config graphify.toml    # metrics → analysis.json + CSV
graphify report  --config graphify.toml    # all outputs (md, html, neo4j, obsidian, ...)
```

> [!tip] First run
> `graphify init && graphify run` works in any directory once a `local_prefix` is auto-detectable.

## Watch mode

```bash
graphify watch --config graphify.toml      # 300ms debounce, per-project rebuild
```

> [!info] Cache behavior
> `--force` only applies to the **initial** build. Subsequent rebuilds always use the SHA256 cache.

## Drift detection

Compare two snapshots of `analysis.json`:

```bash
# Two saved snapshots
graphify diff --before report/v1/analysis.json --after report/v2/analysis.json

# Live vs stored baseline
graphify diff --baseline report/baseline/analysis.json --config graphify.toml
```

Outputs:
- `drift-report.json` — machine-readable
- `drift-report.md` — human-readable

## CI gates

```bash
graphify check --config graphify.toml      # exit 1 on violations (cycles, hotspots, policy, contracts)
```

Outputs `<project_out>/check-report.json` unconditionally — consumed by `pr-summary`.

> [!warning] Exit code
> `graphify check` uses **exit 1** for violations (not exit 2). All graphify CLI errors are exit 1 — uniform convention.

## PR summary

```bash
graphify pr-summary <PROJECT_OUT_DIR>      # writes Markdown to stdout
```

Pure renderer over `analysis.json` (required) + `drift-report.json` + `check-report.json` (optional). **Does not gate** — that's `check`'s job.

## Trend tracking

```bash
graphify trend --config graphify.toml      # aggregate historical snapshots → trend-report.{json,md}
```

## Graph queries

```bash
graphify query "app.services.*" --config graphify.toml          # glob-style search
graphify path  app.main app.services.llm --config graphify.toml # find dependency paths
graphify explain app.services.llm --config graphify.toml        # impact + profile of one node
graphify shell --config graphify.toml                           # interactive REPL
```

> [!info] No cache
> Query commands always do a fresh extraction — cache is bypassed.

## MCP server

```bash
graphify mcp --config graphify.toml        # JSON-RPC over stdio for AI assistants
```

Exposes 9 tools backed by `QueryEngine`. Eager extraction on startup; stderr for diagnostics, stdout reserved for protocol.

## AI integrations (Claude Code / Codex)

```bash
graphify install-integrations                   # auto-detect ~/.claude + ~/.agents
graphify install-integrations --project-local   # install into ./.claude (team-shareable)
graphify install-integrations --force           # overwrite edited files
graphify install-integrations --uninstall       # reverse manifest-tracked changes
```

Installs 5 slash commands (`/gf-setup`, `/gf-analyze`, `/gf-onboard`, `/gf-refactor-plan`, `/gf-drift-check`), 3 skills, 2 agents, and registers the MCP server. After install, run `/gf-setup` from inside the client for diagnostics + upgrade. Full guide: [[AI Integrations]].

> [!warning] MCP reload
> Slash commands hot-reload; the **MCP server is only loaded on client boot** — restart Claude Code / Codex after install.

## Build from source

```bash
cargo build --release -p graphify-cli      # binary at target/release/graphify
cargo test  --workspace                    # full test suite
cargo test  -p graphify-extract            # single crate
```

## Config snippet

```toml
[settings]
output  = "./report"
weights = [0.4, 0.2, 0.2, 0.2]   # betweenness, pagerank, in_degree, in_cycle
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format  = ["json", "csv", "md", "html"]   # also: neo4j, graphml, obsidian

[[project]]
name         = "ana-service"
repo         = "./apps/ana-service"
lang         = ["python"]
local_prefix = "app"          # optional — auto-detected when omitted (FEAT-011)
```

Full reference: [[Configuration]].

## Output file map

| File | Producer |
|---|---|
| `graph.json` | `extract` (NetworkX `node_link_data` format) |
| `graph_nodes.csv` / `graph_edges.csv` | `analyze` |
| `analysis.json` | `analyze` |
| `architecture_report.md` | `report` |
| `architecture_graph.html` | `report` (D3.js, self-contained) |
| `graph.cypher` | `report` (Neo4j import script) |
| `graph.graphml` | `report` (yEd / Gephi compatible) |
| `obsidian_vault/` | `report` (one `.md` per node, wikilinks) |
| `check-report.json` | `check` |
| `drift-report.{json,md}` | `diff` |
| `trend-report.{json,md}` | `trend` |
| `graphify-summary.json` | `run` (only when 2+ projects) |

## Related

- [[🏠 Home]]
- [[Configuration]]
- [[First Steps]]
- [[Troubleshooting]]
