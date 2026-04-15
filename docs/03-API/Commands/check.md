---
title: "graphify check"
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/cli
  - command
  - ci
related:
  - "[[CLI Reference]]"
  - "[[pr-summary]]"
  - "[[ADR-008 CI Quality Gates]]"
  - "[[ADR-011 Contract Drift Detection]]"
---

# `graphify check`

Evaluate architectural quality gates for CI. Re-runs extract + analyze in memory; **exits 1 on any violation in any project**. Always writes `<project_out>/check-report.json` (consumed by [[pr-summary]]).

## Synopsis

```bash
graphify check [--config <path>] [--output <dir>] \
               [--max-cycles <N>] [--max-hotspot-score <F>] \
               [--project <name>] [--json] [--force] \
               [--contracts | --no-contracts] [--contracts-warnings-as-errors]
```

## Arguments

None.

## Flags

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | `graphify.toml` | Path to config file |
| `--output <dir>` | `[settings].output` | Override output directory |
| `--max-cycles <N>` | unlimited | Maximum allowed cycle count per project |
| `--max-hotspot-score <F>` | unlimited | Maximum allowed hotspot score per project |
| `--project <name>` | all | Filter to a single project |
| `--json` | `false` | Stable JSON output to stdout |
| `--force` | `false` | Bypass SHA256 extraction cache |
| `--contracts` | implicit if `[[contract.pair]]` declared | Run the contract drift gate |
| `--no-contracts` | `false` | Skip the contract drift gate even when pairs are configured |
| `--contracts-warnings-as-errors` | `false` | Escalate `UnmappedOrmType` warnings to errors |

> [!warning] Mutually exclusive
> `--contracts` and `--no-contracts` cannot be combined (clap-enforced).

## Behavior

1. Load config, optionally filter to one project
2. For each project: run extract + analyze in memory (cache respected unless `--force`)
3. Evaluate gates per project:
   - `max_cycles` — fails if cycle count > limit
   - `max_hotspot_score` — fails if max hotspot score > limit
   - `policy` rules — see [[ADR-008]] (FEAT-013)
4. If contracts configured (or `--contracts`): run the contract drift comparison ([[ADR-011]])
5. **Always** write `<project_out>/check-report.json` (unified output)
6. Print human or JSON output to stdout
7. Exit `1` if any violation; `0` otherwise

## Examples

```bash
# Run with all gates (no limits → just summary, exits 0)
graphify check

# Strict CI gate
graphify check --max-cycles 0 --max-hotspot-score 0.7

# JSON output for CI parsers
graphify check --json > check.json

# Skip contract drift even when pairs are declared
graphify check --no-contracts

# Treat unmapped ORM types as errors
graphify check --contracts-warnings-as-errors
```

## Output

Stdout (human, default):

```
[ana-service] PASS  nodes=142 edges=287 cycles=0 hotspot_max=0.42
[api]         FAIL  cycle_count=2 (limit 0)
                    hotspot_max=0.81 (limit 0.70)

2 violations across 1 project
```

Stdout (`--json`): structured `CheckReport` JSON — see source schema in `crates/graphify-report/src/check_report.rs`.

Files always written:

| File | Description |
|---|---|
| `<output>/<project>/check-report.json` | Unified per-project + workspace-level contracts |

> [!info] Always-on disk artifact
> `check-report.json` is written **regardless** of `--json` flag (FEAT-015 ecosystem change). Tooling expecting clean output dirs needs to know.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | All gates passed (or no limits set) |
| 1 | At least one gate violation, OR config / IO error |

## Flag interactions

- **No limits + no contracts**: prints summary, exits 0. Safe to call in CI without configuration.
- **`--contracts` without any `[[contract.pair]]`**: emits a warning and skips silently.
- **`--contracts-warnings-as-errors`** affects only the contract gate (project gates have no warning class).
- **`--project` with multi-project config**: only the named project is evaluated.

## Gotchas

- Exit-1 convention is **uniform across the Graphify CLI** — not POSIX exit-2 for usage errors. Document if your CI special-cases exit codes.
- The aggregate exit code loses per-project granularity. Use `--json` if you need to fail individual jobs.
- Contract violations span pairs that often **cross `[[project]]` boundaries** — pair file paths resolve workspace-root relative.

## See also

- [[pr-summary]] — consumes `check-report.json` to render PR Markdown
- [[ADR-008 CI Quality Gates]] — design rationale
- [[ADR-011 Contract Drift Detection]] — contracts gate detail
- [[Troubleshooting#CLI exit codes]]
