---
title: "ADR-008: CI Quality Gates (`graphify check`)"
created: 2026-04-13
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-004"
tags:
  - type/adr
  - status/accepted
  - cli
  - ci
supersedes:
superseded_by:
---

# ADR-008: CI Quality Gates

## Status

**Accepted** — 2026-04-13

## Context

Architecture violations silently accumulate without enforcement. Developers asked: "How do I fail a PR if it introduces a new circular dependency or a hotspot above some threshold?" We needed a CI-friendly subcommand with:

- Non-zero exit code on violations
- Stable JSON output for parsers
- Per-project evaluation with multi-project rollup
- No additional artifacts to manage (recompute in memory)

## Decision

**Chosen option:** Add `graphify check` as a new subcommand. Re-runs extract+analyze in memory; evaluates configurable gates (`--max-cycles`, `--max-hotspot-score`); emits human or `--json` output; **exits 1 on any violation** in any project.

Initial gates: `max_cycles` and `max_hotspot_score`. Designed for extension — FEAT-013 (policy rules) and FEAT-016 (contract drift) plug into the same `CheckReport` shape later.

Without explicit limits, the command prints a summary and exits 0 — safe to call in CI without configuration.

## Consequences

### Positive

- Single command answers "is this PR architecturally OK?"
- JSON contract is stable and machine-parseable
- Per-project FAIL/PASS lines are scannable in CI logs
- No new artifacts on disk by default — pure in-memory evaluation
- Extension point used by FEAT-013, FEAT-016, FEAT-015 — `CheckReport` becomes the unified gate output
- Permissive default (no limits → exit 0) keeps adoption friction low

### Negative

- Re-extracts every run — slower than reading existing `analysis.json` (mitigated by [[ADR-003 SHA256 Extraction Cache]])
- Initial gate set is small (cycles + hotspot) — ambitious users want more knobs
- Single exit code aggregates all projects — losing per-project granularity in shell pipelines (use `--json`)
- Exit-1 convention (not exit-2) deviates from POSIX "usage = 2" norm — kept for uniformity across the Graphify CLI

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **`graphify check` re-runs in memory** (chosen) | Always fresh; no stale artifacts | Re-extract cost (cache mitigates) |
| Read existing `analysis.json` | Faster | Stale data; breaks when missing |
| External script over `--json` output | Most flexible | Every user reinvents it |
| Ship a wrapper GitHub Action | Easy onboarding | Adds release surface; not in v1 scope |
| Severity levels (warn/error) | Richer output | Premature; binary pass/fail covers v1 |

## Plan de Rollback

**Triggers:** Gate semantics change in a way that breaks existing CI parsers.

**Steps:**
1. Bump JSON schema version inside `CheckReport` and document the change
2. If structural: deprecate `graphify check`, point users at `graphify run` + manual jq filtering
3. Remove `Commands::Check` from `graphify-cli`

**Validation:** Existing CI workflows continue to call `graphify check`; if removed, they get a clear "unknown command" exit 1 with no false positives.

## Links

- Spec: `docs/superpowers/specs/2026-04-13-feat-004-ci-quality-gates-design.md`
- Plan: `docs/superpowers/plans/2026-04-13-feat-004-ci-quality-gates.md`
- Task: `[[FEAT-004-ci-quality-gates]]`
- Related ADRs: [[ADR-007 Architectural Drift Detection]], [[ADR-011 Contract Drift Detection]], [[ADR-012 PR Summary CLI]]
