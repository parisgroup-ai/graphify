use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use graphify_core::{graph::CodeGraph, metrics::NodeMetrics};

use crate::{Community, Cycle};

// Embed assets at compile time.
const D3_JS: &str = include_str!("../assets/d3.v7.min.js");
const GRAPH_JS: &str = include_str!("../assets/graph.js");
const GRAPH_CSS: &str = include_str!("../assets/graph.css");

// ---------------------------------------------------------------------------
// Data structures for the merged JSON blob
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HtmlNodeData {
    id: String,
    kind: String,
    file_path: String,
    language: String,
    line: usize,
    is_local: bool,
    betweenness: f64,
    pagerank: f64,
    in_degree: usize,
    out_degree: usize,
    in_cycle: bool,
    score: f64,
    community_id: usize,
    /// FEAT-021: TS-only barrel re-export aliases. Always present in the JSON
    /// blob (possibly empty) so `graph.js` can render a consistent inspector
    /// block without branching on presence.
    alternative_paths: Vec<String>,
}

#[derive(Serialize)]
struct HtmlEdgeData {
    source: String,
    target: String,
    kind: String,
    weight: u32,
    confidence: f64,
    confidence_kind: String,
}

#[derive(Serialize)]
struct HtmlCommunityData {
    id: usize,
    members: Vec<String>,
}

#[derive(Serialize)]
struct HtmlSummary {
    total_nodes: usize,
    total_edges: usize,
    total_communities: usize,
    total_cycles: usize,
}

#[derive(Serialize)]
struct HtmlGraphData {
    project_name: String,
    nodes: Vec<HtmlNodeData>,
    edges: Vec<HtmlEdgeData>,
    communities: Vec<HtmlCommunityData>,
    cycles: Vec<Vec<String>>,
    summary: HtmlSummary,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generates a self-contained interactive HTML visualization and writes it
/// to `path`.
///
/// The HTML file embeds D3.js, graph.js, graph.css, and the serialized
/// graph + analysis data. It can be opened in any modern browser with no
/// server or internet connection required.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_html(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    path: &Path,
) {
    let data = build_data(project_name, graph, metrics, communities, cycles);
    let data_json = serde_json::to_string(&data).expect("serialize HTML data");
    // Escape </script> sequences that could appear in node IDs or file paths.
    let data_json = data_json.replace("</script>", r"<\/script>");

    let capacity = D3_JS.len() + GRAPH_JS.len() + GRAPH_CSS.len() + data_json.len() + 4096;
    let mut html = String::with_capacity(capacity);

    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("<title>Graphify: {}</title>\n", project_name));
    html.push_str("<style>\n");
    html.push_str(GRAPH_CSS);
    html.push_str("\n</style>\n");
    html.push_str("</head>\n<body>\n");

    // D3.js library
    html.push_str("<script>\n");
    html.push_str(D3_JS);
    html.push_str("\n</script>\n");

    // Graph data
    html.push_str("<script>\nvar GRAPHIFY_DATA = ");
    html.push_str(&data_json);
    html.push_str(";\n</script>\n");

    // Page structure
    html.push_str(concat!(
        "<div id=\"app\">\n",
        "  <header id=\"header\">\n",
        "    <h1>Graphify: <span id=\"project-name\"></span></h1>\n",
        "    <button id=\"export-png\" title=\"Export as PNG\">PNG</button>\n",
        "  </header>\n",
        "  <div id=\"main\">\n",
        "    <aside id=\"sidebar\"></aside>\n",
        "    <div id=\"viewport\"></div>\n",
        "  </div>\n",
        "  <footer id=\"footer\">\n",
        "    <span id=\"tooltip\">Hover over a node to see details</span>\n",
        "  </footer>\n",
        "</div>\n",
    ));

    // Visualization script
    html.push_str("<script>\n");
    html.push_str(GRAPH_JS);
    html.push_str("\n</script>\n");

    html.push_str("</body>\n</html>\n");

    std::fs::write(path, html).expect("write HTML report");
}

// ---------------------------------------------------------------------------
// Data assembly
// ---------------------------------------------------------------------------

fn build_data(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
) -> HtmlGraphData {
    let metrics_map: HashMap<&str, &NodeMetrics> =
        metrics.iter().map(|m| (m.id.as_str(), m)).collect();

    let nodes: Vec<HtmlNodeData> = graph
        .nodes()
        .into_iter()
        .map(|n| {
            let m = metrics_map.get(n.id.as_str());
            HtmlNodeData {
                id: n.id.clone(),
                kind: format!("{:?}", n.kind),
                file_path: n.file_path.to_string_lossy().into_owned(),
                language: format!("{:?}", n.language),
                line: n.line,
                is_local: n.is_local,
                betweenness: m.map_or(0.0, |m| m.betweenness),
                pagerank: m.map_or(0.0, |m| m.pagerank),
                in_degree: m.map_or(0, |m| m.in_degree),
                out_degree: m.map_or(0, |m| m.out_degree),
                in_cycle: m.is_some_and(|m| m.in_cycle),
                score: m.map_or(0.0, |m| m.score),
                community_id: m.map_or(0, |m| m.community_id),
                alternative_paths: n.alternative_paths.clone(),
            }
        })
        .collect();

    let edges: Vec<HtmlEdgeData> = graph
        .edges()
        .into_iter()
        .map(|(src, tgt, e)| HtmlEdgeData {
            source: src.to_string(),
            target: tgt.to_string(),
            kind: format!("{:?}", e.kind),
            weight: e.weight,
            confidence: e.confidence,
            confidence_kind: format!("{:?}", e.confidence_kind),
        })
        .collect();

    let communities_data: Vec<HtmlCommunityData> = communities
        .iter()
        .map(|c| HtmlCommunityData {
            id: c.id,
            members: c.members.clone(),
        })
        .collect();

    HtmlGraphData {
        project_name: project_name.to_string(),
        nodes,
        edges,
        communities: communities_data,
        cycles: cycles.to_vec(),
        summary: HtmlSummary {
            total_nodes: metrics.len(),
            total_edges: graph.edge_count(),
            total_communities: communities.len(),
            total_cycles: cycles.len(),
        },
    }
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

    fn make_communities() -> Vec<Community> {
        vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
            cohesion: 0.0,
        }]
    }

    fn make_cycles() -> Vec<Cycle> {
        vec![vec!["app.main".to_string(), "app.utils".to_string()]]
    }

    #[test]
    fn write_html_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "test-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        assert!(path.exists(), "HTML file should be created");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.is_empty(), "HTML file should not be empty");
    }

    #[test]
    fn html_contains_data_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "test-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("GRAPHIFY_DATA"),
            "should contain data block"
        );
        let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
        let end = content[start..].find(";\n</script>").unwrap() + start;
        let json_str = &content[start..end];
        let value: serde_json::Value =
            serde_json::from_str(json_str).expect("data should be valid JSON");
        assert_eq!(value["project_name"], "test-project");
        assert_eq!(value["summary"]["total_nodes"], 2);
    }

    #[test]
    fn html_contains_d3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "test-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("forceSimulation"),
            "should contain D3.js force module"
        );
    }

    #[test]
    fn html_contains_project_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "my-cool-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<title>Graphify: my-cool-project</title>"));
        assert!(content.contains("my-cool-project"));
    }

    #[test]
    fn html_data_blob_carries_alternative_paths() {
        use graphify_core::types::NodeKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("alts.html");

        let mut g = CodeGraph::new();
        g.add_node(
            Node::symbol(
                "src.entities.Course",
                NodeKind::Class,
                "src/entities/course.ts",
                Language::TypeScript,
                1,
                true,
            )
            .with_alternative_paths(["src.domain.Course", "src.presentation.Course"]),
        );

        let metrics = vec![NodeMetrics {
            id: "src.entities.Course".to_string(),
            score: 0.9,
            ..Default::default()
        }];

        write_html("alts", &g, &metrics, &[], &[], &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
        let end = content[start..].find(";\n</script>").unwrap() + start;
        let value: serde_json::Value = serde_json::from_str(&content[start..end]).unwrap();
        let alts = &value["nodes"][0]["alternative_paths"];
        assert_eq!(alts[0], "src.domain.Course");
        assert_eq!(alts[1], "src.presentation.Course");
    }

    #[test]
    fn html_empty_graph() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.html");
        write_html("empty", &CodeGraph::new(), &[], &[], &[], &path);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("GRAPHIFY_DATA"));
        let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
        let end = content[start..].find(";\n</script>").unwrap() + start;
        let json_str = &content[start..end];
        let value: serde_json::Value =
            serde_json::from_str(json_str).expect("should be valid JSON");
        assert_eq!(value["summary"]["total_nodes"], 0);
    }

    #[test]
    fn html_single_node_no_edges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("single.html");
        let mut g = CodeGraph::new();
        g.add_node(Node::module(
            "only.module",
            "only/module.py",
            Language::Python,
            1,
            true,
        ));
        let metrics = vec![NodeMetrics {
            id: "only.module".to_string(),
            betweenness: 0.0,
            pagerank: 1.0,
            in_degree: 0,
            out_degree: 0,
            in_cycle: false,
            score: 0.0,
            ..Default::default()
        }];
        write_html("single", &g, &metrics, &[], &[], &path);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("only.module"));
    }
}
