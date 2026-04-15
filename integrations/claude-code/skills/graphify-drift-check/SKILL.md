---
name: graphify-drift-check
description: "Run architectural drift gate. Compares the current Graphify analysis against a baseline and fails on regression (new cycles, hotspot growth, threshold breach). Use in CI, pre-merge hooks, and when the user asks to 'check drift', 'gate this PR', or 'verify no regression'."
version: 1.0.0
min_graphify_version: "0.6.0"
---

# Graphify Drift Check

## Purpose

Architectural drift gate. Compares current repo state against a baseline analysis, renders a PR-comment-ready Markdown, and emits a deterministic exit code. Safe to run in CI.

## Modes

- **CI mode** — env `CI` is non-empty → non-interactive; output Markdown to stdout, exit code propagates
- **Local mode** — interactive; same pipeline + a one-liner suggesting `/gf-refactor-plan` when violations exist

## Prerequisites

```bash
command -v graphify >/dev/null || { echo "graphify not installed" >&2; exit 1; }
[ -f graphify.toml ] || { echo "graphify.toml missing" >&2; exit 1; }
```

## Baseline Resolution

In priority order:
1. Skill argument `--baseline <path>`
2. `report/baseline/analysis.json` if present
3. Abort with this message:

```
No baseline found. Produce one with:
  graphify run && cp report/<project>/analysis.json report/baseline/
```

## Flow

1. Detect mode: `[ -n "$CI" ]` → CI mode
2. Resolve baseline (see above)
3. Run fresh extraction (force bypasses cache to guarantee accurate gate):
   ```bash
   graphify run --config graphify.toml --force
   ```
4. Run check — writes `report/<project>/check-report.json`:
   ```bash
   graphify check --config graphify.toml --json
   ```
5. Run drift:
   ```bash
   graphify diff --before "$BASELINE" --after report/<project>/analysis.json
   ```
6. Delegate to the `graphify-ci-guardian` agent via Task tool:

   > Render a drift report. Inputs:
   > - `CHECK_REPORT` = `report/<project>/check-report.json`
   > - `DRIFT_REPORT` = `report/<project>/drift-report.json`
   >
   > Follow the determinism rules in your system prompt. Write Markdown to stdout; propagate exit code.

7. Propagate A2's exit code as the skill's final exit status
8. In local mode, if exit != 0, append: `Run /gf-refactor-plan for a remediation plan.`

## Exit Codes

- `0` — no new violations
- `1` — any new cycle, hotspot regression past threshold, or contract violation
