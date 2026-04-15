---
title: "graphify explain"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - query
related:
  - "[[CLI Reference]]"
  - "[[query]]"
  - "[[path]]"
---

# `graphify explain`

Profile card + impact analysis for a single node: metrics, community membership, cycle participation, direct + transitive dependents, blast radius.

## Synopsis

```bash
graphify explain <node_id> [--config <path>] [--project <name>] [--json]
```

## Arguments

| Arg | Required | Description |
|---|---|---|
| `<node_id>` | yes | Full node ID (e.g. `app.services.llm`) |

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--project <name>` | all | Filter to a single project |
| `--json` | `false` | Machine-readable JSON output |

## Examples

```bash
# Standard human output
graphify explain app.services.llm

# JSON for tooling
graphify explain app.services.llm --json | jq .metrics

# Multi-project: be specific
graphify explain src.api.users --project web
```

## Output

Human (default):

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
  ...

  ── Impact ──
  Transitive dependents: 14 modules
  Blast radius: 58% of local codebase
```

JSON (`--json`): full `ExplainReport` — `{ node_id, kind, file_path, language, metrics, community_id, in_cycle, cycle_peers, direct_dependents, direct_dependencies, transitive_dependent_count, top_transitive_dependents }`.

## Node resolution

If `<node_id>` is not found exactly:

1. Substring fuzzy-match against all node IDs
2. Show up to 3 suggestions
3. Exit 1

```
Error: node "app.service.llm" not found.
Did you mean: app.services.llm?
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Node found and explained |
| 1 | Node not found, config error, or fatal IO error |

## Gotchas

- **Always re-extracts** — bypasses cache by design.
- **External (non-local) nodes work**, but their metrics are minimal — they only have incoming edges from local code.
- **Blast radius** is the percentage of **local** nodes reachable transitively from `<node_id>`'s dependents — not the absolute count.
- **`--project` matters in multi-project setups**: omit and `explain` searches across all projects; the first match wins.

## See also

- [[query]] — find candidate nodes by glob
- [[path]] — traceable paths to/from this node
- [[ADR-004 Graph Query Interface]]
