---
title: "graphify query"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - query
related:
  - "[[CLI Reference]]"
  - "[[explain]]"
  - "[[path]]"
  - "[[shell]]"
---

# `graphify query`

Glob-search nodes by ID. Returns matching nodes with their key metrics. Designed for "where do all my service modules live?" or "what's the most central thing under `app.routers.*`?"

## Synopsis

```bash
graphify query <pattern> [--config <path>] [--kind <K>] [--sort <S>] [--project <name>] [--json]
```

## Arguments

| Arg | Required | Description |
|---|---|---|
| `<pattern>` | yes | Glob pattern matched against node IDs. Supports `*` (any chars) and `?` (single char). |

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--kind <K>` | all | Filter by node kind: `module`, `function`, `class`, `method` |
| `--sort <S>` | `score` | Sort by: `score` (default), `name`, `in_degree` |
| `--project <name>` | all | Filter to a single project |
| `--json` | `false` | Machine-readable JSON output |

## Examples

```bash
# All nodes under app.services
graphify query "app.services.*"

# Just classes named like "Repo"
graphify query "*Repo" --kind class

# Sort alphabetically instead of by score
graphify query "app.*" --sort name

# JSON for scripting
graphify query "app.*" --json | jq '.[] | select(.score > 0.5) | .node_id'

# Multi-project: filter to one
graphify query "src.api.*" --project web
```

## Output

Human (default):

```
Matches (4 nodes):

  app.services.llm         Module   score=0.847  community=2  ●cycle
  app.services.auth        Module   score=0.623  community=1
  app.services.billing     Module   score=0.412  community=1
  app.services.cache       Module   score=0.201  community=3
```

JSON (`--json`): array of `QueryMatch` objects — `{ node_id, kind, file_path, score, community_id, in_cycle }`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Matched nodes returned (or no matches — not an error) |
| 1 | Config error or fatal IO error |

## Gotchas

- **Always re-extracts** from source — bypasses the SHA256 cache by design. Fresh data matters when you ask a question.
- **Glob, not regex.** Use `**` only as a literal — Graphify treats `*` as "any chars" globally (no nested-pattern semantics).
- **No matches → exit 0**, not exit 1. Empty result is valid.
- **Multi-project**: searches across all projects unless `--project` is set; results aren't qualified by project name in the human output (use `--json` to know which project a hit belongs to).

## See also

- [[explain]] — drill into one node from your query result
- [[path]] — find dependency paths between hits
- [[shell]] — interactive REPL with the same search
- [[ADR-004 Graph Query Interface]]
