---
title: "graphify path"
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
  - "[[explain]]"
---

# `graphify path`

Find dependency paths between two nodes. Default is the **shortest** path; `--all` enumerates up to `--max-paths` paths capped at `--max-depth`.

## Synopsis

```bash
graphify path <source> <target> [--config <path>] [--all] [--max-depth <N>] [--max-paths <N>] [--project <name>] [--json]
```

## Arguments

| Arg | Required | Description |
|---|---|---|
| `<source>` | yes | Source node ID |
| `<target>` | yes | Target node ID |

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--all` | `false` | Show all paths (default: shortest only) |
| `--max-depth <N>` | `10` | Maximum path length in edges (only with `--all`) |
| `--max-paths <N>` | `20` | Maximum number of paths to return (only with `--all`) |
| `--project <name>` | first match | Filter to a single project |
| `--json` | `false` | Machine-readable JSON output |

## Examples

```bash
# Shortest path
graphify path app.main app.services.llm

# All paths up to depth 10, max 20
graphify path app.main app.services.llm --all

# Wider search
graphify path app.main app.services.llm --all --max-depth 20 --max-paths 50

# JSON
graphify path app.main app.services.llm --json
```

## Output

Human (default, shortest):

```
app.main ─[Imports]→ app.routes.api ─[Imports]→ app.services.llm

  3 hops, 2 edges
```

Human (`--all`):

```
Path 1 (3 hops):
  app.main ─[Imports]→ app.routes.api ─[Imports]→ app.services.llm

Path 2 (4 hops):
  app.main ─[Imports]→ app.routes.chat ─[Imports]→ app.services.auth ─[Imports]→ app.services.llm

3 paths total (capped at 20)
```

JSON (`--json`): array of paths, each an array of `PathStep` objects — `{ node_id, edge_kind, weight }`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Path(s) found, OR no path exists between the two nodes (not an error) |
| 1 | Source / target not found, config error, or fatal IO error |

## Gotchas

- **Single-project paths only.** Cross-project paths are not supported (separate dependency graphs). With multi-project configs, `path` searches each project's graph and returns the first match unless `--project` is set.
- **`--max-paths` is hard-capped** to prevent runaway enumeration — at high depths, the count of distinct paths explodes combinatorially.
- **No path exists** → human output prints "No path from `<A>` to `<B>`" and exits 0. JSON emits an empty array.
- **Edge kinds in the path** reflect how the source reaches the target (`Imports`, `Defines`, `Calls`).

## See also

- [[query]] — find candidates for source/target
- [[explain]] — full impact view of a node
- [[ADR-004 Graph Query Interface]]
