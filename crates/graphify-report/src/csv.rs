use std::path::Path;

use graphify_core::{graph::CodeGraph, metrics::NodeMetrics};

/// Writes node metrics to a CSV file.
///
/// Header: `id,betweenness,pagerank,in_degree,out_degree,score,community_id,in_cycle`
///
/// # Panics
/// Panics if file I/O or CSV serialization fails.
pub fn write_nodes_csv(metrics: &[NodeMetrics], path: &Path) {
    let mut wtr = csv::Writer::from_path(path).expect("open nodes CSV for writing");
    wtr.write_record(&[
        "id",
        "betweenness",
        "pagerank",
        "in_degree",
        "out_degree",
        "score",
        "community_id",
        "in_cycle",
    ])
    .expect("write nodes CSV header");

    for m in metrics {
        wtr.write_record(&[
            m.id.as_str(),
            &m.betweenness.to_string(),
            &m.pagerank.to_string(),
            &m.in_degree.to_string(),
            &m.out_degree.to_string(),
            &m.score.to_string(),
            &m.community_id.to_string(),
            &m.in_cycle.to_string(),
        ])
        .expect("write nodes CSV row");
    }

    wtr.flush().expect("flush nodes CSV");
}

/// Writes graph edges to a CSV file.
///
/// Header: `source,target,kind,weight,line`
///
/// # Panics
/// Panics if file I/O or CSV serialization fails.
pub fn write_edges_csv(graph: &CodeGraph, path: &Path) {
    let mut wtr = csv::Writer::from_path(path).expect("open edges CSV for writing");
    wtr.write_record(&["source", "target", "kind", "weight", "line"])
        .expect("write edges CSV header");

    for (src, tgt, edge) in graph.edges() {
        wtr.write_record(&[
            src,
            tgt,
            &format!("{:?}", edge.kind),
            &edge.weight.to_string(),
            &edge.line.to_string(),
        ])
        .expect("write edges CSV row");
    }

    wtr.flush().expect("flush edges CSV");
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

    fn make_metrics() -> Vec<NodeMetrics> {
        vec![NodeMetrics {
            id: "app.main".to_string(),
            betweenness: 0.5,
            pagerank: 0.3,
            in_degree: 1,
            out_degree: 2,
            in_cycle: true,
            score: 0.4,
            community_id: 0,
        }]
    }

    fn make_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("app.main", "app/main.py", Language::Python, 1, true));
        g.add_node(Node::module("app.utils", "app/utils.py", Language::Python, 1, true));
        g.add_edge("app.main", "app.utils", Edge::imports(3));
        g
    }

    #[test]
    fn write_nodes_csv_header_and_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodes.csv");
        write_nodes_csv(&make_metrics(), &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(
            lines[0],
            "id,betweenness,pagerank,in_degree,out_degree,score,community_id,in_cycle"
        );
        assert!(lines.len() >= 2, "should have at least one data row");
        assert!(lines[1].starts_with("app.main,"));
    }

    #[test]
    fn write_edges_csv_header_and_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edges.csv");
        let graph = make_graph();
        write_edges_csv(&graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines[0], "source,target,kind,weight,line");
        assert!(lines.len() >= 2, "should have at least one edge row");
        // The edge is app.main → app.utils, Imports kind
        assert!(lines[1].contains("app.main") || lines[1].contains("app.utils"));
        assert!(lines[1].contains("Imports"));
    }
}
