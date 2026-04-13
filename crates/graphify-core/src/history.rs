use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::community::Community;
use crate::diff::{Delta, ScoreChange};
use crate::graph::CodeGraph;
use crate::metrics::NodeMetrics;
use crate::types::ConfidenceKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalSnapshot {
    pub captured_at: u128,
    pub project: String,
    pub summary: SummarySnapshot,
    pub top_hotspots: Vec<HotspotEntry>,
    pub confidence_summary: ConfidenceSummary,
    pub nodes: Vec<HistoricalNode>,
    pub communities: Vec<HistoricalCommunity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarySnapshot {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_communities: usize,
    pub total_cycles: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotEntry {
    pub id: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceSummary {
    pub extracted_count: usize,
    pub extracted_pct: f64,
    pub inferred_count: usize,
    pub inferred_pct: f64,
    pub ambiguous_count: usize,
    pub ambiguous_pct: f64,
    pub mean_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalNode {
    pub id: String,
    pub score: f64,
    pub community_id: usize,
    pub in_degree: usize,
    pub out_degree: usize,
    pub in_cycle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalCommunity {
    pub id: usize,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendReport {
    pub project: String,
    pub snapshot_count: usize,
    pub window: TrendWindow,
    pub points: Vec<TrendPoint>,
    pub intervals: Vec<TrendInterval>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendWindow {
    pub first_captured_at: u128,
    pub last_captured_at: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    pub captured_at: u128,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_communities: usize,
    pub total_cycles: usize,
    pub top_hotspots: Vec<HotspotEntry>,
    pub mean_confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendInterval {
    pub from_captured_at: u128,
    pub to_captured_at: u128,
    pub summary_delta: TrendSummaryDelta,
    pub hotspots: TrendHotspotDelta,
    pub communities: CommunityChurn,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendSummaryDelta {
    pub nodes: Delta<usize>,
    pub edges: Delta<usize>,
    pub communities: Delta<usize>,
    pub cycles: Delta<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendHotspotDelta {
    pub new_hotspots: Vec<HotspotEntry>,
    pub removed_hotspots: Vec<HotspotEntry>,
    pub rising: Vec<ScoreChange>,
    pub falling: Vec<ScoreChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunityChurn {
    pub moved_nodes: usize,
    pub stable_nodes: usize,
    pub churn_pct: f64,
}

pub fn build_historical_snapshot(
    project: &str,
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Vec<String>],
    captured_at: u128,
) -> HistoricalSnapshot {
    let total_communities = communities.len();

    let mut sorted_metrics: Vec<&NodeMetrics> = metrics.iter().collect();
    sorted_metrics.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_hotspots = sorted_metrics
        .iter()
        .take(20)
        .map(|metric| HotspotEntry {
            id: metric.id.clone(),
            score: metric.score,
        })
        .collect();

    let nodes = metrics
        .iter()
        .map(|metric| HistoricalNode {
            id: metric.id.clone(),
            score: metric.score,
            community_id: metric.community_id,
            in_degree: metric.in_degree,
            out_degree: metric.out_degree,
            in_cycle: metric.in_cycle,
        })
        .collect();

    let communities = communities
        .iter()
        .map(|community| HistoricalCommunity {
            id: community.id,
            members: community.members.clone(),
        })
        .collect();

    HistoricalSnapshot {
        captured_at,
        project: project.to_string(),
        summary: SummarySnapshot {
            total_nodes: metrics.len(),
            total_edges: graph.edge_count(),
            total_communities,
            total_cycles: cycles.len(),
        },
        top_hotspots,
        confidence_summary: build_confidence_summary(graph),
        nodes,
        communities,
    }
}

fn build_confidence_summary(graph: &CodeGraph) -> ConfidenceSummary {
    let all_edges = graph.edges();
    let total_edge_count = all_edges.len();
    let mut extracted = 0usize;
    let mut inferred = 0usize;
    let mut ambiguous = 0usize;
    let mut confidence_sum = 0.0f64;

    for (_, _, edge) in &all_edges {
        match edge.confidence_kind {
            ConfidenceKind::Extracted => extracted += 1,
            ConfidenceKind::Inferred => inferred += 1,
            ConfidenceKind::Ambiguous => ambiguous += 1,
        }
        confidence_sum += edge.confidence;
    }

    let pct = |count: usize| -> f64 {
        if total_edge_count > 0 {
            (count as f64 / total_edge_count as f64) * 100.0
        } else {
            0.0
        }
    };

    ConfidenceSummary {
        extracted_count: extracted,
        extracted_pct: pct(extracted),
        inferred_count: inferred,
        inferred_pct: pct(inferred),
        ambiguous_count: ambiguous,
        ambiguous_pct: pct(ambiguous),
        mean_confidence: if total_edge_count > 0 {
            confidence_sum / total_edge_count as f64
        } else {
            0.0
        },
    }
}

pub fn load_historical_snapshots(dir: &Path) -> Result<Vec<HistoricalSnapshot>, String> {
    let mut snapshots = Vec::new();
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("cannot read history directory {}: {err}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|err| format!("cannot read history entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let raw = fs::read_to_string(&path)
            .map_err(|err| format!("cannot read history snapshot {}: {err}", path.display()))?;
        let snapshot = serde_json::from_str::<HistoricalSnapshot>(&raw)
            .map_err(|err| format!("invalid history snapshot {}: {err}", path.display()))?;
        snapshots.push(snapshot);
    }

    snapshots.sort_by_key(|snapshot| snapshot.captured_at);
    Ok(snapshots)
}

pub fn compute_trend_report(
    project: &str,
    snapshots: &[HistoricalSnapshot],
    limit: Option<usize>,
) -> Result<TrendReport, String> {
    let snapshots = sort_and_limit_snapshots(snapshots, limit);
    if snapshots.is_empty() {
        return Err("no historical snapshots available".to_string());
    }

    let points = snapshots
        .iter()
        .map(|snapshot| TrendPoint {
            captured_at: snapshot.captured_at,
            total_nodes: snapshot.summary.total_nodes,
            total_edges: snapshot.summary.total_edges,
            total_communities: snapshot.summary.total_communities,
            total_cycles: snapshot.summary.total_cycles,
            top_hotspots: snapshot.top_hotspots.clone(),
            mean_confidence: snapshot.confidence_summary.mean_confidence,
        })
        .collect::<Vec<_>>();

    let intervals = snapshots
        .windows(2)
        .map(|pair| TrendInterval {
            from_captured_at: pair[0].captured_at,
            to_captured_at: pair[1].captured_at,
            summary_delta: compute_summary_delta(&pair[0], &pair[1]),
            hotspots: compute_hotspot_delta(&pair[0], &pair[1]),
            communities: compute_community_churn(&pair[0], &pair[1]),
        })
        .collect::<Vec<_>>();

    let window = TrendWindow {
        first_captured_at: snapshots
            .first()
            .map(|snapshot| snapshot.captured_at)
            .unwrap(),
        last_captured_at: snapshots
            .last()
            .map(|snapshot| snapshot.captured_at)
            .unwrap(),
    };

    Ok(TrendReport {
        project: project.to_string(),
        snapshot_count: snapshots.len(),
        window,
        points,
        intervals,
    })
}

fn sort_and_limit_snapshots(
    snapshots: &[HistoricalSnapshot],
    limit: Option<usize>,
) -> Vec<HistoricalSnapshot> {
    let mut sorted = snapshots.to_vec();
    sorted.sort_by_key(|snapshot| snapshot.captured_at);
    if let Some(limit) = limit {
        if sorted.len() > limit {
            sorted = sorted.split_off(sorted.len() - limit);
        }
    }
    sorted
}

fn compute_summary_delta(
    before: &HistoricalSnapshot,
    after: &HistoricalSnapshot,
) -> TrendSummaryDelta {
    TrendSummaryDelta {
        nodes: delta(before.summary.total_nodes, after.summary.total_nodes),
        edges: delta(before.summary.total_edges, after.summary.total_edges),
        communities: delta(
            before.summary.total_communities,
            after.summary.total_communities,
        ),
        cycles: delta(before.summary.total_cycles, after.summary.total_cycles),
    }
}

fn delta(before: usize, after: usize) -> Delta<usize> {
    Delta {
        before,
        after,
        change: after as i64 - before as i64,
    }
}

fn compute_hotspot_delta(
    before: &HistoricalSnapshot,
    after: &HistoricalSnapshot,
) -> TrendHotspotDelta {
    let before_map: HashMap<&str, f64> = before
        .top_hotspots
        .iter()
        .map(|entry| (entry.id.as_str(), entry.score))
        .collect();
    let after_map: HashMap<&str, f64> = after
        .top_hotspots
        .iter()
        .map(|entry| (entry.id.as_str(), entry.score))
        .collect();

    let mut new_hotspots = after
        .top_hotspots
        .iter()
        .filter(|entry| !before_map.contains_key(entry.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    new_hotspots.sort_by(|a, b| a.id.cmp(&b.id));

    let mut removed_hotspots = before
        .top_hotspots
        .iter()
        .filter(|entry| !after_map.contains_key(entry.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    removed_hotspots.sort_by(|a, b| a.id.cmp(&b.id));

    let mut rising = Vec::new();
    let mut falling = Vec::new();
    for (id, before_score) in &before_map {
        if let Some(after_score) = after_map.get(id) {
            let delta = *after_score - *before_score;
            if delta > 0.0 {
                rising.push(ScoreChange {
                    id: (*id).to_string(),
                    before: *before_score,
                    after: *after_score,
                    delta,
                });
            } else if delta < 0.0 {
                falling.push(ScoreChange {
                    id: (*id).to_string(),
                    before: *before_score,
                    after: *after_score,
                    delta,
                });
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

    TrendHotspotDelta {
        new_hotspots,
        removed_hotspots,
        rising,
        falling,
    }
}

fn compute_community_churn(
    before: &HistoricalSnapshot,
    after: &HistoricalSnapshot,
) -> CommunityChurn {
    let before_nodes: HashMap<&str, &HistoricalNode> = before
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let after_nodes: HashMap<&str, &HistoricalNode> = after
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let after_to_before =
        build_after_to_before_community_map(&before.communities, &after.communities);

    let mut moved_nodes = 0usize;
    let mut stable_nodes = 0usize;

    for (id, before_node) in &before_nodes {
        if let Some(after_node) = after_nodes.get(id) {
            let equivalent_before = after_to_before
                .get(&after_node.community_id)
                .copied()
                .unwrap_or(after_node.community_id);
            if before_node.community_id == equivalent_before {
                stable_nodes += 1;
            } else {
                moved_nodes += 1;
            }
        }
    }

    let total = moved_nodes + stable_nodes;
    let churn_pct = if total > 0 {
        (moved_nodes as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    CommunityChurn {
        moved_nodes,
        stable_nodes,
        churn_pct,
    }
}

fn build_after_to_before_community_map(
    before: &[HistoricalCommunity],
    after: &[HistoricalCommunity],
) -> HashMap<usize, usize> {
    let before_members: Vec<(usize, HashSet<&str>)> = before
        .iter()
        .map(|community| {
            (
                community.id,
                community
                    .members
                    .iter()
                    .map(|member| member.as_str())
                    .collect(),
            )
        })
        .collect();

    let mut map = HashMap::new();
    for after_community in after {
        let after_members: HashSet<&str> = after_community
            .members
            .iter()
            .map(|member| member.as_str())
            .collect();
        let best_match = before_members
            .iter()
            .max_by_key(|(_, members)| members.intersection(&after_members).count())
            .map(|(id, _)| *id);
        if let Some(before_id) = best_match {
            map.insert(after_community.id, before_id);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        captured_at: u128,
        nodes: Vec<HistoricalNode>,
        communities: Vec<HistoricalCommunity>,
    ) -> HistoricalSnapshot {
        HistoricalSnapshot {
            captured_at,
            project: "demo".into(),
            summary: SummarySnapshot {
                total_nodes: nodes.len(),
                total_edges: nodes.len(),
                total_communities: communities.len(),
                total_cycles: 0,
            },
            top_hotspots: nodes
                .iter()
                .map(|node| HotspotEntry {
                    id: node.id.clone(),
                    score: node.score,
                })
                .collect(),
            confidence_summary: ConfidenceSummary {
                extracted_count: 0,
                extracted_pct: 0.0,
                inferred_count: 0,
                inferred_pct: 0.0,
                ambiguous_count: 0,
                ambiguous_pct: 0.0,
                mean_confidence: 1.0,
            },
            nodes,
            communities,
        }
    }

    #[test]
    fn trend_report_requires_at_least_one_snapshot() {
        let err = compute_trend_report("demo", &[], None).unwrap_err();
        assert!(err.contains("no historical snapshots"));
    }

    #[test]
    fn community_churn_uses_overlap_mapping() {
        let before = snapshot(
            1,
            vec![
                HistoricalNode {
                    id: "app.alpha".into(),
                    score: 0.4,
                    community_id: 1,
                    in_degree: 1,
                    out_degree: 1,
                    in_cycle: false,
                },
                HistoricalNode {
                    id: "app.beta".into(),
                    score: 0.5,
                    community_id: 2,
                    in_degree: 1,
                    out_degree: 1,
                    in_cycle: false,
                },
            ],
            vec![
                HistoricalCommunity {
                    id: 1,
                    members: vec!["app.alpha".into()],
                },
                HistoricalCommunity {
                    id: 2,
                    members: vec!["app.beta".into()],
                },
            ],
        );
        let after = snapshot(
            2,
            vec![
                HistoricalNode {
                    id: "app.alpha".into(),
                    score: 0.45,
                    community_id: 8,
                    in_degree: 1,
                    out_degree: 1,
                    in_cycle: false,
                },
                HistoricalNode {
                    id: "app.beta".into(),
                    score: 0.55,
                    community_id: 9,
                    in_degree: 1,
                    out_degree: 1,
                    in_cycle: false,
                },
            ],
            vec![
                HistoricalCommunity {
                    id: 8,
                    members: vec!["app.alpha".into()],
                },
                HistoricalCommunity {
                    id: 9,
                    members: vec!["app.beta".into()],
                },
            ],
        );

        let churn = compute_community_churn(&before, &after);
        assert_eq!(churn.moved_nodes, 0);
        assert_eq!(churn.stable_nodes, 2);
    }
}
