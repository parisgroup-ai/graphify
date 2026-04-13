---
uid: feat-008
status: done
priority: normal
timeEstimate: 480
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - graph
  - metrics
---

# Edge confidence scoring

Add confidence classification and scoring to edges so users know how reliable each relationship is.

## Goals

- Classify edges as `Extracted` (found in AST), `Inferred` (resolved by heuristic), or `Ambiguous` (placeholder/unresolved)
- Add `confidence: f64` field (0.0–1.0) to Edge type
- AST-extracted imports/calls → confidence 1.0, type `Extracted`
- Resolved relative imports → confidence 0.8–0.9, type `Extracted`
- Placeholder nodes (unresolved refs) → confidence 0.3–0.5, type `Ambiguous`
- Include confidence breakdown in reports (% extracted, % inferred, % ambiguous)
- Filter edges by confidence in query interface (e.g., `--min-confidence 0.8`)

## Inspiration

safishamsi/graphify tags every edge as EXTRACTED, INFERRED, or AMBIGUOUS with a per-edge confidence_score (0.0-1.0). This transparency lets users know what was discovered vs guessed. Ambiguous edges are flagged for review rather than hidden.

## Subtasks

- [x] Add `confidence: f64` and `confidence_kind: ConfidenceKind` to Edge in types.rs
- [x] Set confidence during extraction (python.rs, typescript.rs)
- [x] Set confidence during resolution (resolver.rs)
- [x] Include confidence stats in analysis output
- [x] Add confidence column to CSV export
- [x] Filter by confidence in reports (highlight ambiguous edges)
- [x] Tests: verify confidence values for different edge scenarios

## Notes

This is relatively low-effort since we already track edge types. The main work is propagating confidence through the pipeline and surfacing it in outputs.
