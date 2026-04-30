//! Architectural smell edge scoring (FEAT-037).
//!
//! Pure function over [`AnalysisSnapshot`] (plus optional [`DiffReport`]) that
//! promotes the handful of edges most worth discussing in a PR review. See the
//! FEAT-037 task body for the scoring rationale.

use graphify_core::diff::{AnalysisSnapshot, DiffReport, EdgeSnapshot, NodeSnapshot};
use std::collections::{HashMap, HashSet};

/// An edge flagged by the smell detector, with the specific bonuses that
/// contributed to its score.
#[derive(Debug, Clone)]
pub struct SmellEdge {
    pub source: String,
    pub target: String,
    pub edge_kind: String,
    pub confidence: f64,
    pub confidence_kind: String,
    pub score: u32,
    pub reasons: Vec<String>,
}

/// Minimum score required to appear in the output. Filters out trivial
/// single-attribute edges (e.g. plain Extracted with no other signal).
const MIN_SCORE_FLOOR: u32 = 3;

/// Number of top-scoring nodes considered "hotspots" for the adjacency bonus.
const HOTSPOT_LOOKUP_SIZE: usize = 10;

/// Scores every edge in `analysis` and returns the top `top_n` entries above
/// the floor. Deterministic: ties are broken by drift-new first (when `drift`
/// is supplied) then lexicographically on `src, tgt`.
///
/// Returns an empty vector when `top_n == 0`, when the snapshot carries no
/// edges (legacy analysis.json or empty project), or when nothing scores above
/// the floor.
pub fn score_smells(
    analysis: &AnalysisSnapshot,
    drift: Option<&DiffReport>,
    top_n: usize,
) -> Vec<SmellEdge> {
    if top_n == 0 || analysis.edges.is_empty() {
        return Vec::new();
    }

    let top_hotspots = top_hotspot_ids(&analysis.nodes, HOTSPOT_LOOKUP_SIZE);
    let degree_map = build_degree_map(&analysis.nodes);
    let drift_new: HashSet<&str> = drift
        .map(|d| d.edges.added_nodes.iter().map(String::as_str).collect())
        .unwrap_or_default();

    let mut scored: Vec<SmellEdge> = analysis
        .edges
        .iter()
        .map(|e| score_edge(e, &top_hotspots, &degree_map))
        .filter(|s| s.score >= MIN_SCORE_FLOOR)
        .collect();

    scored.sort_by(|a, b| {
        let b_drift_new =
            drift_new.contains(b.source.as_str()) || drift_new.contains(b.target.as_str());
        let a_drift_new =
            drift_new.contains(a.source.as_str()) || drift_new.contains(a.target.as_str());
        b.score
            .cmp(&a.score)
            .then_with(|| b_drift_new.cmp(&a_drift_new))
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
    });

    scored.truncate(top_n);
    scored
}

fn score_edge(
    edge: &EdgeSnapshot,
    top_hotspots: &HashSet<&str>,
    degree_map: &HashMap<&str, usize>,
) -> SmellEdge {
    let mut score: u32 = 0;
    let mut reasons: Vec<String> = Vec::new();

    let conf_bonus = confidence_bonus(&edge.confidence_kind);
    if conf_bonus > 0 {
        score += conf_bonus;
        reasons.push(format!("low-confidence ({})", edge.confidence_kind));
    }

    if edge.source_community != edge.target_community {
        score += 2;
        reasons.push("cross-community".to_string());
    }

    if edge.in_cycle {
        score += 2;
        reasons.push("in-cycle".to_string());
    }

    let src_deg = degree_map.get(edge.source.as_str()).copied().unwrap_or(0);
    let tgt_deg = degree_map.get(edge.target.as_str()).copied().unwrap_or(0);
    if is_peripheral_to_hub(src_deg, tgt_deg) {
        score += 1;
        reasons.push("peripheral\u{2192}hub".to_string());
    }

    let src_hot = top_hotspots.contains(edge.source.as_str());
    let tgt_hot = top_hotspots.contains(edge.target.as_str());
    if src_hot || tgt_hot {
        score += 1;
        // Prefer target — the typical "leaf coupling INTO a hub" shape.
        let hot_id = if tgt_hot { &edge.target } else { &edge.source };
        reasons.push(format!("touches hotspot `{}`", hot_id));
    }

    SmellEdge {
        source: edge.source.clone(),
        target: edge.target.clone(),
        edge_kind: edge.kind.clone(),
        confidence: edge.confidence,
        confidence_kind: edge.confidence_kind.clone(),
        score,
        reasons,
    }
}

/// Maps a ConfidenceKind string to its score bonus. Unknown variants score 0
/// defensively — future enum additions shouldn't crash old scorers.
fn confidence_bonus(kind: &str) -> u32 {
    match kind {
        "Ambiguous" => 3,
        "Inferred" => 2,
        "Extracted" => 1,
        "ExpectedExternal" => 0,
        _ => 0,
    }
}

/// An edge is "peripheral→hub" when one endpoint has total-degree ≤ 2 and the
/// other has total-degree ≥ 5 — a classic "leaf coupling into a hub" smell.
fn is_peripheral_to_hub(src_deg: usize, tgt_deg: usize) -> bool {
    let min_deg = src_deg.min(tgt_deg);
    let max_deg = src_deg.max(tgt_deg);
    min_deg <= 2 && max_deg >= 5
}

fn top_hotspot_ids(nodes: &[NodeSnapshot], n: usize) -> HashSet<&str> {
    let mut sorted: Vec<&NodeSnapshot> = nodes.iter().collect();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.iter().take(n).map(|n| n.id.as_str()).collect()
}

fn build_degree_map(nodes: &[NodeSnapshot]) -> HashMap<&str, usize> {
    nodes
        .iter()
        .map(|n| (n.id.as_str(), n.in_degree + n.out_degree))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::diff::{
        CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta,
    };

    fn node(id: &str, score: f64, in_deg: usize, out_deg: usize, community: usize) -> NodeSnapshot {
        NodeSnapshot {
            id: id.into(),
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

    fn edge(
        src: &str,
        tgt: &str,
        confidence_kind: &str,
        src_comm: usize,
        tgt_comm: usize,
        in_cycle: bool,
    ) -> EdgeSnapshot {
        EdgeSnapshot {
            source: src.into(),
            target: tgt.into(),
            kind: "Imports".into(),
            confidence: 0.5,
            confidence_kind: confidence_kind.into(),
            source_community: src_comm,
            target_community: tgt_comm,
            in_cycle,
        }
    }

    fn snapshot(nodes: Vec<NodeSnapshot>, edges: Vec<EdgeSnapshot>) -> AnalysisSnapshot {
        use graphify_core::diff::SummarySnapshot;
        AnalysisSnapshot {
            generated_at: None,
            summary: SummarySnapshot {
                total_nodes: nodes.len(),
                total_edges: edges.len(),
                total_communities: 0,
                total_cycles: 0,
            },
            nodes,
            edges,
            communities: vec![],
            cycles: vec![],
            allowlisted_symbols: None,
        }
    }

    #[test]
    fn score_includes_confidence_bonus() {
        // Ambiguous (3) + cross (2) + hotspot-adj (1, both nodes land in top-10
        // on a tiny graph) = 6. Primary check: the low-confidence reason fired.
        let s = snapshot(
            vec![node("a", 0.1, 1, 1, 0), node("b", 0.1, 1, 1, 1)],
            vec![edge("a", "b", "Ambiguous", 0, 1, false)],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 1);
        assert!(out[0]
            .reasons
            .iter()
            .any(|r| r.contains("low-confidence (Ambiguous)")));
        // Score ≥ confidence bonus alone (3) since other signals may also fire.
        assert!(out[0].score >= 3);
    }

    #[test]
    fn score_flags_cross_community_edge() {
        // Cross-community alone (Extracted) = 1 + 2 = 3 → at floor.
        let s = snapshot(
            vec![node("a", 0.1, 1, 1, 0), node("b", 0.1, 1, 1, 1)],
            vec![edge("a", "b", "Extracted", 0, 1, false)],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 1);
        assert!(out[0].reasons.iter().any(|r| r == "cross-community"));
    }

    #[test]
    fn score_flags_in_cycle_edge() {
        // Inferred (2) + in-cycle (2) + hotspot-adj (1, tiny-graph implicit) = 5.
        // Primary check: the in-cycle reason fired.
        let s = snapshot(
            vec![node("a", 0.1, 1, 1, 0), node("b", 0.1, 1, 1, 0)],
            vec![edge("a", "b", "Inferred", 0, 0, true)],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 1);
        assert!(out[0].reasons.iter().any(|r| r == "in-cycle"));
        assert!(
            out[0].score >= 4,
            "at least conf+cycle = 4, got {}",
            out[0].score
        );
    }

    #[test]
    fn score_flags_peripheral_to_hub() {
        // src degree 1 (leaf), tgt degree 6 (hub). Extracted = 1. Same community,
        // not in cycle. Not a hotspot. 1 (extracted) + 1 (peripheral) = 2 → BELOW floor.
        // Promote by adding cross-community to land at floor: 1 + 2 + 1 = 4.
        let s = snapshot(
            vec![node("a", 0.1, 1, 0, 0), node("b", 0.1, 3, 3, 1)],
            vec![edge("a", "b", "Extracted", 0, 1, false)],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 1);
        assert!(out[0].reasons.iter().any(|r| r.contains("peripheral")));
    }

    #[test]
    fn score_flags_hotspot_adjacent() {
        // Extracted (1) + cross-community (2) + peripheral->hub (1, a has
        // degree 2, hub has degree 10) + hotspot-adj (1, hub is top score) = 5.
        // Primary check: reasons name the specific hotspot.
        let s = snapshot(
            vec![
                node("a", 0.01, 1, 1, 0),
                node("hub", 0.90, 5, 5, 1),
                node("c", 0.02, 1, 1, 0),
            ],
            vec![edge("a", "hub", "Extracted", 0, 1, false)],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 1);
        assert!(out[0]
            .reasons
            .iter()
            .any(|r| r.contains("touches hotspot `hub`")));
    }

    #[test]
    fn floor_filters_low_score_edges() {
        // Extracted + same-community + no-cycle + not peripheral + not hotspot = 1.
        // Must be filtered out by floor = 3.
        let s = snapshot(
            vec![node("a", 0.1, 1, 1, 0), node("b", 0.1, 1, 1, 0)],
            vec![edge("a", "b", "Extracted", 0, 0, false)],
        );
        let out = score_smells(&s, None, 5);
        assert!(out.is_empty());
    }

    #[test]
    fn top_n_zero_returns_empty() {
        let s = snapshot(
            vec![node("a", 0.1, 1, 1, 0), node("b", 0.1, 1, 1, 1)],
            vec![edge("a", "b", "Ambiguous", 0, 1, true)],
        );
        let out = score_smells(&s, None, 0);
        assert!(out.is_empty());
    }

    #[test]
    fn tie_break_lexicographic_when_no_drift() {
        // Two edges tie at score 5 (Ambiguous + cross). Expect lex order: "a→b" before "c→d".
        let s = snapshot(
            vec![
                node("a", 0.1, 1, 1, 0),
                node("b", 0.1, 1, 1, 1),
                node("c", 0.1, 1, 1, 0),
                node("d", 0.1, 1, 1, 1),
            ],
            vec![
                edge("c", "d", "Ambiguous", 0, 1, false),
                edge("a", "b", "Ambiguous", 0, 1, false),
            ],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, "a");
        assert_eq!(out[1].source, "c");
    }

    fn drift_with_added_nodes(added: Vec<&str>) -> DiffReport {
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                edges: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                communities: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                cycles: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
            },
            edges: EdgeDiff {
                added_nodes: added.into_iter().map(String::from).collect(),
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
                stable_count: 0,
            },
        }
    }

    #[test]
    fn tie_break_prefers_drift_new_edges_when_drift_present() {
        // Two edges tie at score 5. Drift reports `c` as a newly-added node.
        // The c→d edge should sort ABOVE a→b despite lex order favoring a→b.
        let s = snapshot(
            vec![
                node("a", 0.1, 1, 1, 0),
                node("b", 0.1, 1, 1, 1),
                node("c", 0.1, 1, 1, 0),
                node("d", 0.1, 1, 1, 1),
            ],
            vec![
                edge("a", "b", "Ambiguous", 0, 1, false),
                edge("c", "d", "Ambiguous", 0, 1, false),
            ],
        );
        let drift = drift_with_added_nodes(vec!["c"]);
        let out = score_smells(&s, Some(&drift), 5);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, "c", "drift-new should sort first");
        assert_eq!(out[1].source, "a");
    }

    #[test]
    fn reasons_collect_every_contributing_bonus() {
        // Ambiguous + cross + in_cycle + peripheral->hub + hotspot = max-score 9.
        let s = snapshot(
            vec![node("leaf", 0.01, 1, 0, 0), node("hub", 0.90, 3, 3, 1)],
            vec![edge("leaf", "hub", "Ambiguous", 0, 1, true)],
        );
        let out = score_smells(&s, None, 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].score, 9);
        assert_eq!(out[0].reasons.len(), 5);
    }

    #[test]
    fn empty_edges_returns_empty() {
        // Legacy / empty snapshot — no crash, no output.
        let s = snapshot(vec![node("a", 0.1, 0, 0, 0)], vec![]);
        let out = score_smells(&s, None, 5);
        assert!(out.is_empty());
    }
}
