//! Integration test for the HTML report pipeline.

use graphify_core::{
    community::Community,
    graph::CodeGraph,
    metrics::NodeMetrics,
    types::{Edge, Language, Node},
};
use graphify_report::{write_html, Cycle};

fn build_test_graph() -> (CodeGraph, Vec<NodeMetrics>, Vec<Community>, Vec<Cycle>) {
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
    g.add_node(Node::module(
        "app.db",
        "app/db.py",
        Language::Python,
        1,
        true,
    ));
    g.add_node(Node::module(
        "app.api",
        "app/api.py",
        Language::Python,
        1,
        true,
    ));

    g.add_edge("app.main", "app.utils", Edge::imports(1));
    g.add_edge("app.main", "app.db", Edge::imports(2));
    g.add_edge("app.api", "app.main", Edge::imports(3));
    g.add_edge("app.utils", "app.db", Edge::calls(5));

    let metrics = vec![
        NodeMetrics {
            id: "app.main".into(),
            betweenness: 0.8,
            pagerank: 0.3,
            in_degree: 1,
            out_degree: 2,
            in_cycle: false,
            score: 0.6,
            community_id: 0,
        },
        NodeMetrics {
            id: "app.utils".into(),
            betweenness: 0.3,
            pagerank: 0.2,
            in_degree: 1,
            out_degree: 1,
            in_cycle: false,
            score: 0.3,
            community_id: 0,
        },
        NodeMetrics {
            id: "app.db".into(),
            betweenness: 0.1,
            pagerank: 0.25,
            in_degree: 2,
            out_degree: 0,
            in_cycle: false,
            score: 0.2,
            community_id: 1,
        },
        NodeMetrics {
            id: "app.api".into(),
            betweenness: 0.0,
            pagerank: 0.15,
            in_degree: 0,
            out_degree: 1,
            in_cycle: false,
            score: 0.1,
            community_id: 1,
        },
    ];

    let communities = vec![
        Community {
            id: 0,
            members: vec!["app.main".into(), "app.utils".into()],
        },
        Community {
            id: 1,
            members: vec!["app.db".into(), "app.api".into()],
        },
    ];

    let cycles: Vec<Cycle> = vec![];

    (g, metrics, communities, cycles)
}

#[test]
fn full_pipeline_html_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("architecture_graph.html");

    let (graph, metrics, communities, cycles) = build_test_graph();
    write_html(
        "integration-test",
        &graph,
        &metrics,
        &communities,
        &cycles,
        &path,
    );

    assert!(path.exists(), "HTML file should be created");

    let content = std::fs::read_to_string(&path).unwrap();

    // Structure checks
    assert!(content.contains("<!DOCTYPE html>"));
    assert!(content.contains("<title>Graphify: integration-test</title>"));
    assert!(content.contains("GRAPHIFY_DATA"));
    assert!(content.contains("forceSimulation"));
    assert!(content.contains("id=\"sidebar\""));
    assert!(content.contains("id=\"viewport\""));
    assert!(content.contains("id=\"export-png\""));

    // Data checks
    assert!(content.contains("app.main"));
    assert!(content.contains("app.utils"));
    assert!(content.contains("app.db"));
    assert!(content.contains("app.api"));

    // Verify data block is valid JSON
    let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
    let end = content[start..].find(";\n</script>").unwrap() + start;
    let json_str = &content[start..end];
    let value: serde_json::Value =
        serde_json::from_str(json_str).expect("data should be valid JSON");
    assert_eq!(value["project_name"], "integration-test");
    assert_eq!(value["summary"]["total_nodes"], 4);
    assert_eq!(value["summary"]["total_edges"], 4);
    assert_eq!(value["summary"]["total_communities"], 2);
    assert_eq!(value["communities"].as_array().unwrap().len(), 2);
}
