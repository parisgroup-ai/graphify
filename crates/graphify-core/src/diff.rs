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
pub struct Delta<T> {
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

    rising.sort_by(|a, b| {
        b.delta
            .partial_cmp(&a.delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    falling.sort_by(|a, b| {
        a.delta
            .partial_cmp(&b.delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_n = 20;
    let before_top: Vec<&str> = {
        let mut sorted: Vec<&NodeSnapshot> = before.nodes.iter().collect();
        sorted.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.iter().take(top_n).map(|n| n.id.as_str()).collect()
    };
    let after_top: Vec<&str> = {
        let mut sorted: Vec<&NodeSnapshot> = after.nodes.iter().collect();
        sorted.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
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
    new_hotspots.sort_by(|a, b| {
        b.after
            .partial_cmp(&a.after)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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

fn compute_community_diff(before: &AnalysisSnapshot, after: &AnalysisSnapshot) -> CommunityDiff {
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

    let mut equiv: HashMap<usize, usize> = HashMap::new();
    for (&after_id, after_mems) in &after_members {
        let after_set: std::collections::HashSet<&str> = after_mems.iter().copied().collect();
        let mut best_id = after_id;
        let mut best_overlap = 0usize;
        for (&before_id, before_mems) in &before_members {
            let overlap = before_mems
                .iter()
                .filter(|m| after_set.contains(*m))
                .count();
            if overlap > best_overlap || (overlap == best_overlap && before_id == after_id) {
                best_overlap = overlap;
                best_id = before_id;
            }
        }
        equiv.insert(after_id, best_id);
    }

    let mut moved_nodes: Vec<CommunityMove> = Vec::new();
    let mut stable_count = 0usize;

    for (id, &before_cid) in &before_comm {
        if let Some(&after_cid) = after_comm.get(id) {
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
            vec![CommunitySnapshot {
                id: 0,
                members: vec!["x".into()],
            }],
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
            vec![CommunitySnapshot {
                id: 0,
                members: vec!["a".into(), "b".into()],
            }],
            vec![],
            3,
        );
        let after = make_snapshot(
            vec![node("a", 0.5, 2, 1, 0), node("c", 0.2, 0, 1, 0)],
            vec![CommunitySnapshot {
                id: 0,
                members: vec!["a".into(), "c".into()],
            }],
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
            vec![node("a", 0.80, 5, 3, 0), node("b", 0.20, 1, 1, 0)],
            vec![],
            vec![],
            6,
        );
        let after = make_snapshot(
            vec![node("a", 0.50, 5, 3, 0), node("b", 0.60, 1, 1, 0)],
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
                CommunitySnapshot {
                    id: 0,
                    members: vec!["a".into(), "b".into()],
                },
                CommunitySnapshot {
                    id: 1,
                    members: vec!["c".into()],
                },
            ],
            vec![],
            4,
        );
        let after = make_snapshot(
            vec![
                node("a", 0.5, 2, 1, 0),
                node("b", 0.3, 1, 1, 1),
                node("c", 0.2, 1, 0, 1),
            ],
            vec![
                CommunitySnapshot {
                    id: 0,
                    members: vec!["a".into()],
                },
                CommunitySnapshot {
                    id: 1,
                    members: vec!["b".into(), "c".into()],
                },
            ],
            vec![],
            4,
        );
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.communities.moved_nodes.len(), 1);
        assert_eq!(report.communities.moved_nodes[0].id, "b");
        assert_eq!(report.communities.moved_nodes[0].from_community, 0);
        assert_eq!(report.communities.moved_nodes[0].to_community, 1);
        assert_eq!(report.communities.stable_count, 2);
    }

    #[test]
    fn diff_empty_before_snapshot() {
        let before = make_snapshot(vec![], vec![], vec![], 0);
        let after = make_snapshot(
            vec![node("a", 0.5, 2, 1, 0)],
            vec![CommunitySnapshot {
                id: 0,
                members: vec!["a".into()],
            }],
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
            vec![CommunitySnapshot {
                id: 0,
                members: vec!["a".into()],
            }],
            vec![],
            2,
        );
        let after = make_snapshot(vec![], vec![], vec![], 0);
        let report = compute_diff(&before, &after, 0.05);
        assert_eq!(report.summary_delta.nodes.change, -1);
        assert!(report.edges.added_nodes.is_empty());
        assert_eq!(report.edges.removed_nodes, vec!["a"]);
    }
}
