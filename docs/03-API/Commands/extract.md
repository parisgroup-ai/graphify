---
title: "graphify extract"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - pipeline
related:
  - "[[CLI Reference]]"
  - "[[analyze]]"
  - "[[run]]"
---

# `graphify extract`

Discover source files, parse them via tree-sitter, and write the dependency graph as `graph.json` per project. **No metrics, no reports** — extraction only.

## Synopsis

```bash
graphify extract [--config <path>] [--output <dir>] [--force]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--output <dir>` | `[settings].output` | Override output directory |
| `--force` | `false` | Bypass SHA256 extraction cache; full rebuild |

## Examples

```bash
# Default config in current directory
graphify extract

# Custom config and output
graphify extract --config configs/dev.toml --output ./out

# Force a clean rebuild (skip cache)
graphify extract --force
```

## Output

Per project under `<output>/<project>/`:

| File | Description |
|---|---|
| `graph.json` | Dependency graph in NetworkX `node_link_data` format |
| `.graphify-cache.json` | SHA256 extraction cache (hidden) |

Plus stderr cache stats: `[ana-service] Cache: 47 hits, 3 misses, 2 evicted`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Extraction succeeded for all projects |
| 1 | Config not found, parse error, or fatal IO error |

## Gotchas

- **No analysis is performed.** Use [[analyze]] (or [[run]]) for metrics, communities, cycles.
- **`--force` saves a fresh cache after the rebuild.** Subsequent runs without `--force` will use it.
- Walker excludes are directory-level, not glob-level — see [[Configuration#Default exclusions]].

## See also

- [[analyze]] — next stage in the pipeline
- [[run]] — full pipeline (extract + analyze + report)
- [[ADR-001 Rust Rewrite]] — extraction architecture
- [[ADR-003 SHA256 Extraction Cache]] — cache details
