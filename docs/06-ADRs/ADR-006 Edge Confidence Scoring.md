---
title: "ADR-006: Edge Confidence Scoring with ConfidenceKind"
created: 2026-04-12
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-008"
tags:
  - type/adr
  - status/accepted
  - data-model
  - extract
supersedes:
superseded_by:
---

# ADR-006: Edge Confidence Scoring

## Status

**Accepted** — 2026-04-12

## Context

Graphify treated all edges as equally reliable. In practice they aren't: a direct `import os` is unambiguous; `from . import utils` depends on heuristic resolution; a bare `bar()` call may not resolve at all. Users had no way to distinguish high-confidence structural facts from best-effort guesses, which contaminated downstream metrics (centrality, hotspots) with low-quality edges.

## Decision

**Chosen option:** Add two fields to every `Edge`:

- `confidence: f64` — continuous score in `[0.0, 1.0]`
- `confidence_kind: ConfidenceKind` — `Extracted` / `Inferred` / `Ambiguous`

Defaults: extractor edges = 1.0/Extracted; bare call sites = 0.7/Inferred. Resolver returns confidence per resolution path (direct=1.0, Python relative=0.9, TS alias=0.85). Non-local downgrade clamps to `min(c, 0.5)` and labels `Ambiguous`. Graph merge keeps the **maximum** confidence across observations.

Surfaced in all outputs (JSON, CSV, Markdown, HTML), filterable in `QueryEngine` (`min_confidence`), and exposed in MCP tools.

## Consequences

### Positive

- Users (and downstream tooling) can filter to only "trusted" edges
- HTML report colors edges by confidence (green/yellow/red) — visible at a glance
- Bare-call false positives no longer pollute hotspot scoring at the same weight
- Builder pattern (`Edge::imports().with_confidence(0.9, Inferred)`) keeps construction ergonomic
- Pure additive schema change — old consumers that ignore the fields still work

### Negative

- `Edge` no longer derives `Eq` automatically (`f64` is not `Eq`); manual `Eq` via `f64::to_bits()` adds boilerplate
- Two new CSV columns at the end — readers parsing by position break (column-name readers fine)
- Adds two fields to every cached edge — `.graphify-cache.json` grows ~10–20%
- Concept of "confidence" is opinionated; values are heuristic, not statistical
- Pipeline complexity: extractor confidence + resolver confidence + ambiguous downgrade rules must compose cleanly

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Score + kind enum** (chosen) | Continuous + categorical; flexible filtering | Two fields to maintain |
| Score only | Simpler | Loses categorical signal in tooling |
| Kind enum only | Simpler | Loses ordering/filtering by threshold |
| Per-extractor confidence config | User-tunable | Premature; defaults suffice |
| Drop low-confidence edges before adding | Cleaner graph | Loses signal; users couldn't override |

## Plan de Rollback

**Triggers:** Confidence values become misleading (e.g., users learn `0.7 ≠ "70% accurate"`); or the manual `Eq` impl hides a real equality bug.

**Steps:**
1. Default all edges to `confidence: 1.0, confidence_kind: Extracted` in extractor and resolver
2. Hide confidence columns/fields behind a `--confidence` flag in outputs
3. Bump cache version to discard caches with confidence-aware payloads
4. If structural: remove `ConfidenceKind` and the `confidence` field; revert `Eq` derive

**Validation:** Reports show identical content with and without confidence flags. `cargo test --workspace` green.

## Links

- Spec: `docs/superpowers/specs/2026-04-12-feat-008-confidence-scoring-design.md`
- Plan: `docs/superpowers/plans/2026-04-12-feat-008-confidence-scoring.md`
- Task: `[[FEAT-008-confidence-scoring]]`
- Related ADRs: [[ADR-001 Rust Rewrite]], [[ADR-003 SHA256 Extraction Cache]] (cache schema)
