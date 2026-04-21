---
uid: feat-033
status: open
priority: normal
scheduled: 2026-04-21
pomodoros: 0
tags:
- task
- feat
- metrics
- hotspot-scoring
- feat-032-followup
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: med
  hintsInferred: true
---

# FEAT: Deprioritize `ExpectedExternal` edges in hotspot scoring

Post-FEAT-032 the ambiguity metric correctly reclassifies `std`/`serde`/etc. calls as `ExpectedExternal` instead of `Ambiguous`. The dogfood `graphify.toml` now carries per-project external_stubs arrays, and 472 edges across the 5-crate workspace get the correct classification.

But the top hotspots of every Rust project still show external packages. In graphify-cli `std::path::PathBuf` sits at #1 (score 0.4, in_degree 6), `Ok`/`format`/`Vec::new` at #2–#4, and the first LOCAL symbol doesn't appear until #6. The reason is simple: metrics computation (`crates/graphify-core/src/metrics.rs` — in_degree, betweenness, pagerank, unified scoring) doesn't look at `confidence_kind`. It counts every incoming edge equally.

The prior session brief (post-BUG-016, 2026-04-21 08:41) misread `external_stubs` as "filter externals out of hotspots." The feature was never that — per `crates/graphify-extract/src/stubs.rs:1-8` it only affects `ConfidenceKind` (Ambiguous → ExpectedExternal). Surfacing local hotspots needs a metrics-level change.

## Design options

### Option A — filter ExpectedExternal edges from scoring inputs

Skip edges with `confidence_kind == ExpectedExternal` when building the degree/pagerank/betweenness input graphs. Cleanest signal: a node whose in-edges are 100% external-stub calls disappears from the hotspot list entirely.

Risk: loss of information — `graphify query "std::*"` output would be empty even though the edges exist in `graph.json`. Mitigate by keeping edges in the graph data but having `compute_metrics` consume a filtered view.

### Option B — weight ExpectedExternal edges lower in scoring

Apply a multiplier (e.g. 0.1) to `ExpectedExternal` edges' contribution to in_degree / pagerank mass. Keeps externals visible but ranked far below local hotspots. More tunable but adds a magic number that consumers will have to understand.

### Option C — two hotspot lists

Compute two rankings — "local hotspots" (excludes ExpectedExternal) and "cross-boundary hotspots" (only ExpectedExternal). Report-renderers show both. Useful for audit flows where someone WANTS to see "which stdlib calls dominate" separately from "what's my biggest internal coupling."

## Recommendation

Start with **Option A** — hotspot lists should be actionable, and `std::path::PathBuf` appearing as the #1 hotspot of graphify-cli is not actionable (you're not going to refactor `std`). Implementation surface: `crates/graphify-core/src/metrics.rs::compute_metrics_with_thresholds` (and the underlying in_degree / betweenness / pagerank helpers), plus the `analysis.json` writer in graphify-report.

If Option A makes the output too sparse in practice, revisit with C (keep the flagship hotspot list clean, expose externals via a `graphify explain` / `graphify query --include-external` flag).

## Out of scope (for v1)

- Surfacing external hotspots as architectural-dependency insights (reserved for a future "boundary audit" feature — Option C above)
- Changing how `confidence_kind: Ambiguous` edges are scored (they reflect extractor uncertainty and deserve visibility)

## Acceptance criteria

- `graphify run` on graphify's own workspace — top 10 hotspots across all 5 crates are dominated by local `src.*` symbols and legitimate cross-crate workspace edges (`graphify_core::*`, `graphify_extract::*`, …), with std/serde/tree_sitter/rayon etc. absent from the top 10.
- `graphify query`/`explain` still expose external node metrics on request (no data loss, just different default ranking).
- `cargo test --workspace` green; `cargo clippy -D warnings` clean.
- No regression on non-Rust dogfood projects (Python / TS / Go / PHP) — external_stubs is Rust-shaped in practice but not Rust-only.

## Discovered context

Discovered 2026-04-21 during FEAT-032 rollout (v0.11.5) when the dogfood config was added and the expected hotspot shift didn't materialise. The depth-cap bug (BUG-017) that also shipped in v0.11.5 was a separate, co-discovered issue unblocking the real measurement of this one.
