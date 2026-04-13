use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::graph::CodeGraph;
use graphify_core::metrics::NodeMetrics;

use crate::{Community, Cycle};

/// Generates a Markdown architecture report and writes it to `path`.
///
/// Sections:
/// - Title
/// - Summary (node count, community count, circular dep count, confidence breakdown)
/// - Top Hotspots table (top 20 by score)
/// - Communities
/// - Circular Dependencies (omitted when `cycles` is empty)
///
/// # Panics
/// Panics if file I/O fails.
pub fn write_report(
    project_name: &str,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    graph: &CodeGraph,
    path: &Path,
) {
    let mut buf = String::new();

    // Title
    writeln!(buf, "# Architecture Report: {project_name}").unwrap();
    writeln!(buf).unwrap();

    // Summary
    writeln!(buf, "## Summary").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Metric | Value |").unwrap();
    writeln!(buf, "|--------|-------|").unwrap();
    writeln!(buf, "| Total nodes | {} |", metrics.len()).unwrap();
    writeln!(buf, "| Communities | {} |", communities.len()).unwrap();
    writeln!(buf, "| Circular dependencies | {} |", cycles.len()).unwrap();

    // Confidence breakdown
    let all_edges = graph.edges();
    let total = all_edges.len();
    if total > 0 {
        let extracted = all_edges
            .iter()
            .filter(|(_, _, e)| {
                matches!(
                    e.confidence_kind,
                    graphify_core::types::ConfidenceKind::Extracted
                )
            })
            .count();
        let inferred = all_edges
            .iter()
            .filter(|(_, _, e)| {
                matches!(
                    e.confidence_kind,
                    graphify_core::types::ConfidenceKind::Inferred
                )
            })
            .count();
        let ambiguous = total - extracted - inferred;
        let mean: f64 = all_edges.iter().map(|(_, _, e)| e.confidence).sum::<f64>() / total as f64;
        writeln!(
            buf,
            "| Confidence | {:.1}% extracted, {:.1}% inferred, {:.1}% ambiguous (mean: {:.2}) |",
            extracted as f64 / total as f64 * 100.0,
            inferred as f64 / total as f64 * 100.0,
            ambiguous as f64 / total as f64 * 100.0,
            mean,
        )
        .unwrap();
    }

    writeln!(buf).unwrap();

    // Top Hotspots table
    writeln!(buf, "## Top Hotspots").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "| Rank | Module | Score | Betweenness | PageRank | In-degree | In cycle |"
    )
    .unwrap();
    writeln!(
        buf,
        "|------|--------|-------|-------------|----------|-----------|----------|"
    )
    .unwrap();

    let mut sorted: Vec<&NodeMetrics> = metrics.iter().collect();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (rank, m) in sorted.iter().take(20).enumerate() {
        writeln!(
            buf,
            "| {} | `{}` | {:.4} | {:.4} | {:.4} | {} | {} |",
            rank + 1,
            m.id,
            m.score,
            m.betweenness,
            m.pagerank,
            m.in_degree,
            if m.in_cycle { "yes" } else { "no" },
        )
        .unwrap();
    }
    writeln!(buf).unwrap();

    // Communities
    writeln!(buf, "## Communities").unwrap();
    writeln!(buf).unwrap();
    for community in communities {
        writeln!(buf, "### Community {}", community.id).unwrap();
        writeln!(buf).unwrap();
        for member in &community.members {
            writeln!(buf, "- `{member}`").unwrap();
        }
        writeln!(buf).unwrap();
    }

    // Circular Dependencies (only when present)
    if !cycles.is_empty() {
        writeln!(buf, "## Circular Dependencies").unwrap();
        writeln!(buf).unwrap();
        for (i, cycle) in cycles.iter().enumerate() {
            let chain = cycle.join(" → ");
            writeln!(buf, "{}. {chain}", i + 1).unwrap();
        }
        writeln!(buf).unwrap();
    }

    std::fs::write(path, buf).expect("write markdown report");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::metrics::NodeMetrics;
    use graphify_core::types::{Edge, Language, Node};

    fn make_metrics() -> Vec<NodeMetrics> {
        vec![
            NodeMetrics {
                id: "app.main".to_string(),
                betweenness: 0.5,
                pagerank: 0.3,
                in_degree: 2,
                out_degree: 1,
                in_cycle: false,
                score: 0.42,
                community_id: 0,
            },
            NodeMetrics {
                id: "app.utils".to_string(),
                betweenness: 0.1,
                pagerank: 0.1,
                in_degree: 0,
                out_degree: 2,
                in_cycle: true,
                score: 0.08,
                community_id: 0,
            },
        ]
    }

    fn make_communities() -> Vec<Community> {
        vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
        }]
    }

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

    #[test]
    fn write_report_contains_title_and_hotspots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![vec!["app.main".to_string(), "app.utils".to_string()]];
        let graph = make_graph();

        write_report("my-project", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Architecture Report: my-project"));
        assert!(content.contains("## Top Hotspots"));
        assert!(content.contains("## Circular Dependencies"));
        assert!(content.contains("app.main"));
    }

    #[test]
    fn write_report_no_cycles_omits_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_no_cycles.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];
        let graph = make_graph();

        write_report("my-project", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("## Circular Dependencies"));
    }

    #[test]
    fn write_report_confidence_breakdown() {
        use graphify_core::types::ConfidenceKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_conf.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];

        let mut graph = CodeGraph::new();
        graph.add_node(Node::module("a", "a.py", Language::Python, 1, true));
        graph.add_node(Node::module("b", "b.py", Language::Python, 1, true));
        graph.add_node(Node::module("c", "c.py", Language::Python, 1, true));
        graph.add_edge("a", "b", Edge::imports(1));
        graph.add_edge(
            "a",
            "c",
            Edge::imports(2).with_confidence(0.7, ConfidenceKind::Inferred),
        );

        write_report("test", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Confidence"));
        assert!(content.contains("extracted"));
        assert!(content.contains("inferred"));
    }
}
