---
title: "ADR-012: PR Summary CLI (`graphify pr-summary`)"
created: 2026-04-14
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-015"
tags:
  - type/adr
  - status/accepted
  - cli
  - report
  - ci
supersedes:
superseded_by:
---

# ADR-012: PR Summary CLI

## Status

**Accepted** — 2026-04-14

## Context

`graphify check` says "PASS/FAIL". `graphify diff` produces a verbose drift report. Neither is suitable as a **concise PR comment** that an LLM (self-reviewing pre-push) or a human (skimming an LLM's change before merge) can scan in 10 seconds. We wanted a small Markdown render that:

- Reads from existing JSON artifacts (no re-extraction)
- Includes inline next-step CLI commands beside each finding
- Degrades gracefully when optional inputs are missing
- Slots into `$GITHUB_STEP_SUMMARY` or `gh pr comment --body-file`

The host bit: `CheckReport` types lived as **private** items inside `graphify-cli/src/main.rs`. To make `pr-summary` a clean module in `graphify-report`, those types had to become **public** in `graphify-report::check_report`. And `graphify check` had to start writing `<project_out>/check-report.json` to disk (it was previously stdout-only under `--json`).

## Decision

**Chosen option:** Add `graphify pr-summary <DIR>` as a **pure renderer** in `graphify-report/src/pr_summary.rs`. Reads `analysis.json` (required) plus `drift-report.json` and `check-report.json` (both optional). Writes Markdown to stdout, warnings to stderr.

Three structural changes ride along:

1. Move `CheckReport` / `ProjectCheckResult` / `CheckViolation` / related types from private CLI scope to **public** `graphify-report::check_report` (`Serialize` + `Deserialize`).
2. Make `graphify check` write `<project_out>/check-report.json` **unconditionally** (additive — stdout `--json` output preserved).
3. Adopt **exit 1** for all `pr-summary` error paths (matches `cmd_diff`/`cmd_trend`; uniform across the Graphify CLI even though POSIX would suggest exit 2 for usage errors).

Per-list cap of 5 rows; fixed section order; no configuration in v1; deterministic output.

## Consequences

### Positive

- Pure renderer is trivially testable (16 unit + 7 CLI + 2 e2e tests)
- LLM-friendly format: Markdown + identifiers in backticks + inline commands
- Graceful degradation — missing optional files become helpful hints, not errors
- One-line GitHub Actions integration: `graphify pr-summary ./report/X >> "$GITHUB_STEP_SUMMARY"`
- Type-move forced an honest split between **producer** (`check`) and **consumer** (`pr-summary`) — both now read the same public types
- Determinism: same inputs → same output → suitable for snapshot testing
- `graphify check` now writes a debuggable on-disk artifact even without `--json` flag

### Negative

- Adds a **third on-disk artifact** to manage (`check-report.json`) — tooling that prunes Graphify outputs needs to know about it
- Hard-coded section order, hard-coded 5-row cap — opinionated; users wanting more control wait for v2
- Exit-1 convention deviates from POSIX usage-2 norm — documented, but may surprise CI scripts that special-case exit 2
- Pure renderer can't compute "new vs pre-existing" violations without a baseline `check-report.json` — left for v2
- Future MCP `summarize_for_pr` tool would be a third consumer — not yet wired

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Pure renderer + ecosystem changes** (chosen) | Clean separation; testable; everything wired | Three changes in one feature |
| Render inline in `graphify check` | Single command | Couples gating with formatting |
| Companion GitHub Action wrapper | Easy onboarding | Second release surface; adds `parisgroup-ai/graphify-action` |
| SARIF output | Industry standard | Security-flavored; awkward fit for arch findings |
| MCP-only `summarize_for_pr` tool | LLM-native | Wrong audience — PR summary is CI-facing, MCP is editor-facing |

## Plan de Rollback

**Triggers:** Markdown layout proves unstable across LLM/reviewer styles; or the `check-report.json` on-disk artifact creates noise in `git status` / cache directories.

**Steps:**
1. Stop writing `check-report.json` from `graphify check` (revert to stdout-only `--json`)
2. Remove `Commands::PrSummary` from `graphify-cli`
3. Move `CheckReport` types **back** to private — but only if no external consumer materializes
4. Bump CLI version (visible behavior change)

**Validation:** `graphify check` still passes/fails CI identically. No `check-report.json` files left behind. Renderer module deleted; tests removed.

## Links

- Spec: `docs/superpowers/specs/2026-04-14-feat-015-pr-summary-cli-design.md`
- Plan: `docs/superpowers/plans/2026-04-14-feat-015-pr-summary-cli.md`
- Task: `[[FEAT-015-pr-and-editor-integration]]`
- Related ADRs: [[ADR-007 Architectural Drift Detection]] (drift input), [[ADR-008 CI Quality Gates]] (check input), [[ADR-011 Contract Drift Detection]] (contracts subsection)
