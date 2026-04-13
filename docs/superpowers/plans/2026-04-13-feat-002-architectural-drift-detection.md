# FEAT-002: Architectural Drift Detection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `graphify diff` command that compares two analysis snapshots and reports architectural drift across five dimensions: summary deltas, node/edge changes, cycle changes, hotspot movement, and community shifts.

**Architecture:** New `diff` module in `graphify-core` handles snapshot deserialization and diff computation. New `diff_json` and `diff_markdown` modules in `graphify-report` handle output. CLI gets a new `Commands::Diff` variant. No new dependencies.

**Tech Stack:** Rust, serde (Serialize/Deserialize), std::collections (HashMap, HashSet, BTreeSet), clap derive macros.

**Spec:** `docs/superpowers/specs/2026-04-13-feat-002-architectural-drift-detection-design.md`

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `crates/graphify-core/src/diff.rs` | `AnalysisSnapshot` (Deserialize), `DiffReport` + sub-structs (Serialize), `compute_diff()` |
| Modify | `crates/graphify-core/src/lib.rs:1-7` | Add `pub mod diff;` |
| Create | `crates/graphify-report/src/diff_json.rs` | `write_diff_json()` — serialize DiffReport to JSON file |
| Create | `crates/graphify-report/src/diff_markdown.rs` | `write_diff_markdown()` — render DiffReport as Markdown |
| Modify | `crates/graphify-report/src/lib.rs:1-17` | Add `pub mod diff_json; pub mod diff_markdown;` and re-exports |
| Modify | `crates/graphify-cli/src/main.rs:72-253` | Add `Commands::Diff` variant |
| Modify | `crates/graphify-cli/src/main.rs:587-591` | Add `Commands::Diff` match arm + handler |

---

### Task 1: AnalysisSnapshot types + deserialization tests

**Files:**
- Create: `crates/graphify-core/src/diff.rs`
- Modify: `crates/graphify-core/src/lib.rs:1-7`

- [ ] **Step 1: Create diff module with AnalysisSnapshot types**

Create `crates/graphify-core/src/diff.rs`:

```rust
use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Input: AnalysisSnapshot (deserialized from analysis.json)
// ---------------------------------------------------------------------------

/// A deserializable snapshot of analysis.json — the input to diffing.
///
/// This is intentionally decoupled from the internal `NodeMetrics`/`Community`
/// types. It mirrors the JSON shape exactly so any analysis.json can be loaded.
#[derive(Debug, Clone, Deserialize)]
pub struct AnalysisSnapshot {
    pub nodes: Vec<NodeSnapshot>,
    pub communities: Vec<CommunitySnapshot>,
    pub cycles: Vec<Vec<String>>,
    pub summary: SummarySnapshot,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct CommunitySnapshot {
    pub id: usize,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SummarySnapshot {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_communities: usize,
    pub total_cycles: usize,
}

// ---------------------------------------------------------------------------
// Output: DiffReport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub summary_delta: SummaryDelta,
    pub edges: EdgeDiff,
    pub cycles: CycleDiff,
    pub hotspots: HotspotDiff,
    pub communities: CommunityDiff,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummaryDelta {
    pub nodes: Delta<usize>,
    pub edges: Delta<usize>,
    pub communities: Delta<usize>,
    pub cycles: Delta<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Delta<T: Serialize> {
    pub before: T,
    pub after: T,
    pub change: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeDiff {
    pub added_nodes: Vec<String>,
    pub removed_nodes: Vec<String>,
    pub degree_changes: Vec<DegreeChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DegreeChange {
    pub id: String,
    pub in_degree: Delta<usize>,
    pub out_degree: Delta<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleDiff {
    pub introduced: Vec<Vec<String>>,
    pub resolved: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HotspotDiff {
    pub rising: Vec<ScoreChange>,
    pub falling: Vec<ScoreChange>,
    pub new_hotspots: Vec<ScoreChange>,
    pub removed_hotspots: Vec<ScoreChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoreChange {
    pub id: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunityDiff {
    pub moved_nodes: Vec<CommunityMove>,
    pub stable_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunityMove {
    pub id: String,
    pub from_community: usize,
    pub to_community: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: minimal valid analysis JSON matching the real output format.
    fn sample_analysis_json() -> &'static str {
        r#"{
            "nodes": [
                {
                    "id": "app.main",
                    "betweenness": 0.5,
                    "pagerank": 0.3,
                    "in_degree": 2,
                    "out_degree": 3,
                    "in_cycle": false,
                    "score": 0.45,
                    "community_id": 0
                }
            ],
            "communities": [
                { "id": 0, "members": ["app.main"] }
            ],
            "cycles": [],
            "summary": {
                "total_nodes": 1,
                "total_edges": 3,
                "total_communities": 1,
                "total_cycles": 0,
                "top_hotspots": [["app.main", 0.45]]
            },
            "confidence_summary": {
                "extracted_count": 3,
                "extracted_pct": 100.0,
                "inferred_count": 0,
                "inferred_pct": 0.0,
                "ambiguous_count": 0,
                "ambiguous_pct": 0.0,
                "mean_confidence": 1.0
            }
        }"#
    }

    #[test]
    fn deserialize_analysis_snapshot_from_json() {
        let snapshot: AnalysisSnapshot =
            serde_json::from_str(sample_analysis_json()).expect("deserialize");
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.nodes[0].id, "app.main");
        assert_eq!(snapshot.summary.total_nodes, 1);
        assert_eq!(snapshot.summary.total_edges, 3);
        assert_eq!(snapshot.communities.len(), 1);
        assert!(snapshot.cycles.is_empty());
    }

    #[test]
    fn deserialize_ignores_unknown_fields() {
        // analysis.json has confidence_summary and top_hotspots which are not
        // in our snapshot structs — serde should ignore them silently.
        let snapshot: AnalysisSnapshot =
            serde_json::from_str(sample_analysis_json()).expect("deserialize");
        assert_eq!(snapshot.nodes[0].score, 0.45);
    }
}
```

- [ ] **Step 2: Register the diff module in lib.rs**

Modify `crates/graphify-core/src/lib.rs` — add `pub mod diff;` after the existing modules:

```rust
pub mod community;
pub mod cycles;
pub mod diff;
pub mod graph;
pub mod metrics;
pub mod query;
pub mod types;
```

- [ ] **Step 3: Run tests to verify deserialization works**

Run: `cargo test -p graphify-core diff`
Expected: 2 tests pass (deserialize_analysis_snapshot_from_json, deserialize_ignores_unknown_fields)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-core/src/diff.rs crates/graphify-core/src/lib.rs
git commit -m "feat(core): AnalysisSnapshot + DiffReport types for FEAT-002 drift detection"
```

---

### Task 2: compute_diff — summary delta + edge diff

**Files:**
- Modify: `crates/graphify-core/src/diff.rs`

- [ ] **Step 1: Write failing tests for summary delta and edge diff**

Add to the `mod tests` block in `crates/graphify-core/src/diff.rs`:

```rust
    fn make_snapshot(
        nodes: Vec<NodeSnapshot>,
        communities: Vec<CommunitySnapshot>,
        cycles: Vec<Vec<String>>,
        total_edges: usize,
    ) -> AnalysisSnapshot {
        let total_nodes = nodes.len();
        let total_communities = communities.len();
        let total_cycles = cycles.len();
        AnalysisSnapshot {
            nodes,
            communities,
            cycles,
            summary: SummarySnapshot {
                total_nodes,
                total_edges,
                total_communities,
                total_cycles,
            },
        }
    }

    fn node(id: &str, score: f64, in_deg: usize, out_deg: usize, community: usize) -> NodeSnapshot {
        NodeSnapshot {
            id: id.to_string(),
            betweenness: 0.0,
            pagerank: 0.0,
            in_degree: in_deg,
            out_degree: out_deg,
            in_cycle: false,
            score,
            community_id: community,
        }
    }

    #[test]
    fn diff_identical_snapshots_all_zeros() {
        let a = make_snapshot(
            vec![node("x", 0.5, 2, 3, 0)],
            vec![CommunitySnapshot { id: 0, members: vec!["x".into()] }],
            vec![],
            3,
        );
        let report = compute_diff(&a, &a, 0.05);
        assert_eq!(report.summary_delta.nodes.change, 0);
        assert_eq!(report.summary_delta.edges.change, 0);
        assert!(report.edges.added_nodes.is_empty());
        assert!(report.edges.removed_nodes.is_empty());
        assert!(report.edges.degree_changes.is_empty());
    }

    #[test]
    fn diff_detects_added_and_removed_nodes() {
        let before = make_snapshot(
            vec![node("a", 0.5, 2, 1, 0), node("b", 0.3, 1, 0, 0)],
            vec![CommunitySnapshot { id: 0, members: vec!["a".into(), "b".into()] }],
            vec![],
            3,
        );
        let after = make_snapshot(
            vec![node("a", 0.5, 2, 1, 0), node("c", 0.2, 0, 1, 0)],
            vec![CommunitySnapshot { id: 0, members: vec!["a".into(), "c".into()] }],
            vec![],
            3,
        );
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.edges.added_nodes, vec!["c"]);
        assert_eq!(report.edges.removed_nodes, vec!["b"]);
    }

    #[test]
    fn diff_detects_degree_changes() {
        let before = make_snapshot(vec![node("a", 0.5, 2, 1, 0)], vec![], vec![], 2);
        let after = make_snapshot(vec![node("a", 0.5, 5, 1, 0)], vec![], vec![], 5);
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.edges.degree_changes.len(), 1);
        assert_eq!(report.edges.degree_changes[0].id, "a");
        assert_eq!(report.edges.degree_changes[0].in_degree.before, 2);
        assert_eq!(report.edges.degree_changes[0].in_degree.after, 5);
        assert_eq!(report.edges.degree_changes[0].in_degree.change, 3);
    }
```

- [ ] **Step 2: Run tests — verify they fail**

Run: `cargo test -p graphify-core diff`
Expected: FAIL — `compute_diff` not found.

- [ ] **Step 3: Implement compute_diff (summary delta + edge diff)**

Add above `#[cfg(test)]` in `crates/graphify-core/src/diff.rs`:

```rust
// ---------------------------------------------------------------------------
// compute_diff
// ---------------------------------------------------------------------------

/// Compares two analysis snapshots and produces a structured diff report.
///
/// `score_threshold` controls the minimum absolute score delta to consider
/// a hotspot change significant (e.g. 0.05).
pub fn compute_diff(
    before: &AnalysisSnapshot,
    after: &AnalysisSnapshot,
    score_threshold: f64,
) -> DiffReport {
    let summary_delta = compute_summary_delta(before, after);
    let edges = compute_edge_diff(before, after);
    let cycles = compute_cycle_diff(before, after);
    let hotspots = compute_hotspot_diff(before, after, score_threshold);
    let communities = compute_community_diff(before, after);

    DiffReport {
        summary_delta,
        edges,
        cycles,
        hotspots,
        communities,
    }
}

// ---------------------------------------------------------------------------
// Summary delta
// ---------------------------------------------------------------------------

fn compute_summary_delta(before: &AnalysisSnapshot, after: &AnalysisSnapshot) -> SummaryDelta {
    SummaryDelta {
        nodes: Delta {
            before: before.summary.total_nodes,
            after: after.summary.total_nodes,
            change: after.summary.total_nodes as i64 - before.summary.total_nodes as i64,
        },
        edges: Delta {
            before: before.summary.total_edges,
            after: after.summary.total_edges,
            change: after.summary.total_edges as i64 - before.summary.total_edges as i64,
        },
        communities: Delta {
            before: before.summary.total_communities,
            after: after.summary.total_communities,
            change: after.summary.total_communities as i64
                - before.summary.total_communities as i64,
        },
        cycles: Delta {
            before: before.summary.total_cycles,
            after: after.summary.total_cycles,
            change: after.summary.total_cycles as i64 - before.summary.total_cycles as i64,
        },
    }
}

// ---------------------------------------------------------------------------
// Edge diff (via degree changes)
// ---------------------------------------------------------------------------

fn compute_edge_diff(before: &AnalysisSnapshot, after: &AnalysisSnapshot) -> EdgeDiff {
    let before_map: HashMap<&str, &NodeSnapshot> =
        before.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let after_map: HashMap<&str, &NodeSnapshot> =
        after.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut added_nodes: Vec<String> = after_map
        .keys()
        .filter(|id| !before_map.contains_key(*id))
        .map(|id| id.to_string())
        .collect();
    added_nodes.sort();

    let mut removed_nodes: Vec<String> = before_map
        .keys()
        .filter(|id| !after_map.contains_key(*id))
        .map(|id| id.to_string())
        .collect();
    removed_nodes.sort();

    let mut degree_changes: Vec<DegreeChange> = Vec::new();
    for (id, before_node) in &before_map {
        if let Some(after_node) = after_map.get(id) {
            if before_node.in_degree != after_node.in_degree
                || before_node.out_degree != after_node.out_degree
            {
                degree_changes.push(DegreeChange {
                    id: id.to_string(),
                    in_degree: Delta {
                        before: before_node.in_degree,
                        after: after_node.in_degree,
                        change: after_node.in_degree as i64 - before_node.in_degree as i64,
                    },
                    out_degree: Delta {
                        before: before_node.out_degree,
                        after: after_node.out_degree,
                        change: after_node.out_degree as i64 - before_node.out_degree as i64,
                    },
                });
            }
        }
    }
    degree_changes.sort_by(|a, b| a.id.cmp(&b.id));

    EdgeDiff {
        added_nodes,
        removed_nodes,
        degree_changes,
    }
}

// ---------------------------------------------------------------------------
// Cycle diff
// ---------------------------------------------------------------------------

fn compute_cycle_diff(before: &AnalysisSnapshot, after: &AnalysisSnapshot) -> CycleDiff {
    let before_set: BTreeSet<&Vec<String>> = before.cycles.iter().collect();
    let after_set: BTreeSet<&Vec<String>> = after.cycles.iter().collect();

    let introduced: Vec<Vec<String>> = after_set
        .difference(&before_set)
        .map(|c| (*c).clone())
        .collect();
    let resolved: Vec<Vec<String>> = before_set
        .difference(&after_set)
        .map(|c| (*c).clone())
        .collect();

    CycleDiff {
        introduced,
        resolved,
    }
}

// ---------------------------------------------------------------------------
// Hotspot diff
// ---------------------------------------------------------------------------

fn compute_hotspot_diff(
    before: &AnalysisSnapshot,
    after: &AnalysisSnapshot,
    score_threshold: f64,
) -> HotspotDiff {
    let before_map: HashMap<&str, &NodeSnapshot> =
        before.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let after_map: HashMap<&str, &NodeSnapshot> =
        after.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut rising: Vec<ScoreChange> = Vec::new();
    let mut falling: Vec<ScoreChange> = Vec::new();

    // Score changes for nodes in both snapshots.
    for (id, before_node) in &before_map {
        if let Some(after_node) = after_map.get(id) {
            let delta = after_node.score - before_node.score;
            if delta.abs() >= score_threshold {
                let change = ScoreChange {
                    id: id.to_string(),
                    before: before_node.score,
                    after: after_node.score,
                    delta,
                };
                if delta > 0.0 {
                    rising.push(change);
                } else {
                    falling.push(change);
                }
            }
        }
    }

    rising.sort_by(|a, b| b.delta.partial_cmp(&a.delta).unwrap_or(std::cmp::Ordering::Equal));
    falling.sort_by(|a, b| a.delta.partial_cmp(&b.delta).unwrap_or(std::cmp::Ordering::Equal));

    // Top-20 hotspot comparison.
    let top_n = 20;
    let before_top: Vec<&str> = {
        let mut sorted: Vec<&NodeSnapshot> = before.nodes.iter().collect();
        sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        sorted.iter().take(top_n).map(|n| n.id.as_str()).collect()
    };
    let after_top: Vec<&str> = {
        let mut sorted: Vec<&NodeSnapshot> = after.nodes.iter().collect();
        sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        sorted.iter().take(top_n).map(|n| n.id.as_str()).collect()
    };

    let before_top_set: std::collections::HashSet<&str> = before_top.into_iter().collect();
    let after_top_set: std::collections::HashSet<&str> = after_top.into_iter().collect();

    let mut new_hotspots: Vec<ScoreChange> = after_top_set
        .difference(&before_top_set)
        .map(|&id| {
            let after_score = after_map.get(id).map(|n| n.score).unwrap_or(0.0);
            let before_score = before_map.get(id).map(|n| n.score).unwrap_or(0.0);
            ScoreChange {
                id: id.to_string(),
                before: before_score,
                after: after_score,
                delta: after_score - before_score,
            }
        })
        .collect();
    new_hotspots.sort_by(|a, b| b.after.partial_cmp(&a.after).unwrap_or(std::cmp::Ordering::Equal));

    let mut removed_hotspots: Vec<ScoreChange> = before_top_set
        .difference(&after_top_set)
        .map(|&id| {
            let before_score = before_map.get(id).map(|n| n.score).unwrap_or(0.0);
            let after_score = after_map.get(id).map(|n| n.score).unwrap_or(0.0);
            ScoreChange {
                id: id.to_string(),
                before: before_score,
                after: after_score,
                delta: after_score - before_score,
            }
        })
        .collect();
    removed_hotspots.sort_by(|a, b| {
        b.before
            .partial_cmp(&a.before)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    HotspotDiff {
        rising,
        falling,
        new_hotspots,
        removed_hotspots,
    }
}

// ---------------------------------------------------------------------------
// Community diff
// ---------------------------------------------------------------------------

fn compute_community_diff(
    before: &AnalysisSnapshot,
    after: &AnalysisSnapshot,
) -> CommunityDiff {
    // Build node→community_id maps.
    let before_comm: HashMap<&str, usize> = before
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.community_id))
        .collect();
    let after_comm: HashMap<&str, usize> = after
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.community_id))
        .collect();

    // Build community equivalence map: for each after community, find the
    // before community with maximum member overlap.
    let before_members: HashMap<usize, Vec<&str>> = {
        let mut map: HashMap<usize, Vec<&str>> = HashMap::new();
        for c in &before.communities {
            map.insert(c.id, c.members.iter().map(|s| s.as_str()).collect());
        }
        map
    };
    let after_members: HashMap<usize, Vec<&str>> = {
        let mut map: HashMap<usize, Vec<&str>> = HashMap::new();
        for c in &after.communities {
            map.insert(c.id, c.members.iter().map(|s| s.as_str()).collect());
        }
        map
    };

    // Map after_community_id → best matching before_community_id.
    let mut equiv: HashMap<usize, usize> = HashMap::new();
    for (&after_id, after_mems) in &after_members {
        let after_set: std::collections::HashSet<&str> = after_mems.iter().copied().collect();
        let mut best_id = after_id; // fallback: map to self
        let mut best_overlap = 0usize;
        for (&before_id, before_mems) in &before_members {
            let overlap = before_mems
                .iter()
                .filter(|m| after_set.contains(*m))
                .count();
            if overlap > best_overlap {
                best_overlap = overlap;
                best_id = before_id;
            }
        }
        equiv.insert(after_id, best_id);
    }

    // For each node in both snapshots, check if its mapped community changed.
    let mut moved_nodes: Vec<CommunityMove> = Vec::new();
    let mut stable_count = 0usize;

    for (id, &before_cid) in &before_comm {
        if let Some(&after_cid) = after_comm.get(id) {
            // Map the after community back to its equivalent before community.
            let mapped_before = equiv.get(&after_cid).copied().unwrap_or(after_cid);
            if mapped_before != before_cid {
                moved_nodes.push(CommunityMove {
                    id: id.to_string(),
                    from_community: before_cid,
                    to_community: after_cid,
                });
            } else {
                stable_count += 1;
            }
        }
    }

    moved_nodes.sort_by(|a, b| a.id.cmp(&b.id));

    CommunityDiff {
        moved_nodes,
        stable_count,
    }
}
```

- [ ] **Step 4: Run tests — verify they pass**

Run: `cargo test -p graphify-core diff`
Expected: 5 tests pass (2 from Task 1 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/src/diff.rs
git commit -m "feat(core): compute_diff — summary delta + edge diff (FEAT-002)"
```

---

### Task 3: compute_diff — cycle, hotspot, and community diff tests

**Files:**
- Modify: `crates/graphify-core/src/diff.rs` (tests section only)

- [ ] **Step 1: Add tests for cycle, hotspot, and community diffs**

Add to the `mod tests` block in `crates/graphify-core/src/diff.rs`:

```rust
    #[test]
    fn diff_detects_introduced_and_resolved_cycles() {
        let before = make_snapshot(
            vec![node("a", 0.5, 1, 1, 0), node("b", 0.3, 1, 1, 0)],
            vec![],
            vec![vec!["a".into(), "b".into()]],
            2,
        );
        let after = make_snapshot(
            vec![
                node("a", 0.5, 1, 1, 0),
                node("b", 0.3, 1, 1, 0),
                node("c", 0.2, 1, 1, 0),
            ],
            vec![],
            vec![vec!["b".into(), "c".into()]],
            3,
        );
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.cycles.introduced.len(), 1);
        assert_eq!(report.cycles.introduced[0], vec!["b", "c"]);
        assert_eq!(report.cycles.resolved.len(), 1);
        assert_eq!(report.cycles.resolved[0], vec!["a", "b"]);
    }

    #[test]
    fn diff_detects_rising_and_falling_hotspots() {
        let before = make_snapshot(
            vec![
                node("a", 0.80, 5, 3, 0),
                node("b", 0.20, 1, 1, 0),
            ],
            vec![],
            vec![],
            6,
        );
        let after = make_snapshot(
            vec![
                node("a", 0.50, 5, 3, 0), // fell by 0.30
                node("b", 0.60, 1, 1, 0), // rose by 0.40
            ],
            vec![],
            vec![],
            6,
        );
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.hotspots.rising.len(), 1);
        assert_eq!(report.hotspots.rising[0].id, "b");
        assert!((report.hotspots.rising[0].delta - 0.40).abs() < 1e-9);
        assert_eq!(report.hotspots.falling.len(), 1);
        assert_eq!(report.hotspots.falling[0].id, "a");
        assert!((report.hotspots.falling[0].delta - (-0.30)).abs() < 1e-9);
    }

    #[test]
    fn diff_threshold_filters_small_changes() {
        let before = make_snapshot(vec![node("a", 0.50, 2, 1, 0)], vec![], vec![], 2);
        let after = make_snapshot(vec![node("a", 0.52, 2, 1, 0)], vec![], vec![], 2);
        // delta = 0.02, threshold = 0.05 → should be filtered out
        let report = compute_diff(&before, &after, 0.05);
        assert!(report.hotspots.rising.is_empty());
        assert!(report.hotspots.falling.is_empty());
    }

    #[test]
    fn diff_threshold_zero_reports_all() {
        let before = make_snapshot(vec![node("a", 0.50, 2, 1, 0)], vec![], vec![], 2);
        let after = make_snapshot(vec![node("a", 0.501, 2, 1, 0)], vec![], vec![], 2);
        let report = compute_diff(&before, &after, 0.0);
        assert_eq!(report.hotspots.rising.len(), 1);
    }

    #[test]
    fn diff_detects_community_moves() {
        let before = make_snapshot(
            vec![
                node("a", 0.5, 2, 1, 0),
                node("b", 0.3, 1, 1, 0),
                node("c", 0.2, 1, 0, 1),
            ],
            vec![
                CommunitySnapshot { id: 0, members: vec!["a".into(), "b".into()] },
                CommunitySnapshot { id: 1, members: vec!["c".into()] },
            ],
            vec![],
            4,
        );
        // In after: "b" moved from community 0 to community 1.
        // After community 0 still has "a", after community 1 has "b" + "c".
        let after = make_snapshot(
            vec![
                node("a", 0.5, 2, 1, 0),
                node("b", 0.3, 1, 1, 1),
                node("c", 0.2, 1, 0, 1),
            ],
            vec![
                CommunitySnapshot { id: 0, members: vec!["a".into()] },
                CommunitySnapshot { id: 1, members: vec!["b".into(), "c".into()] },
            ],
            vec![],
            4,
        );
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.communities.moved_nodes.len(), 1);
        assert_eq!(report.communities.moved_nodes[0].id, "b");
        assert_eq!(report.communities.moved_nodes[0].from_community, 0);
        assert_eq!(report.communities.moved_nodes[0].to_community, 1);
        assert_eq!(report.communities.stable_count, 2); // a and c stayed
    }

    #[test]
    fn diff_empty_before_snapshot() {
        let before = make_snapshot(vec![], vec![], vec![], 0);
        let after = make_snapshot(
            vec![node("a", 0.5, 2, 1, 0)],
            vec![CommunitySnapshot { id: 0, members: vec!["a".into()] }],
            vec![],
            2,
        );
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.summary_delta.nodes.change, 1);
        assert_eq!(report.edges.added_nodes, vec!["a"]);
        assert!(report.edges.removed_nodes.is_empty());
    }

    #[test]
    fn diff_empty_after_snapshot() {
        let before = make_snapshot(
            vec![node("a", 0.5, 2, 1, 0)],
            vec![CommunitySnapshot { id: 0, members: vec!["a".into()] }],
            vec![],
            2,
        );
        let after = make_snapshot(vec![], vec![], vec![], 0);
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.summary_delta.nodes.change, -1);
        assert!(report.edges.added_nodes.is_empty());
        assert_eq!(report.edges.removed_nodes, vec!["a"]);
    }
```

- [ ] **Step 2: Run all diff tests**

Run: `cargo test -p graphify-core diff`
Expected: 12 tests pass (2 deserialization + 3 from Task 2 + 7 new).

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-core/src/diff.rs
git commit -m "test(core): comprehensive tests for compute_diff — cycles, hotspots, communities (FEAT-002)"
```

---

### Task 4: Diff JSON output writer

**Files:**
- Create: `crates/graphify-report/src/diff_json.rs`
- Modify: `crates/graphify-report/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/graphify-report/src/diff_json.rs`:

```rust
use std::path::Path;

use graphify_core::diff::DiffReport;

/// Writes the diff report as pretty-printed JSON to `path`.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_diff_json(report: &DiffReport, path: &Path) {
    let json = serde_json::to_string_pretty(report).expect("serialize diff JSON");
    std::fs::write(path, json).expect("write diff JSON");
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::diff::*;

    fn empty_report() -> DiffReport {
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta { before: 5, after: 5, change: 0 },
                edges: Delta { before: 10, after: 10, change: 0 },
                communities: Delta { before: 2, after: 2, change: 0 },
                cycles: Delta { before: 0, after: 0, change: 0 },
            },
            edges: EdgeDiff {
                added_nodes: vec![],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![],
                resolved: vec![],
            },
            hotspots: HotspotDiff {
                rising: vec![],
                falling: vec![],
                new_hotspots: vec![],
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: vec![],
                stable_count: 5,
            },
        }
    }

    #[test]
    fn write_diff_json_creates_valid_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drift-report.json");
        let report = empty_report();
        write_diff_json(&report, &path);

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["summary_delta"]["nodes"]["change"], 0);
        assert_eq!(value["communities"]["stable_count"], 5);
    }

    #[test]
    fn write_diff_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drift-report.json");
        let report = empty_report();
        write_diff_json(&report, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        // Verify it parses back to valid JSON with expected structure.
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(value["summary_delta"].is_object());
        assert!(value["edges"].is_object());
        assert!(value["cycles"].is_object());
        assert!(value["hotspots"].is_object());
        assert!(value["communities"].is_object());
    }
}
```

- [ ] **Step 2: Register in lib.rs**

Modify `crates/graphify-report/src/lib.rs` — add the module and re-export:

```rust
pub mod csv;
pub mod diff_json;
pub mod diff_markdown;
pub mod graphml;
pub mod html;
pub mod json;
pub mod markdown;
pub mod neo4j;
pub mod obsidian;

// Re-export the main write functions for convenience.
pub use csv::{write_edges_csv, write_nodes_csv};
pub use diff_json::write_diff_json;
pub use diff_markdown::write_diff_markdown;
pub use graphml::write_graphml;
pub use html::write_html;
pub use json::{write_analysis_json, write_graph_json};
pub use markdown::write_report;
pub use neo4j::write_cypher;
pub use obsidian::write_obsidian_vault;

// Re-export core types used across the report modules.
pub use graphify_core::community::Community;

/// A cycle represented as an ordered list of node IDs.
pub type Cycle = Vec<String>;
```

Note: `diff_markdown` module doesn't exist yet — create a stub so this compiles. Create `crates/graphify-report/src/diff_markdown.rs` with just:

```rust
use std::path::Path;

use graphify_core::diff::DiffReport;

/// Writes the diff report as a human-readable Markdown file to `path`.
///
/// # Panics
/// Panics if file I/O fails.
pub fn write_diff_markdown(_report: &DiffReport, _path: &Path) {
    todo!("implemented in Task 5")
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-report diff_json`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-report/src/diff_json.rs crates/graphify-report/src/diff_markdown.rs crates/graphify-report/src/lib.rs
git commit -m "feat(report): write_diff_json — drift report JSON output (FEAT-002)"
```

---

### Task 5: Diff Markdown output writer

**Files:**
- Modify: `crates/graphify-report/src/diff_markdown.rs` (replace stub)

- [ ] **Step 1: Write the failing test**

Replace the stub in `crates/graphify-report/src/diff_markdown.rs` with:

```rust
use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::diff::DiffReport;

/// Writes the diff report as a human-readable Markdown file to `path`.
///
/// # Panics
/// Panics if file I/O fails.
pub fn write_diff_markdown(report: &DiffReport, path: &Path) {
    let buf = render_diff_markdown(report);
    std::fs::write(path, buf).expect("write diff markdown");
}

/// Renders the DiffReport as a Markdown string.
fn render_diff_markdown(report: &DiffReport) -> String {
    let mut buf = String::new();

    // Title
    writeln!(buf, "# Architectural Drift Report").unwrap();
    writeln!(buf).unwrap();

    // Summary table
    writeln!(buf, "## Summary").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Metric | Before | After | Change |").unwrap();
    writeln!(buf, "|--------|--------|-------|--------|").unwrap();
    write_summary_row(&mut buf, "Nodes", &report.summary_delta.nodes);
    write_summary_row(&mut buf, "Edges", &report.summary_delta.edges);
    write_summary_row(&mut buf, "Communities", &report.summary_delta.communities);
    write_summary_row(&mut buf, "Cycles", &report.summary_delta.cycles);
    writeln!(buf).unwrap();

    // New / Removed nodes
    write_node_list(&mut buf, "New Nodes", &report.edges.added_nodes);
    write_node_list(&mut buf, "Removed Nodes", &report.edges.removed_nodes);

    // Degree changes
    if !report.edges.degree_changes.is_empty() {
        writeln!(buf, "## Degree Changes ({})", report.edges.degree_changes.len()).unwrap();
        writeln!(buf).unwrap();
        writeln!(buf, "| Node | In (before→after) | Out (before→after) |").unwrap();
        writeln!(buf, "|------|-------------------|-------------------|").unwrap();
        for dc in &report.edges.degree_changes {
            writeln!(
                buf,
                "| `{}` | {}→{} ({:+}) | {}→{} ({:+}) |",
                dc.id,
                dc.in_degree.before, dc.in_degree.after, dc.in_degree.change,
                dc.out_degree.before, dc.out_degree.after, dc.out_degree.change,
            )
            .unwrap();
        }
        writeln!(buf).unwrap();
    }

    // Cycle changes
    writeln!(buf, "## Cycle Changes").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "### Introduced ({})", report.cycles.introduced.len()).unwrap();
    writeln!(buf).unwrap();
    if report.cycles.introduced.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for cycle in &report.cycles.introduced {
            writeln!(buf, "- {}", format_cycle(cycle)).unwrap();
        }
    }
    writeln!(buf).unwrap();
    writeln!(buf, "### Resolved ({})", report.cycles.resolved.len()).unwrap();
    writeln!(buf).unwrap();
    if report.cycles.resolved.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for cycle in &report.cycles.resolved {
            writeln!(buf, "- {}", format_cycle(cycle)).unwrap();
        }
    }
    writeln!(buf).unwrap();

    // Hotspot movement
    writeln!(buf, "## Hotspot Movement").unwrap();
    writeln!(buf).unwrap();
    write_score_table(&mut buf, "Rising", &report.hotspots.rising);
    write_score_table(&mut buf, "Falling", &report.hotspots.falling);
    write_score_list(&mut buf, "New in Top 20", &report.hotspots.new_hotspots, true);
    write_score_list(
        &mut buf,
        "Left Top 20",
        &report.hotspots.removed_hotspots,
        false,
    );

    // Community shifts
    writeln!(buf, "## Community Shifts").unwrap();
    writeln!(buf).unwrap();
    if report.communities.moved_nodes.is_empty() {
        writeln!(buf, "No community changes detected.").unwrap();
    } else {
        writeln!(
            buf,
            "- **{} nodes** moved communities",
            report.communities.moved_nodes.len()
        )
        .unwrap();
        for mv in &report.communities.moved_nodes {
            writeln!(
                buf,
                "  - `{}`: community {} → {}",
                mv.id, mv.from_community, mv.to_community
            )
            .unwrap();
        }
    }
    writeln!(
        buf,
        "- **{} nodes** stable",
        report.communities.stable_count
    )
    .unwrap();

    buf
}

fn write_summary_row(buf: &mut String, label: &str, delta: &graphify_core::diff::Delta<usize>) {
    let sign = if delta.change > 0 { "+" } else { "" };
    writeln!(
        buf,
        "| {} | {} | {} | {}{} |",
        label, delta.before, delta.after, sign, delta.change
    )
    .unwrap();
}

fn write_node_list(buf: &mut String, title: &str, nodes: &[String]) {
    writeln!(buf, "## {} ({})", title, nodes.len()).unwrap();
    writeln!(buf).unwrap();
    if nodes.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for n in nodes {
            writeln!(buf, "- `{}`", n).unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn write_score_table(
    buf: &mut String,
    title: &str,
    changes: &[graphify_core::diff::ScoreChange],
) {
    writeln!(buf, "### {}", title).unwrap();
    writeln!(buf).unwrap();
    if changes.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        writeln!(buf, "| Node | Before | After | Delta |").unwrap();
        writeln!(buf, "|------|--------|-------|-------|").unwrap();
        for sc in changes {
            writeln!(
                buf,
                "| `{}` | {:.3} | {:.3} | {:+.3} |",
                sc.id, sc.before, sc.after, sc.delta
            )
            .unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn write_score_list(
    buf: &mut String,
    title: &str,
    changes: &[graphify_core::diff::ScoreChange],
    show_after: bool,
) {
    writeln!(buf, "### {}", title).unwrap();
    writeln!(buf).unwrap();
    if changes.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for sc in changes {
            let score = if show_after { sc.after } else { sc.before };
            writeln!(buf, "- `{}` (score: {:.3})", sc.id, score).unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn format_cycle(cycle: &[String]) -> String {
    let parts: Vec<String> = cycle.iter().map(|id| format!("`{}`", id)).collect();
    parts.join(" → ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::diff::*;

    fn report_with_changes() -> DiffReport {
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta { before: 10, after: 12, change: 2 },
                edges: Delta { before: 20, after: 25, change: 5 },
                communities: Delta { before: 3, after: 4, change: 1 },
                cycles: Delta { before: 1, after: 0, change: -1 },
            },
            edges: EdgeDiff {
                added_nodes: vec!["app.new".into()],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![],
                resolved: vec![vec!["a".into(), "b".into()]],
            },
            hotspots: HotspotDiff {
                rising: vec![ScoreChange {
                    id: "app.hot".into(),
                    before: 0.3,
                    after: 0.6,
                    delta: 0.3,
                }],
                falling: vec![],
                new_hotspots: vec![],
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: vec![CommunityMove {
                    id: "app.moved".into(),
                    from_community: 0,
                    to_community: 2,
                }],
                stable_count: 9,
            },
        }
    }

    #[test]
    fn markdown_contains_expected_sections() {
        let md = render_diff_markdown(&report_with_changes());
        assert!(md.contains("# Architectural Drift Report"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("## New Nodes (1)"));
        assert!(md.contains("## Removed Nodes (0)"));
        assert!(md.contains("## Cycle Changes"));
        assert!(md.contains("### Resolved (1)"));
        assert!(md.contains("## Hotspot Movement"));
        assert!(md.contains("## Community Shifts"));
    }

    #[test]
    fn markdown_summary_table_has_correct_values() {
        let md = render_diff_markdown(&report_with_changes());
        assert!(md.contains("| Nodes | 10 | 12 | +2 |"));
        assert!(md.contains("| Cycles | 1 | 0 | -1 |"));
    }

    #[test]
    fn write_diff_markdown_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drift-report.md");
        write_diff_markdown(&report_with_changes(), &path);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Architectural Drift Report"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p graphify-report diff_markdown`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-report/src/diff_markdown.rs
git commit -m "feat(report): write_diff_markdown — drift report Markdown output (FEAT-002)"
```

---

### Task 6: CLI Commands::Diff variant + handler

**Files:**
- Modify: `crates/graphify-cli/src/main.rs:72-253` (Commands enum)
- Modify: `crates/graphify-cli/src/main.rs:587-591` (match arm)

- [ ] **Step 1: Add Commands::Diff variant**

In `crates/graphify-cli/src/main.rs`, add a new variant to the `Commands` enum, after `Watch` and before the closing `}` at line 253:

```rust
    /// Compare two analysis snapshots to detect architectural drift
    Diff {
        /// Path to the "before" analysis.json (file-vs-file mode)
        #[arg(long)]
        before: Option<PathBuf>,

        /// Path to the "after" analysis.json (file-vs-file mode)
        #[arg(long)]
        after: Option<PathBuf>,

        /// Path to a baseline analysis.json (baseline-vs-live mode)
        #[arg(long)]
        baseline: Option<PathBuf>,

        /// Path to graphify.toml (for live extraction in baseline mode)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Project name (for baseline mode with multi-project configs)
        #[arg(long)]
        project: Option<String>,

        /// Output directory for drift report files (default: current directory)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Minimum score delta to report as significant (default: 0.05)
        #[arg(long, default_value = "0.05")]
        threshold: f64,
    },
```

- [ ] **Step 2: Add the import for diff types**

Add to the `use graphify_core::` import block at the top of `main.rs`:

```rust
use graphify_core::diff::{AnalysisSnapshot, compute_diff};
```

Add to the `use graphify_report::` import block:

```rust
use graphify_report::{
    write_analysis_json, write_cypher, write_diff_json, write_diff_markdown, write_edges_csv,
    write_graph_json, write_graphml, write_html, write_nodes_csv, write_obsidian_vault,
    write_report, Cycle,
};
```

- [ ] **Step 3: Add the match arm and handler**

In the `match cli.command` block, after the `Commands::Watch` arm (around line 587-589), add:

```rust
        Commands::Diff {
            before,
            after,
            baseline,
            config,
            project,
            output,
            threshold,
        } => {
            cmd_diff(
                before.as_deref(),
                after.as_deref(),
                baseline.as_deref(),
                config.as_deref(),
                project.as_deref(),
                output.as_deref(),
                threshold,
            );
        }
```

Then add the `cmd_diff` function before the `// Config loading helpers` section:

```rust
// ---------------------------------------------------------------------------
// diff command
// ---------------------------------------------------------------------------

fn cmd_diff(
    before: Option<&Path>,
    after: Option<&Path>,
    baseline: Option<&Path>,
    config: Option<&Path>,
    project: Option<&str>,
    output: Option<&Path>,
    threshold: f64,
) {
    let (before_snapshot, after_snapshot) = match (before, after, baseline, config) {
        // File-vs-file mode
        (Some(before_path), Some(after_path), None, None) => {
            let b = load_snapshot(before_path);
            let a = load_snapshot(after_path);
            (b, a)
        }
        // Baseline-vs-live mode
        (None, None, Some(baseline_path), Some(config_path)) => {
            let b = load_snapshot(baseline_path);
            let cfg = load_config(config_path);
            let projects = filter_projects(&cfg, project);
            let project_cfg = projects[0];
            let w = resolve_weights(&cfg, None);
            let (graph, _, _stats) = run_extract(project_cfg, &cfg.settings, None, false);
            let (mut metrics, communities, cycles_simple) = run_analyze(&graph, &w);
            assign_community_ids(&mut metrics, &communities);
            // Build an AnalysisSnapshot from live data.
            let total_nodes = metrics.len();
            let total_edges = graph.edge_count();
            let total_communities = communities.len();
            let total_cycles = cycles_simple.len();
            let a = AnalysisSnapshot {
                nodes: metrics
                    .iter()
                    .map(|m| graphify_core::diff::NodeSnapshot {
                        id: m.id.clone(),
                        betweenness: m.betweenness,
                        pagerank: m.pagerank,
                        in_degree: m.in_degree,
                        out_degree: m.out_degree,
                        in_cycle: m.in_cycle,
                        score: m.score,
                        community_id: m.community_id,
                    })
                    .collect(),
                communities: communities
                    .iter()
                    .map(|c| graphify_core::diff::CommunitySnapshot {
                        id: c.id,
                        members: c.members.clone(),
                    })
                    .collect(),
                cycles: cycles_simple,
                summary: graphify_core::diff::SummarySnapshot {
                    total_nodes,
                    total_edges,
                    total_communities,
                    total_cycles,
                },
            };
            (b, a)
        }
        _ => {
            eprintln!(
                "Error: use either --before + --after (file mode) or --baseline + --config (live mode)"
            );
            std::process::exit(1);
        }
    };

    let report = compute_diff(&before_snapshot, &after_snapshot, threshold);

    let out_dir = output.unwrap_or(Path::new("."));
    std::fs::create_dir_all(out_dir).expect("create output directory");

    write_diff_json(&report, &out_dir.join("drift-report.json"));
    write_diff_markdown(&report, &out_dir.join("drift-report.md"));

    // Print summary to stdout.
    println!("Architectural Drift Report");
    println!("  Nodes:       {} → {} ({:+})", report.summary_delta.nodes.before, report.summary_delta.nodes.after, report.summary_delta.nodes.change);
    println!("  Edges:       {} → {} ({:+})", report.summary_delta.edges.before, report.summary_delta.edges.after, report.summary_delta.edges.change);
    println!("  Communities: {} → {} ({:+})", report.summary_delta.communities.before, report.summary_delta.communities.after, report.summary_delta.communities.change);
    println!("  Cycles:      {} → {} ({:+})", report.summary_delta.cycles.before, report.summary_delta.cycles.after, report.summary_delta.cycles.change);
    if !report.edges.added_nodes.is_empty() {
        println!("  New nodes:   {}", report.edges.added_nodes.len());
    }
    if !report.edges.removed_nodes.is_empty() {
        println!("  Removed:     {}", report.edges.removed_nodes.len());
    }
    if !report.hotspots.rising.is_empty() || !report.hotspots.falling.is_empty() {
        println!(
            "  Hotspots:    {} rising, {} falling",
            report.hotspots.rising.len(),
            report.hotspots.falling.len()
        );
    }
    if !report.communities.moved_nodes.is_empty() {
        println!(
            "  Community:   {} moved, {} stable",
            report.communities.moved_nodes.len(),
            report.communities.stable_count
        );
    }
    println!("Written to {}", out_dir.display());
}

fn load_snapshot(path: &Path) -> AnalysisSnapshot {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Cannot read {:?}: {e}", path);
            std::process::exit(1);
        }
    };
    match serde_json::from_str::<AnalysisSnapshot>(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid analysis JSON {:?}: {e}", path);
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build -p graphify-cli`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat(cli): add graphify diff command — file-vs-file and baseline-vs-live modes (FEAT-002)"
```

---

### Task 7: Build, test full workspace, update docs

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass (251 existing + ~17 new ≈ 268 total).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Fix any issues from tests or clippy, re-run until clean**

Fix any issues found. Common ones:
- Unused imports (remove them)
- Missing `Clone` derives (add them)
- Clippy lints on `let` bindings or match arms

- [ ] **Step 4: Update CLAUDE.md**

Add to the "Running Graphify" section:

```bash
# Compare two analysis snapshots
graphify diff --before report/v1/analysis.json --after report/v2/analysis.json

# Compare baseline against live codebase
graphify diff --baseline report/baseline/analysis.json --config graphify.toml
```

Add to the "Key modules" table:

```
| `crates/graphify-core/src/diff.rs` | AnalysisSnapshot deserialization, DiffReport, compute_diff() |
| `crates/graphify-report/src/diff_json.rs` | Drift report JSON output |
| `crates/graphify-report/src/diff_markdown.rs` | Drift report Markdown output |
```

Add to the "Conventions" section:

```
- Diff operates on analysis.json snapshots (not CodeGraph directly) — decoupled from internal types
- Community equivalence mapping: max-overlap matching handles unstable community IDs across runs
- Hotspot threshold default: 0.05 (configurable via --threshold)
- Drift report output: drift-report.json + drift-report.md
```

- [ ] **Step 5: Update sprint board**

In `docs/TaskNotes/Tasks/sprint.md`, change FEAT-002 status from `open` to `done`.

Add to the Done section:
```
- [[FEAT-002-architectural-drift-detection]] - Implemented: `graphify diff` with file-vs-file and baseline-vs-live modes, 5-dimension drift detection, JSON + Markdown output, ~17 new tests (2026-04-13)
```

- [ ] **Step 6: Final commit**

```bash
git add CLAUDE.md docs/TaskNotes/Tasks/sprint.md
git commit -m "docs: update CLAUDE.md and sprint board for FEAT-002 drift detection"
```
