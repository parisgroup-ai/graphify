---
title: "graphify diff"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - drift
related:
  - "[[CLI Reference]]"
  - "[[pr-summary]]"
  - "[[ADR-007 Architectural Drift Detection]]"
---

# `graphify diff`

Compare two `analysis.json` snapshots and report the architectural delta along 5 dimensions: **summary, edges, cycles, hotspots, communities**. Two modes — file-vs-file or baseline-vs-live.

## Synopsis

```bash
# File-vs-file
graphify diff --before <X.json> --after <Y.json> [--output <dir>] [--threshold <F>]

# Baseline-vs-live
graphify diff --baseline <X.json> --config <toml> [--project <name>] [--output <dir>] [--threshold <F>]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--before <path>` | — | (file mode) Path to "before" `analysis.json` |
| `--after <path>` | — | (file mode) Path to "after" `analysis.json` |
| `--baseline <path>` | — | (live mode) Path to a stored baseline `analysis.json` |
| `--config <path>` | — | (live mode) Path to `graphify.toml` for live extraction |
| `--project <name>` | first project | (live mode) Project to extract |
| `--output <dir>` | current dir | Where to write `drift-report.{json,md}` |
| `--threshold <F>` | `0.05` | Minimum score delta to report as significant |

> [!warning] Mode validation
> Either (`--before` + `--after`) **or** (`--baseline` + `--config`) must be set. Mixing them errors out.

## Examples

```bash
# Compare two saved snapshots
graphify diff --before report-v1/api/analysis.json \
              --after  report-v2/api/analysis.json

# Compare main branch baseline against the working tree
graphify diff --baseline ./baseline/api/analysis.json \
              --config graphify.toml --project api

# Lower the threshold to surface micro-movements
graphify diff --before X.json --after Y.json --threshold 0.0

# Custom output directory
graphify diff --before X.json --after Y.json --output ./drift/
```

## Output

| File | Format |
|---|---|
| `drift-report.json` | Machine-readable `DiffReport` |
| `drift-report.md` | Human-readable Markdown summary |

The Markdown covers (per dimension):

- **Summary delta** — node/edge/community/cycle counts before vs after
- **New nodes / Removed nodes** — node-set diff
- **Cycle changes** — introduced + resolved
- **Hotspot movement** — rising, falling, new in top-20, left top-20
- **Community shifts** — moved nodes + stable count

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Diff computed and written |
| 1 | Mode validation failure, file not found, or fatal IO error |

> [!info] Diff doesn't gate
> `graphify diff` does **not** exit 1 on findings. It reports drift; gating belongs to [[check]].

## Gotchas

- **Communities have unstable IDs across runs.** The diff handles this via max-overlap matching, but small graphs can show "moves" that are really renumbering noise.
- **Cycles are compared as sorted node lists** — same cycle entered at a different rotation point would otherwise compare unequal (mitigated by canonical sorting).
- **Edge-level diff is not exposed.** Degree changes proxy for edge-set changes; rare patterns are lost.
- **Threshold is unidirectional** — applies to absolute delta `|after - before|`. Lower it to see noise; raise it to focus on big movers.

## See also

- [[pr-summary]] — consumes `drift-report.json` for PR Markdown
- [[trend]] — aggregate many snapshots over time
- [[ADR-007 Architectural Drift Detection]] — design rationale
