---
title: "ADR-007: Architectural Drift Detection (`graphify diff`)"
created: 2026-04-13
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-002"
tags:
  - type/adr
  - status/accepted
  - cli
  - core
supersedes:
superseded_by:
---

# ADR-007: Architectural Drift Detection

## Status

**Accepted** — 2026-04-13

## Context

Architecture changes silently across PRs: a new module appears, an existing one accumulates dependencies, a cycle is introduced. Reading the current `analysis.json` doesn't tell you what **changed**. We needed a way to compare two snapshots and surface the differences along axes that match how developers think (new/removed nodes, new/resolved cycles, hotspot movement, community shifts).

## Decision

**Chosen option:** Add `graphify diff` with two modes:

- `--before X.json --after Y.json` — compare two saved snapshots
- `--baseline X.json --config graphify.toml` — compare a stored baseline against the live codebase

A new `AnalysisSnapshot` type in `graphify-core/src/diff.rs` deserializes `analysis.json` independently of the internal analysis types — **decoupled from internal data structures** so future internal refactors don't break drift compatibility.

`compute_diff(before, after, threshold)` is a pure function returning a `DiffReport` covering 5 dimensions (summary, edges, cycles, hotspots, communities). Outputs `drift-report.{json,md}`.

Community equivalence handled via **max-overlap matching** since community IDs are unstable across runs.

## Consequences

### Positive

- 5-dimension delta gives developers a clear "what changed" view
- Snapshot-based design decouples drift from internal types — future analysis refactors don't break drift compatibility
- Pure `compute_diff()` function is trivially testable
- Markdown output reads well in PR descriptions; JSON drives CI integrations
- Threshold filter (default 0.05) prevents noise from micro-fluctuations
- No new dependencies

### Negative

- Doesn't operate on `graph.json` (edge-level) — degree changes proxy for edge changes; missing rare patterns
- Community comparison via max-overlap is heuristic — small graphs can show "moves" that are really renumbering noise
- Requires the user to manage baseline files (no automatic baseline storage)
- No HTML diff visualization yet
- Cycles compared as sorted node lists — same cycle with different rotation entry point would compare unequal (mitigated by canonical sorting)

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Snapshot diff with 5 dimensions** (chosen) | Decoupled, complete, testable | Heuristic on communities |
| Full `graph.json` edge diff | Most precise | Large outputs, hard to read |
| Single "drift score" number | Simple | Loses all detail; hard to act on |
| Git-style unified diff | Familiar format | Doesn't fit metric/community changes |
| External tool (script over JSON) | No code changes | Reinvents the wheel for every user |

## Plan de Rollback

**Triggers:** Drift signals become misleading (false positives drown out real changes); or the snapshot deserialization breaks on a future analysis schema change.

**Steps:**
1. Remove `Commands::Diff` from `graphify-cli`
2. Keep `compute_diff()` and `AnalysisSnapshot` in core (used by `graphify pr-summary`)
3. Document the new contract in [[ADR-Index]]

**Validation:** `graphify diff --help` returns "unknown command". Other commands unaffected.

## Links

- Spec: `docs/superpowers/specs/2026-04-13-feat-002-architectural-drift-detection-design.md`
- Plan: `docs/superpowers/plans/2026-04-13-feat-002-architectural-drift-detection.md`
- Task: `[[FEAT-002-architectural-drift-detection]]`
- Related ADRs: [[ADR-001 Rust Rewrite]], [[ADR-008 CI Quality Gates]] (parallel CI surface), [[ADR-012 PR Summary CLI]] (consumer)
