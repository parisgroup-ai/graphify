# FEAT-002: Architectural Drift Detection — Design Spec

**Date:** 2026-04-13
**Status:** Approved
**Feature:** `graphify diff` — compare two analysis snapshots to surface architectural changes

## Overview

Compare two `analysis.json` snapshots over time to detect what changed: new/removed nodes, new/resolved cycles, hotspot score movements, community shifts, and summary deltas. Output as JSON (programmatic) and Markdown (human-readable).

## CLI Interface

Two modes:

```bash
# File vs file — compare two saved snapshots
graphify diff --before analysis-v1.json --after analysis-v2.json [--output diff-report/] [--threshold 0.05]

# Baseline vs live — compare saved baseline against current codebase
graphify diff --baseline analysis-baseline.json --config graphify.toml [--project my-project] [--output diff-report/] [--threshold 0.05]
```

**Validation:** Either `--before` + `--after` are both set, or `--baseline` + `--config` are both set. Error otherwise.

**Default threshold:** 0.05 (minimum score delta to report as significant hotspot movement).

## Data Model

### Input: AnalysisSnapshot

A deserializable representation of `analysis.json`, decoupled from internal analysis types. Lives in `graphify-core/src/diff.rs`.

```rust
#[derive(Deserialize)]
pub struct AnalysisSnapshot {
    pub nodes: Vec<NodeSnapshot>,
    pub communities: Vec<CommunitySnapshot>,
    pub cycles: Vec<Vec<String>>,
    pub summary: SummarySnapshot,
}

#[derive(Deserialize)]
pub struct NodeSnapshot {
    pub id: String,
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: usize,
    pub out_degree: usize,
    pub in_cycle: bool,
    pub score: f64,
    pub community_id: usize,
}

#[derive(Deserialize)]
pub struct CommunitySnapshot {
    pub id: usize,
    pub members: Vec<String>,
}

#[derive(Deserialize)]
pub struct SummarySnapshot {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_communities: usize,
    pub total_cycles: usize,
}
```

### Output: DiffReport

```rust
#[derive(Serialize)]
pub struct DiffReport {
    pub summary_delta: SummaryDelta,
    pub edges: EdgeDiff,
    pub cycles: CycleDiff,
    pub hotspots: HotspotDiff,
    pub communities: CommunityDiff,
}

#[derive(Serialize)]
pub struct SummaryDelta {
    pub nodes: Delta<usize>,
    pub edges: Delta<usize>,
    pub communities: Delta<usize>,
    pub cycles: Delta<usize>,
}

#[derive(Serialize)]
pub struct Delta<T> {
    pub before: T,
    pub after: T,
    pub change: i64,
}

#[derive(Serialize)]
pub struct EdgeDiff {
    pub added_nodes: Vec<String>,
    pub removed_nodes: Vec<String>,
    pub degree_changes: Vec<DegreeChange>,
}

#[derive(Serialize)]
pub struct DegreeChange {
    pub id: String,
    pub in_degree: Delta<usize>,
    pub out_degree: Delta<usize>,
}

#[derive(Serialize)]
pub struct CycleDiff {
    pub introduced: Vec<Vec<String>>,
    pub resolved: Vec<Vec<String>>,
}

#[derive(Serialize)]
pub struct HotspotDiff {
    pub rising: Vec<ScoreChange>,
    pub falling: Vec<ScoreChange>,
    pub new_hotspots: Vec<ScoreChange>,
    pub removed_hotspots: Vec<ScoreChange>,
}

#[derive(Serialize)]
pub struct ScoreChange {
    pub id: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
}

#[derive(Serialize)]
pub struct CommunityDiff {
    pub moved_nodes: Vec<CommunityMove>,
    pub stable_count: usize,
}

#[derive(Serialize)]
pub struct CommunityMove {
    pub id: String,
    pub from_community: usize,
    pub to_community: usize,
}
```

## Diff Engine Logic

### Function signature

```rust
pub fn compute_diff(
    before: &AnalysisSnapshot,
    after: &AnalysisSnapshot,
    score_threshold: f64,
) -> DiffReport
```

### Algorithm per dimension

**1. Summary delta** — Direct subtraction: `after.summary.X - before.summary.X` for nodes, edges, communities, cycles.

**2. Edge diff (via degree changes)** — Build `HashMap<String, &NodeSnapshot>` for both snapshots keyed by node ID. Nodes in `after` but not `before` → `added_nodes`. Nodes in `before` but not `after` → `removed_nodes`. For nodes present in both, compare `in_degree` and `out_degree` — emit `DegreeChange` only when either changed.

**3. Cycle diff** — Cycles are already in canonical order (sorted rotation from `find_simple_cycles`). Compare `before.cycles` and `after.cycles` as sorted `Vec<String>` directly. Cycles in `after` not in `before` → `introduced`. Cycles in `before` not in `after` → `resolved`.

**4. Hotspot diff** — For nodes in both snapshots, compute `delta = after.score - before.score`. Filter by `|delta| >= score_threshold`. Positive deltas → `rising` (sorted by delta descending). Negative deltas → `falling` (sorted by delta ascending). Top-20 comparison: take top-20 by score from each snapshot; nodes in after's top-20 but not before's → `new_hotspots`; reverse → `removed_hotspots`.

**5. Community diff** — Build community equivalence map by maximum member overlap: for each `after` community, find the `before` community sharing the most members. For each node in both snapshots, check if its community mapping changed → `moved_nodes`. Count unchanged nodes → `stable_count`.

## Output Files

Written to the output directory (defaults to current directory):

- `drift-report.json` — serialized `DiffReport`
- `drift-report.md` — human-readable Markdown

### Markdown format

```markdown
# Architectural Drift Report

## Summary

| Metric       | Before | After | Change |
|-------------|--------|-------|--------|
| Nodes        | 45     | 48    | +3     |
| Edges        | 120    | 125   | +5     |
| Communities  | 4      | 5     | +1     |
| Cycles       | 2      | 1     | -1     |

## New Nodes (3)
- `app.services.notifications`
- `app.services.email`
- `app.utils.retry`

## Removed Nodes (0)
_None_

## Cycle Changes
### Introduced (0)
_None_

### Resolved (1)
- `app.main` → `app.services.llm` → `app.main`

## Hotspot Movement
### Rising (score delta ≥ 0.05)
| Node | Before | After | Δ |
|------|--------|-------|---|
| `app.services.llm` | 0.42 | 0.58 | +0.16 |

### Falling (score delta ≤ -0.05)
_None_

### New in Top 20
- `app.services.notifications` (score: 0.31)

### Left Top 20
_None_

## Community Shifts
- **3 nodes** moved communities
  - `app.utils.retry`: community 2 → 4
  - ...
- **42 nodes** stable
```

## Crate Changes

| Crate | Files | Change |
|---|---|---|
| `graphify-core` | `src/diff.rs`, `src/lib.rs` | New module: `AnalysisSnapshot`, `DiffReport`, `compute_diff()` |
| `graphify-report` | `src/diff_json.rs`, `src/diff_markdown.rs`, `src/lib.rs` | New output writers for drift report |
| `graphify-cli` | `src/main.rs` | New `Commands::Diff` variant + handler |

**No new dependencies.** Uses existing `serde`, `serde_json`, `std::collections`.

## Testing

### Unit tests in `graphify-core/src/diff.rs` (~10-12 tests)
- Identical snapshots → all deltas zero
- Added/removed nodes detection
- Cycle introduced/resolved detection
- Hotspot rising/falling with threshold filtering
- Community move detection with renumbered IDs
- Empty snapshots (before or after)
- Single-node snapshot
- Threshold 0.0 reports all changes

### Unit tests in `graphify-report` (~4 tests)
- `diff_json.rs`: output exists and is valid JSON; roundtrip serialize/deserialize
- `diff_markdown.rs`: contains expected section headers; summary table correct

### Estimated: ~15 new tests

## Non-Goals (explicit)

- No `graph.json` diffing (edge-level source/target) — degree changes from analysis.json suffice
- No HTML diff visualization — can layer on later
- No automatic baseline management (`graphify baseline save`) — user manages files
- No MCP integration — `compute_diff()` in core makes this easy to add later
- No CSV diff output — diff is structured delta, not tabular data
