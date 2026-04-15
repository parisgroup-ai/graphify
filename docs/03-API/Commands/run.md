---
title: "graphify run"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - pipeline
related:
  - "[[CLI Reference]]"
  - "[[report]]"
---

# `graphify run`

Alias for [[report]] kept for backward compatibility with the original Python CLI. Runs the full pipeline (extract → analyze → report) and emits all configured formats.

## Synopsis

```bash
graphify run [--config <path>] [--output <dir>] [--force]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--output <dir>` | `[settings].output` | Override output directory |
| `--force` | `false` | Bypass SHA256 extraction cache |

> [!info] Fewer flags than `report`
> `run` does **not** accept `--weights` or `--format` overrides. Use [[report]] when you need those. The intent of `run` is "do whatever the config says".

## Examples

```bash
# Most common invocation — config-driven everything
graphify run

# Custom config
graphify run --config configs/ci.toml

# Skip the cache (full rebuild)
graphify run --force
```

## Output

Same as [[report]] — see that page for the full file map.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Pipeline completed for all projects |
| 1 | Config error, write failure, or fatal IO error |

## When to use which

| Goal | Use |
|---|---|
| Default day-to-day pipeline run | [[run]] |
| Override formats or weights for one invocation | [[report]] |
| Just the graph (no metrics) | [[extract]] |
| Just metrics (no Markdown/HTML) | [[analyze]] |
| Auto-rebuild on file change | [[watch]] |

## See also

- [[report]] — the actual implementation; `run` dispatches to it
- [[Configuration]]
