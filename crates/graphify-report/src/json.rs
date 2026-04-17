use std::path::Path;

use serde::Serialize;

use graphify_core::{
    graph::CodeGraph,
    metrics::{HotspotType, NodeMetrics},
};

use crate::{Community, Cycle};

// ---------------------------------------------------------------------------
// Serialization helpers — graph JSON (node_link_data format)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct NodeRecord<'a> {
    id: &'a str,
    kind: String,
    file_path: String,
    language: String,
    line: usize,
    is_local: bool,
}

#[derive(Serialize)]
struct LinkRecord<'a> {
    source: &'a str,
    target: &'a str,
    kind: String,
    weight: u32,
    line: usize,
    confidence: f64,
    confidence_kind: String,
}

#[derive(Serialize)]
struct GraphJson<'a> {
    directed: bool,
    multigraph: bool,
    nodes: Vec<NodeRecord<'a>>,
    links: Vec<LinkRecord<'a>>,
}

/// Writes the graph in node_link_data format to `path`.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_graph_json(graph: &CodeGraph, path: &Path) {
    let nodes: Vec<NodeRecord<'_>> = graph
        .nodes()
        .into_iter()
        .map(|n| NodeRecord {
            id: n.id.as_str(),
            kind: format!("{:?}", n.kind),
            file_path: n.file_path.to_string_lossy().into_owned(),
            language: format!("{:?}", n.language),
            line: n.line,
            is_local: n.is_local,
        })
        .collect();

    let links: Vec<LinkRecord<'_>> = graph
        .edges()
        .into_iter()
        .map(|(src, tgt, edge)| LinkRecord {
            source: src,
            target: tgt,
            kind: format!("{:?}", edge.kind),
            weight: edge.weight,
            line: edge.line,
            confidence: edge.confidence,
            confidence_kind: format!("{:?}", edge.confidence_kind),
        })
        .collect();

    let payload = GraphJson {
        directed: true,
        multigraph: false,
        nodes,
        links,
    };

    let json = serde_json::to_string_pretty(&payload).expect("serialize graph JSON");
    std::fs::write(path, json).expect("write graph JSON");
}

// ---------------------------------------------------------------------------
// Serialization helpers — analysis JSON
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MetricsRecord<'a> {
    id: &'a str,
    betweenness: f64,
    pagerank: f64,
    in_degree: usize,
    out_degree: usize,
    in_cycle: bool,
    score: f64,
    community_id: usize,
    hotspot_type: HotspotType,
}

#[derive(Serialize)]
struct CommunityRecord<'a> {
    id: usize,
    members: &'a [String],
}

#[derive(Serialize)]
struct Summary {
    total_nodes: usize,
    total_edges: usize,
    total_communities: usize,
    total_cycles: usize,
    /// Top hotspots as (id, score) pairs, sorted descending by score.
    top_hotspots: Vec<(String, f64)>,
}

#[derive(Serialize)]
struct ConfidenceSummary {
    extracted_count: usize,
    extracted_pct: f64,
    inferred_count: usize,
    inferred_pct: f64,
    ambiguous_count: usize,
    ambiguous_pct: f64,
    expected_external_count: usize,
    expected_external_pct: f64,
    mean_confidence: f64,
}

#[derive(Serialize)]
struct AnalysisJson<'a> {
    nodes: Vec<MetricsRecord<'a>>,
    communities: Vec<CommunityRecord<'a>>,
    cycles: &'a [Cycle],
    summary: Summary,
    confidence_summary: ConfidenceSummary,
}

/// Writes the analysis results to `path` in JSON format.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_analysis_json(
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    graph: &CodeGraph,
    path: &Path,
) {
    let nodes: Vec<MetricsRecord<'_>> = metrics
        .iter()
        .map(|m| MetricsRecord {
            id: m.id.as_str(),
            betweenness: m.betweenness,
            pagerank: m.pagerank,
            in_degree: m.in_degree,
            out_degree: m.out_degree,
            in_cycle: m.in_cycle,
            score: m.score,
            community_id: m.community_id,
            hotspot_type: m.hotspot_type,
        })
        .collect();

    let communities_rec: Vec<CommunityRecord<'_>> = communities
        .iter()
        .map(|c| CommunityRecord {
            id: c.id,
            members: &c.members,
        })
        .collect();

    // Top 20 hotspots sorted by score descending.
    let mut sorted: Vec<_> = metrics.iter().collect();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_hotspots: Vec<(String, f64)> = sorted
        .iter()
        .take(20)
        .map(|m| (m.id.clone(), m.score))
        .collect();

    let summary = Summary {
        total_nodes: metrics.len(),
        total_edges: graph.edge_count(),
        total_communities: communities.len(),
        total_cycles: cycles.len(),
        top_hotspots,
    };

    // Compute confidence summary from graph edges.
    let all_edges = graph.edges();
    let total_edge_count = all_edges.len();
    let mut extracted = 0usize;
    let mut inferred = 0usize;
    let mut ambiguous = 0usize;
    let mut expected_external = 0usize;
    let mut confidence_sum = 0.0f64;

    for (_, _, edge) in &all_edges {
        match edge.confidence_kind {
            graphify_core::types::ConfidenceKind::Extracted => extracted += 1,
            graphify_core::types::ConfidenceKind::Inferred => inferred += 1,
            graphify_core::types::ConfidenceKind::Ambiguous => ambiguous += 1,
            graphify_core::types::ConfidenceKind::ExpectedExternal => expected_external += 1,
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

    let confidence_summary = ConfidenceSummary {
        extracted_count: extracted,
        extracted_pct: pct(extracted),
        inferred_count: inferred,
        inferred_pct: pct(inferred),
        ambiguous_count: ambiguous,
        ambiguous_pct: pct(ambiguous),
        expected_external_count: expected_external,
        expected_external_pct: pct(expected_external),
        mean_confidence: if total_edge_count > 0 {
            confidence_sum / total_edge_count as f64
        } else {
            0.0
        },
    };

    let payload = AnalysisJson {
        nodes,
        communities: communities_rec,
        cycles,
        summary,
        confidence_summary,
    };

    let json = serde_json::to_string_pretty(&payload).expect("serialize analysis JSON");
    std::fs::write(path, json).expect("write analysis JSON");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::{
        graph::CodeGraph,
        metrics::NodeMetrics,
        types::{Edge, Language, Node},
    };

    fn make_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module(
            "app.main",
            "app/main.py",
            Language::Python,
            1,
            true,
        ));
        g.add_node(Node::module(
            "app.utils",
            "app/utils.py",
            Language::Python,
            1,
            true,
        ));
        g.add_edge("app.main", "app.utils", Edge::imports(3));
        g
    }

    fn make_metrics() -> Vec<NodeMetrics> {
        vec![
            NodeMetrics {
                id: "app.main".to_string(),
                betweenness: 0.5,
                pagerank: 0.3,
                in_degree: 1,
                out_degree: 2,
                in_cycle: false,
                score: 0.4,
                ..Default::default()
            },
            NodeMetrics {
                id: "app.utils".to_string(),
                betweenness: 0.1,
                pagerank: 0.2,
                in_degree: 0,
                out_degree: 0,
                in_cycle: false,
                score: 0.1,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn write_graph_json_file_exists_and_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");
        let graph = make_graph();

        write_graph_json(&graph, &path);

        assert!(path.exists(), "graph.json should be created");
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["directed"], true);
        assert!(!value["nodes"].as_array().unwrap().is_empty());
        assert!(!value["links"].as_array().unwrap().is_empty());
    }

    #[test]
    fn write_analysis_json_summary_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis.json");
        let graph = make_graph();
        let metrics = make_metrics();
        let communities = vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
        }];
        let cycles: Vec<Cycle> = vec![vec!["app.main".to_string(), "app.utils".to_string()]];

        write_analysis_json(&metrics, &communities, &cycles, &graph, &path);

        assert!(path.exists(), "analysis.json should be created");
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["summary"]["total_nodes"], 2);
        assert_eq!(value["summary"]["total_communities"], 1);
        assert_eq!(value["summary"]["total_cycles"], 1);
        assert!(!value["summary"]["top_hotspots"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn write_graph_json_includes_confidence_fields() {
        use graphify_core::types::ConfidenceKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.json");

        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
        g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
        g.add_edge(
            "a",
            "b",
            Edge::imports(3).with_confidence(0.85, ConfidenceKind::Inferred),
        );

        write_graph_json(&g, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        let link = &value["links"][0];
        assert_eq!(link["confidence"], 0.85);
        assert_eq!(link["confidence_kind"], "Inferred");
    }

    #[test]
    fn write_analysis_json_includes_hotspot_type_per_node() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis.json");
        let graph = make_graph();

        let mut metrics = make_metrics();
        metrics[0].hotspot_type = HotspotType::Bridge;
        metrics[1].hotspot_type = HotspotType::Hub;

        let cycles: Vec<Cycle> = vec![];
        write_analysis_json(&metrics, &[], &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        let nodes = value["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["hotspot_type"], "bridge");
        assert_eq!(nodes[1]["hotspot_type"], "hub");
    }

    #[test]
    fn write_analysis_json_includes_confidence_summary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis.json");
        let graph = make_graph();
        let metrics = make_metrics();
        let communities = vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
        }];
        let cycles: Vec<Cycle> = vec![];

        write_analysis_json(&metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(value["confidence_summary"].is_object());
        assert!(value["confidence_summary"]["extracted_count"].is_number());
        assert!(value["confidence_summary"]["mean_confidence"].is_number());
    }
}
