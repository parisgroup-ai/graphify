---
title: Configuration
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/guide
  - type/reference
  - getting-started
related:
  - "[[Installation]]"
  - "[[First Steps]]"
  - "[[🔍 Quick Reference]]"
---

# Configuration

Graphify is configured via a single TOML file (`graphify.toml` by default). Generate a starter with:

```bash
graphify init
```

## File anatomy

```toml
[settings]
output  = "./report"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format  = ["json", "csv", "md", "html"]

[[project]]
name         = "ana-service"
repo         = "./apps/ana-service"
lang         = ["python"]
local_prefix = "app"
```

## `[settings]` — global

| Key | Type | Default | Notes |
|---|---|---|---|
| `output` | path | `"./report"` | Per-project subdirectories created under this root |
| `weights` | `[f64; 4]` | `[0.4, 0.2, 0.2, 0.2]` | Unified hotspot score: `betweenness, pagerank, in_degree, in_cycle` (must sum to 1.0) |
| `exclude` | `[String]` | see below | Directory names skipped during file discovery |
| `format` | `[String]` | `["json", "csv", "md", "html"]` | Output formats |

### Default exclusions

`__pycache__`, `node_modules`, `.git`, `dist`, `tests`, `__tests__`, `.next`, `build`, `.venv`, `venv`

> [!info] Test files
> Built-in glob excludes for test files: `*.test.{ts,tsx,js,jsx}`, `*.spec.{ts,tsx,js,jsx}`, `*.test.py`, `*_test.py`. You don't need to add these to `exclude`.

### Supported `format` values

| Value | Output |
|---|---|
| `json` | `graph.json`, `analysis.json` |
| `csv` | `graph_nodes.csv`, `graph_edges.csv` |
| `md` | `architecture_report.md` |
| `html` | `architecture_graph.html` (D3.js, self-contained) |
| `neo4j` | `graph.cypher` |
| `graphml` | `graph.graphml` |
| `obsidian` | `obsidian_vault/` (one `.md` per node) |

## `[[project]]` — one block per project

| Key | Type | Required | Notes |
|---|---|---|---|
| `name` | string | yes | Used as the output subdirectory name |
| `repo` | path | yes | Path to project root (relative to config file) |
| `lang` | `[String]` | yes | Subset of `["python", "typescript", "go", "rust"]` |
| `local_prefix` | string | **no** | Module prefix used to mark "local" nodes. Auto-detected if omitted (FEAT-011) |

### `local_prefix` heuristics

When omitted, Graphify auto-detects:

1. Picks `src/` or `app/` if dominant in the project.
2. Falls back to root-level files if neither dominates.
3. Logs the detected prefix to stderr.

Set explicitly when:
- The repo has multiple top-level source roots
- You see warnings about "≤1 file discovered" (the heuristic missed)

> [!warning] Misconfigured prefix
> The walker emits `eprintln!` warnings when a project discovers ≤1 file — usually means the prefix doesn't match your layout.

## Path aliases (TypeScript)

Graphify reads `tsconfig.json` for `paths` aliases.

| Pattern | Behavior |
|---|---|
| `@/*` → `./src/*` | Resolved as local; node ID uses normalized module path |
| `@repo/*` → `../../packages/*` | Resolved to workspace package; preserves original import string when target traverses outside the project |
| External (`@parisgroup-ai/*` not in `paths`) | Treated as external — no Defines edge |

> [!warning] Alias precedence
> `@/*` matches **only** when the target is inside the project. External scoped packages like `@repo/logger` are not captured by it.

## Recipes

### Single Python service

```toml
[settings]
output = "./report"

[[project]]
name = "api"
repo = "."
lang = ["python"]
local_prefix = "app"
```

### Multi-language monorepo

```toml
[settings]
output = "./report"
format = ["json", "md", "html", "obsidian"]

[[project]]
name = "ana-service"
repo = "./apps/ana-service"
lang = ["python"]
local_prefix = "app"

[[project]]
name = "web"
repo = "./apps/web"
lang = ["typescript"]
local_prefix = "src"

[[project]]
name = "shared"
repo = "./packages/shared"
lang = ["typescript"]
local_prefix = "src"
```

> [!tip] Cross-project summary
> When 2+ projects are configured, `graphify run` also writes `<output>/graphify-summary.json` with aggregate stats (no full edge list).

### CI mode (`check` only)

```toml
[settings]
output = "./report"
format = ["json"]

[[project]]
name = "api"
repo = "."
lang = ["python"]
local_prefix = "app"

[check]
max_cycles    = 0
max_hotspots  = 5
hotspot_threshold = 0.05
```

See [[🔍 Quick Reference#CI gates]].

### Policy rules (FEAT-013)

```toml
[[policy.group]]
name     = "domain"
match    = "app.domain.*"

[[policy.group]]
name     = "infra"
match    = "app.infrastructure.*"

[[policy.rule]]
kind  = "deny"
from  = "domain"
to    = "infra"
```

`graphify check` emits a violation for any edge crossing the denied direction.

## Validation

`graphify init` writes a known-good starter. To validate a hand-edited file:

```bash
graphify run --config graphify.toml
# stderr will surface any TOML parse error or missing required field
```

## Next

- [[First Steps]] — run your first analysis
- [[🔍 Quick Reference]] — full CLI cheat sheet
- [[Troubleshooting]]
