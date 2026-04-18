use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::graph::CodeGraph;
use graphify_core::metrics::{HotspotType, NodeMetrics};

fn hotspot_type_label(t: HotspotType) -> &'static str {
    match t {
        HotspotType::Hub => "hub",
        HotspotType::Bridge => "bridge",
        HotspotType::Mixed => "mixed",
    }
}

/// Label for a node's incoming-edge mean confidence, shown in the hotspot table.
///
/// Thresholds match the resolver's confidence scale: 0.9+ for fully-resolved
/// imports, 0.5+ for merged/partial matches, below 0.5 for ambiguous edges.
fn confidence_label(mean_in: Option<f64>) -> &'static str {
    match mean_in {
        None => "—",
        Some(v) if v >= 0.9 => "resolved",
        Some(v) if v >= 0.5 => "partial",
        Some(_) => "ambiguous",
    }
}

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

    // Pre-compute confidence stats (issue #2):
    //   - mean overall confidence and ambiguous percentage drive the banner
    //   - per-node incoming-edge mean drives the hotspot Confidence column
    let all_edges = graph.edges();
    let total = all_edges.len();
    let (extracted, inferred, ambiguous, expected_external, mean_conf) = if total > 0 {
        let mut extracted = 0usize;
        let mut inferred = 0usize;
        let mut ambiguous = 0usize;
        let mut expected_external = 0usize;
        for (_, _, e) in &all_edges {
            match e.confidence_kind {
                graphify_core::types::ConfidenceKind::Extracted => extracted += 1,
                graphify_core::types::ConfidenceKind::Inferred => inferred += 1,
                graphify_core::types::ConfidenceKind::Ambiguous => ambiguous += 1,
                graphify_core::types::ConfidenceKind::ExpectedExternal => expected_external += 1,
            }
        }
        let mean: f64 = all_edges.iter().map(|(_, _, e)| e.confidence).sum::<f64>() / total as f64;
        (
            extracted,
            inferred,
            ambiguous,
            expected_external,
            Some(mean),
        )
    } else {
        (0, 0, 0, 0, None)
    };
    let ambiguous_pct = if total > 0 {
        ambiguous as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    // Low-confidence banner: warn readers before they trust the hotspot table.
    // Triggers on either mean < 0.7 or ambiguous edges exceeding half the graph.
    if let Some(mean) = mean_conf {
        if mean < 0.7 || ambiguous_pct > 50.0 {
            writeln!(
                buf,
                "> ⚠ **Low-confidence extraction.** {ambiguous} of {total} edges are ambiguous \
                 ({ambiguous_pct:.0}%); mean confidence is {mean:.2}. Hotspot rankings below may \
                 be skewed by unresolved references."
            )
            .unwrap();
            writeln!(buf).unwrap();
        }
    }

    // Per-node incoming-edge confidence mean, used by the Confidence column.
    let mut in_conf_sum: HashMap<&str, f64> = HashMap::new();
    let mut in_conf_count: HashMap<&str, usize> = HashMap::new();
    for (_src, tgt, edge) in &all_edges {
        *in_conf_sum.entry(*tgt).or_insert(0.0) += edge.confidence;
        *in_conf_count.entry(*tgt).or_insert(0) += 1;
    }
    let per_node_in_conf: HashMap<&str, f64> = in_conf_count
        .iter()
        .map(|(k, n)| (*k, in_conf_sum[k] / *n as f64))
        .collect();

    // Summary
    writeln!(buf, "## Summary").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Metric | Value |").unwrap();
    writeln!(buf, "|--------|-------|").unwrap();
    writeln!(buf, "| Total nodes | {} |", metrics.len()).unwrap();
    writeln!(buf, "| Communities | {} |", communities.len()).unwrap();
    writeln!(buf, "| Circular dependencies | {} |", cycles.len()).unwrap();

    if let Some(mean) = mean_conf {
        let expected_external_pct = expected_external as f64 / total as f64 * 100.0;
        if expected_external > 0 {
            writeln!(
                buf,
                "| Confidence | {:.1}% extracted, {:.1}% inferred, {:.1}% ambiguous, {:.1}% expected-external (mean: {:.2}) |",
                extracted as f64 / total as f64 * 100.0,
                inferred as f64 / total as f64 * 100.0,
                ambiguous_pct,
                expected_external_pct,
                mean,
            )
            .unwrap();
        } else {
            writeln!(
                buf,
                "| Confidence | {:.1}% extracted, {:.1}% inferred, {:.1}% ambiguous (mean: {:.2}) |",
                extracted as f64 / total as f64 * 100.0,
                inferred as f64 / total as f64 * 100.0,
                ambiguous_pct,
                mean,
            )
            .unwrap();
        }
    }

    writeln!(buf).unwrap();

    // Top Hotspots table
    writeln!(buf, "## Top Hotspots").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "| Rank | Module | Type | Score | Betweenness | PageRank | In-degree | In cycle | Confidence |"
    )
    .unwrap();
    writeln!(
        buf,
        "|------|--------|------|-------|-------------|----------|-----------|----------|------------|"
    )
    .unwrap();

    let mut sorted: Vec<&NodeMetrics> = metrics.iter().collect();
    sorted.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_20: Vec<&&NodeMetrics> = sorted.iter().take(20).collect();
    for (rank, m) in top_20.iter().enumerate() {
        let conf = per_node_in_conf.get(m.id.as_str()).copied();
        writeln!(
            buf,
            "| {} | `{}` | {} | {:.4} | {:.4} | {:.4} | {} | {} | {} |",
            rank + 1,
            m.id,
            hotspot_type_label(m.hotspot_type),
            m.score,
            m.betweenness,
            m.pagerank,
            m.in_degree,
            if m.in_cycle { "yes" } else { "no" },
            confidence_label(conf),
        )
        .unwrap();
    }
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "_Type legend: **hub** = high in-degree (split or invert deps) · **bridge** = high betweenness-per-incoming (inject cross-layer dep) · **mixed** = both or neither (human review)._"
    )
    .unwrap();
    writeln!(
        buf,
        "_Confidence: mean of incoming-edge confidence. **resolved** ≥ 0.9 · **partial** 0.5–0.9 · **ambiguous** < 0.5 · **—** no incoming edges._"
    )
    .unwrap();
    writeln!(buf).unwrap();

    // FEAT-021: surface alternative module paths for any hotspot that was
    // reached through one or more re-export barrels. Only emitted when the
    // top-20 hotspots include at least one node with non-empty
    // `alternative_paths`; otherwise the section is skipped to keep the
    // report quiet for non-TS projects.
    let hotspots_with_alts: Vec<(usize, &NodeMetrics, &[String])> = top_20
        .iter()
        .enumerate()
        .filter_map(|(rank, m)| {
            graph.get_node(&m.id).and_then(|node| {
                if node.alternative_paths.is_empty() {
                    None
                } else {
                    Some((rank + 1, **m, node.alternative_paths.as_slice()))
                }
            })
        })
        .collect();
    if !hotspots_with_alts.is_empty() {
        writeln!(buf, "### Alternative Import Paths").unwrap();
        writeln!(buf).unwrap();
        writeln!(
            buf,
            "_Hotspots reached through one or more re-export barrels. The canonical id is shown; each alternative path is another module the symbol could have been imported from. Reported for TypeScript only (FEAT-021)._"
        )
        .unwrap();
        writeln!(buf).unwrap();
        for (rank, m, alts) in &hotspots_with_alts {
            writeln!(buf, "- **#{rank} `{}`**", m.id).unwrap();
            for alt in *alts {
                writeln!(buf, "  - `{alt}`").unwrap();
            }
        }
        writeln!(buf).unwrap();
    }

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
                ..Default::default()
            },
            NodeMetrics {
                id: "app.utils".to_string(),
                betweenness: 0.1,
                pagerank: 0.1,
                in_degree: 0,
                out_degree: 2,
                in_cycle: true,
                score: 0.08,
                ..Default::default()
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
    fn write_report_renders_hotspot_type_column_and_legend() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_type.md");
        let mut metrics = make_metrics();
        metrics[0].hotspot_type = HotspotType::Hub;
        metrics[1].hotspot_type = HotspotType::Bridge;
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];
        let graph = make_graph();

        write_report("typed", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        // Header includes Type column
        assert!(content.contains("| Rank | Module | Type |"));
        // Both labels appear in body rows
        assert!(content.contains("| hub |"));
        assert!(content.contains("| bridge |"));
        // Legend present
        assert!(content.contains("_Type legend"));
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

    // -----------------------------------------------------------------------
    // Issue #2 — low-confidence banner + per-hotspot confidence column
    // -----------------------------------------------------------------------

    #[test]
    fn low_confidence_graph_emits_banner() {
        use graphify_core::types::ConfidenceKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_low.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];

        // Two nodes connected by two ambiguous edges — drives mean to 0.4,
        // and ambiguous_pct to 100%.
        let mut graph = CodeGraph::new();
        graph.add_node(Node::module("a", "a.py", Language::Python, 1, true));
        graph.add_node(Node::module("b", "b.py", Language::Python, 1, true));
        graph.add_edge(
            "a",
            "b",
            Edge::imports(1).with_confidence(0.4, ConfidenceKind::Ambiguous),
        );
        graph.add_edge(
            "b",
            "a",
            Edge::imports(2).with_confidence(0.4, ConfidenceKind::Ambiguous),
        );

        write_report("low", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("Low-confidence extraction"),
            "banner missing, got:\n{content}"
        );
        assert!(content.contains("⚠"));
    }

    #[test]
    fn high_confidence_graph_omits_banner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_high.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];
        let graph = make_graph();

        write_report("high", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("Low-confidence extraction"),
            "banner should be absent on high-confidence graph, got:\n{content}"
        );
    }

    #[test]
    fn hotspot_table_includes_confidence_column() {
        use graphify_core::types::ConfidenceKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_conf_col.md");
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];

        let mut graph = CodeGraph::new();
        graph.add_node(Node::module(
            "high",
            "high.ts",
            Language::TypeScript,
            1,
            true,
        ));
        graph.add_node(Node::module("low", "low.ts", Language::TypeScript, 1, true));
        graph.add_node(Node::module("src", "src.ts", Language::TypeScript, 1, true));
        // `high` gets a fully-extracted incoming edge → resolved.
        graph.add_edge("src", "high", Edge::imports(1));
        // `low` gets an ambiguous incoming edge → ambiguous.
        graph.add_edge(
            "src",
            "low",
            Edge::imports(2).with_confidence(0.3, ConfidenceKind::Ambiguous),
        );

        let metrics = vec![
            NodeMetrics {
                id: "high".to_string(),
                in_degree: 1,
                score: 0.9,
                ..Default::default()
            },
            NodeMetrics {
                id: "low".to_string(),
                in_degree: 1,
                score: 0.5,
                ..Default::default()
            },
            NodeMetrics {
                id: "src".to_string(),
                out_degree: 2,
                score: 0.1,
                ..Default::default()
            },
        ];

        write_report("conf-col", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("| Confidence |"), "missing column header");
        assert!(
            content.contains("| `high` | mixed | 0.9000") && content.contains("| resolved |"),
            "'high' should be labeled 'resolved', got:\n{content}"
        );
        assert!(
            content.contains("| `low` | mixed | 0.5000") && content.contains("| ambiguous |"),
            "'low' should be labeled 'ambiguous', got:\n{content}"
        );
        // `src` has no incoming edges → em-dash.
        assert!(
            content.contains("| `src` | mixed | 0.1000") && content.contains("| — |"),
            "'src' should be labeled '—', got:\n{content}"
        );
    }

    // -----------------------------------------------------------------------
    // Issue #12 — ExpectedExternal surfaces in the confidence-mix row
    // -----------------------------------------------------------------------

    #[test]
    fn confidence_mix_includes_expected_external_when_present() {
        use graphify_core::types::ConfidenceKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_expected_external.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];

        let mut graph = CodeGraph::new();
        graph.add_node(Node::module("a", "a.ts", Language::TypeScript, 1, true));
        graph.add_node(Node::module(
            "drizzle-orm",
            "",
            Language::TypeScript,
            0,
            false,
        ));
        graph.add_edge(
            "a",
            "drizzle-orm",
            Edge::imports(1).with_confidence(0.5, ConfidenceKind::ExpectedExternal),
        );

        write_report("ee", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("expected-external"),
            "confidence row should mention expected-external, got:\n{content}"
        );
    }

    // -----------------------------------------------------------------------
    // FEAT-021 — alternative_paths section
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_emits_alternative_paths_section_when_top_hotspot_has_alts() {
        use graphify_core::types::NodeKind;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_alts.md");

        let mut graph = CodeGraph::new();
        graph.add_node(
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
        graph.add_node(Node::module(
            "src.main",
            "src/main.ts",
            Language::TypeScript,
            1,
            true,
        ));
        graph.add_edge("src.main", "src.entities.Course", Edge::imports(1));

        let metrics = vec![NodeMetrics {
            id: "src.entities.Course".to_string(),
            score: 0.9,
            in_degree: 1,
            ..Default::default()
        }];

        write_report("alts", &metrics, &[], &[], &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("### Alternative Import Paths"),
            "section missing, got:\n{content}"
        );
        assert!(content.contains("**#1 `src.entities.Course`**"));
        assert!(content.contains("- `src.domain.Course`"));
        assert!(content.contains("- `src.presentation.Course`"));
    }

    #[test]
    fn markdown_omits_alternative_paths_section_when_no_alts_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_noalts.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];
        let graph = make_graph();

        write_report("no-alts", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("### Alternative Import Paths"),
            "section should be omitted when nothing has alts, got:\n{content}"
        );
    }

    #[test]
    fn confidence_mix_omits_expected_external_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report_no_ee.md");
        let metrics = make_metrics();
        let communities = make_communities();
        let cycles: Vec<Cycle> = vec![];
        let graph = make_graph();

        write_report("no-ee", &metrics, &communities, &cycles, &graph, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("expected-external"),
            "row should not mention expected-external when count is zero, got:\n{content}"
        );
    }
}
