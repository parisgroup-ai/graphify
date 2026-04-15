---
title: "graphify trend"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - drift
related:
  - "[[CLI Reference]]"
  - "[[diff]]"
---

# `graphify trend`

Aggregate historical `analysis.json` snapshots into a trend report. Where [[diff]] is point-in-time-vs-point-in-time, `trend` is **time-series** — useful for tracking architectural health weekly or monthly.

## Synopsis

```bash
graphify trend [--config <path>] [--project <name>] [--output <dir>] [--limit <N>] [--json]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--project <name>` | required for multi-project configs | Project to aggregate |
| `--output <dir>` | `[settings].output` | Override output directory |
| `--limit <N>` | unlimited | Aggregate only the most recent N snapshots |
| `--json` | `false` | Print the trend report as JSON to stdout |

## Behavior

Reads snapshots from the configured history store (typically a `history/` directory under output, populated over time by repeated `run` calls), aggregates per-metric series, and writes:

- `trend-report.json` — structured time series
- `trend-report.md` — human-readable summary tables

## Examples

```bash
# Aggregate full history for the only project in config
graphify trend

# Multi-project: must specify
graphify trend --project api

# Last 30 snapshots only
graphify trend --project api --limit 30

# JSON to stdout for CI consumption
graphify trend --project api --json > trend.json
```

## Output

| File | Description |
|---|---|
| `trend-report.json` | Structured snapshot series + per-metric trend |
| `trend-report.md` | Markdown summary with tables and (where relevant) sparkline indicators |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Trend report written |
| 1 | Config error, missing project name (multi-project), or no snapshots found |

## Gotchas

- **You need a history of snapshots to aggregate.** First-time use produces a one-row trend.
- **Multi-project configs require `--project`.** Trends are per-project, not cross-project (no aggregate "monorepo trend").
- **Snapshot retention** is not managed by `graphify trend` — old snapshots stay where they were written until you prune them.
- **Schema evolves.** Older snapshots missing newer fields (e.g., contract violations) are tolerated but those fields just appear `null` in the trend.

## See also

- [[diff]] — point-in-time comparison
- [[Crate - graphify-core]] — `history.rs` module
- [[ADR-007 Architectural Drift Detection]] — sister feature
