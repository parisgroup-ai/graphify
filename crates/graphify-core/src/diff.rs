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
