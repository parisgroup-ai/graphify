use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::consolidation::ConsolidationConfig;
use crate::metrics::HotspotType;

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
    /// Node IDs matched by the `[consolidation].allowlist` in `graphify.toml`,
    /// when a `[consolidation]` section is configured. Absent on legacy
    /// analysis.json files written before the consolidation allowlist landed.
    #[serde(default)]
    pub allowlisted_symbols: Option<Vec<String>>,
    /// Per-edge records for downstream smell scoring (FEAT-037). Empty vector
    /// on legacy snapshots written before `edges` was emitted.
    #[serde(default)]
    pub edges: Vec<EdgeSnapshot>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeSnapshot {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub confidence: f64,
    pub confidence_kind: String,
    pub source_community: usize,
    pub target_community: usize,
    pub in_cycle: bool,
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
    /// Hotspot classification — added in v0.7. Older snapshots without this
    /// field deserialize to `None`.
    #[serde(default)]
    pub hotspot_type: Option<HotspotType>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub summary_delta: SummaryDelta,
    pub edges: EdgeDiff,
    pub cycles: CycleDiff,
    pub hotspots: HotspotDiff,
    pub communities: CommunityDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryDelta {
    pub nodes: Delta<usize>,
    pub edges: Delta<usize>,
    pub communities: Delta<usize>,
    pub cycles: Delta<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta<T> {
    pub before: T,
    pub after: T,
    pub change: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDiff {
    pub added_nodes: Vec<String>,
    pub removed_nodes: Vec<String>,
    pub degree_changes: Vec<DegreeChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegreeChange {
    pub id: String,
    pub in_degree: Delta<usize>,
    pub out_degree: Delta<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleDiff {
    pub introduced: Vec<Vec<String>>,
    pub resolved: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotDiff {
    pub rising: Vec<ScoreChange>,
    pub falling: Vec<ScoreChange>,
    pub new_hotspots: Vec<ScoreChange>,
    pub removed_hotspots: Vec<ScoreChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreChange {
    pub id: String,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
    /// Name of the `[consolidation.intentional_mirrors]` group this node
    /// belongs to, when one is declared. Consumers can filter on this to
    /// collapse expected cross-project duplicates; absent when no mirror
    /// group is declared (preserves legacy JSON shape byte-for-byte).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intentional_mirror: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDiff {
    pub moved_nodes: Vec<CommunityMove>,
    pub stable_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Like [`compute_diff`] but annotates hotspot entries whose node id is
/// declared under `[consolidation.intentional_mirrors]` in `graphify.toml`.
///
/// Pass `None` (or an empty [`ConsolidationConfig`]) to get the exact
/// behaviour of [`compute_diff`]. When a non-empty config is supplied,
/// each [`ScoreChange`] in `rising`, `falling`, `new_hotspots`, and
/// `removed_hotspots` whose id appears in the mirror index gets its
/// [`ScoreChange::intentional_mirror`] field populated with the group
/// name.
pub fn compute_diff_with_config(
    before: &AnalysisSnapshot,
    after: &AnalysisSnapshot,
    score_threshold: f64,
    consolidation: Option<&ConsolidationConfig>,
) -> DiffReport {
    let mut report = compute_diff(before, after, score_threshold);
    if let Some(cfg) = consolidation {
        let index = build_mirror_index(cfg);
        if !index.is_empty() {
            annotate_hotspots(&mut report.hotspots, &index);
        }
    }
    report
}

/// Builds a `node_id -> mirror_group_name` index from a compiled
/// consolidation config. Endpoints are declared as `"<project>:<node_id>"`
/// strings; the `project:` prefix is stripped (if present) because drift
/// reports operate on a single project's analysis at a time — the project
/// disambiguator lives outside the diff surface.
fn build_mirror_index(cfg: &ConsolidationConfig) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for (group, endpoints) in cfg.intentional_mirrors() {
        for endpoint in endpoints {
            let node_id = endpoint
                .split_once(':')
                .map(|(_, id)| id)
                .unwrap_or(endpoint.as_str());
            index.insert(node_id.to_string(), group.clone());
        }
    }
    index
}

fn annotate_hotspots(hotspots: &mut HotspotDiff, index: &HashMap<String, String>) {
    annotate_bucket(&mut hotspots.rising, index);
    annotate_bucket(&mut hotspots.falling, index);
    annotate_bucket(&mut hotspots.new_hotspots, index);
    annotate_bucket(&mut hotspots.removed_hotspots, index);
}

fn annotate_bucket(bucket: &mut [ScoreChange], index: &HashMap<String, String>) {
    for sc in bucket {
        if let Some(group) = index.get(&sc.id) {
            sc.intentional_mirror = Some(group.clone());
        }
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
                    intentional_mirror: None,
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
                intentional_mirror: None,
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
                intentional_mirror: None,
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
    fn deserialize_analysis_snapshot_with_edges_populates_field() {
        // FEAT-037: AnalysisSnapshot carries per-edge records when present.
        let json = r#"{
            "nodes": [],
            "edges": [
                {
                    "source": "a",
                    "target": "b",
                    "kind": "Imports",
                    "confidence": 0.5,
                    "confidence_kind": "Ambiguous",
                    "source_community": 0,
                    "target_community": 1,
                    "in_cycle": true
                }
            ],
            "communities": [],
            "cycles": [],
            "summary": {
                "total_nodes": 0,
                "total_edges": 1,
                "total_communities": 0,
                "total_cycles": 0
            }
        }"#;
        let snapshot: AnalysisSnapshot = serde_json::from_str(json).expect("deserialize");
        assert_eq!(snapshot.edges.len(), 1);
        let e = &snapshot.edges[0];
        assert_eq!(e.source, "a");
        assert_eq!(e.target, "b");
        assert_eq!(e.kind, "Imports");
        assert!((e.confidence - 0.5).abs() < 1e-9);
        assert_eq!(e.confidence_kind, "Ambiguous");
        assert_eq!(e.source_community, 0);
        assert_eq!(e.target_community, 1);
        assert!(e.in_cycle);
    }

    #[test]
    fn deserialize_legacy_analysis_snapshot_without_edges_defaults_to_empty() {
        // FEAT-037 backward compat: legacy analysis.json files pre-FEAT-037 omit
        // the `edges` key entirely. Must deserialize to an empty Vec, not error.
        let snapshot: AnalysisSnapshot =
            serde_json::from_str(sample_analysis_json()).expect("deserialize");
        assert!(snapshot.edges.is_empty());
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
            allowlisted_symbols: None,
            edges: vec![],
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
            hotspot_type: None,
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

    fn mirror_config(group: &str, endpoints: &[&str]) -> ConsolidationConfig {
        use crate::consolidation::ConsolidationConfigRaw;
        let mut mirrors = HashMap::new();
        mirrors.insert(
            group.to_string(),
            endpoints.iter().map(|s| s.to_string()).collect(),
        );
        ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec![],
            intentional_mirrors: mirrors,
            ..Default::default()
        })
        .expect("mirror_config compiles")
    }

    fn rising_snapshots() -> (AnalysisSnapshot, AnalysisSnapshot) {
        let before = make_snapshot(
            vec![node("app.models.tokens.TokenUsage", 0.30, 5, 3, 0)],
            vec![],
            vec![],
            8,
        );
        let after = make_snapshot(
            vec![node("app.models.tokens.TokenUsage", 0.50, 5, 3, 0)],
            vec![],
            vec![],
            8,
        );
        (before, after)
    }

    #[test]
    fn compute_diff_with_none_config_matches_plain_compute_diff() {
        let (before, after) = rising_snapshots();
        let plain = compute_diff(&before, &after, 0.05);
        let annotated = compute_diff_with_config(&before, &after, 0.05, None);

        assert_eq!(plain.hotspots.rising.len(), 1);
        assert_eq!(annotated.hotspots.rising.len(), 1);
        assert_eq!(plain.hotspots.rising[0].id, annotated.hotspots.rising[0].id);
        assert_eq!(plain.hotspots.rising[0].intentional_mirror, None);
        assert_eq!(annotated.hotspots.rising[0].intentional_mirror, None);
    }

    #[test]
    fn compute_diff_with_empty_config_leaves_entries_unannotated() {
        let (before, after) = rising_snapshots();
        let cfg = ConsolidationConfig::default();
        let report = compute_diff_with_config(&before, &after, 0.05, Some(&cfg));
        assert_eq!(report.hotspots.rising[0].intentional_mirror, None);
    }

    #[test]
    fn compute_diff_with_config_annotates_declared_mirror() {
        let (before, after) = rising_snapshots();
        let cfg = mirror_config(
            "TokenUsage",
            &[
                "ana-service:app.models.tokens.TokenUsage",
                "pkg-types:src.tokens.TokenUsage",
            ],
        );
        let report = compute_diff_with_config(&before, &after, 0.05, Some(&cfg));
        assert_eq!(
            report.hotspots.rising[0].intentional_mirror,
            Some("TokenUsage".to_string())
        );
    }

    #[test]
    fn compute_diff_with_config_leaves_non_mirror_leaves_alone() {
        let (before, after) = rising_snapshots();
        // Declares the same leaf-name `TokenUsage` but a DIFFERENT node id —
        // the annotation must not bleed onto the unrelated node.
        let cfg = mirror_config("TokenUsage", &["other-service:app.other.TokenUsage"]);
        let report = compute_diff_with_config(&before, &after, 0.05, Some(&cfg));
        assert_eq!(report.hotspots.rising[0].intentional_mirror, None);
    }

    #[test]
    fn compute_diff_with_config_accepts_bare_node_ids_without_project_prefix() {
        let (before, after) = rising_snapshots();
        let cfg = mirror_config("TokenUsage", &["app.models.tokens.TokenUsage"]);
        let report = compute_diff_with_config(&before, &after, 0.05, Some(&cfg));
        assert_eq!(
            report.hotspots.rising[0].intentional_mirror,
            Some("TokenUsage".to_string())
        );
    }

    #[test]
    fn score_change_json_omits_mirror_field_when_none() {
        let sc = ScoreChange {
            id: "x".into(),
            before: 0.1,
            after: 0.2,
            delta: 0.1,
            intentional_mirror: None,
        };
        let json = serde_json::to_string(&sc).unwrap();
        assert!(
            !json.contains("intentional_mirror"),
            "legacy JSON shape must omit the field when None, got: {json}"
        );
    }

    #[test]
    fn score_change_json_includes_mirror_field_when_some() {
        let sc = ScoreChange {
            id: "x".into(),
            before: 0.1,
            after: 0.2,
            delta: 0.1,
            intentional_mirror: Some("TokenUsage".into()),
        };
        let json = serde_json::to_string(&sc).unwrap();
        assert!(json.contains("\"intentional_mirror\":\"TokenUsage\""));
    }

    #[test]
    fn diff_report_roundtrips_json() {
        use crate::diff::{
            CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta,
        };

        let report = DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 10,
                    after: 12,
                    change: 2,
                },
                edges: Delta {
                    before: 20,
                    after: 25,
                    change: 5,
                },
                communities: Delta {
                    before: 3,
                    after: 3,
                    change: 0,
                },
                cycles: Delta {
                    before: 0,
                    after: 1,
                    change: 1,
                },
            },
            edges: EdgeDiff {
                added_nodes: vec!["a".into()],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![vec!["a".into(), "b".into()]],
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
                stable_count: 3,
            },
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let back: DiffReport = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.summary_delta.nodes.change, 2);
        assert_eq!(back.cycles.introduced.len(), 1);
        assert_eq!(back.edges.added_nodes, vec!["a".to_string()]);
    }
}
