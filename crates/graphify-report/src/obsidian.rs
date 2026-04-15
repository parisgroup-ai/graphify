use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::graph::CodeGraph;
use graphify_core::metrics::NodeMetrics;

use crate::{Community, Cycle};

/// Normalize a node id for use as an Obsidian wiki-link target.
///
/// Filenames under the vault replace `/` with `_` (Obsidian interprets `/` as a
/// folder separator). Wiki-link text must use the same canonicalization so the
/// link resolves to the on-disk file.
fn wikilink_target(id: &str) -> String {
    id.replace('/', "_")
}

/// Writes an Obsidian vault directory at `path`, with one `.md` file per node.
///
/// Each markdown file contains YAML frontmatter with node metadata and a body
/// with `[[wikilinks]]` to related nodes (imports, imported by, defines, calls,
/// called by).  Community and cycle membership are also included.
///
/// # Panics
/// Panics if directory creation or file I/O fails.
pub fn write_obsidian_vault(
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    path: &Path,
) {
    std::fs::create_dir_all(path).expect("create Obsidian vault directory");

    // Build lookup maps for quick access.
    let metrics_map: HashMap<&str, &NodeMetrics> =
        metrics.iter().map(|m| (m.id.as_str(), m)).collect();
    let community_map: HashMap<&str, usize> = communities
        .iter()
        .flat_map(|c| c.members.iter().map(move |m| (m.as_str(), c.id)))
        .collect();
    let node_cycles: HashMap<&str, Vec<usize>> = {
        let mut map: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, cycle) in cycles.iter().enumerate() {
            for member in cycle {
                map.entry(member.as_str()).or_default().push(i + 1);
            }
        }
        map
    };

    // Collect edge relationships per node.
    let mut imports_out: HashMap<&str, Vec<(&str, &graphify_core::types::Edge)>> = HashMap::new();
    let mut imports_in: HashMap<&str, Vec<(&str, &graphify_core::types::Edge)>> = HashMap::new();
    let mut defines_out: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut calls_out: HashMap<&str, Vec<(&str, &graphify_core::types::Edge)>> = HashMap::new();
    let mut calls_in: HashMap<&str, Vec<(&str, &graphify_core::types::Edge)>> = HashMap::new();

    for (src, tgt, edge) in graph.edges() {
        match edge.kind {
            graphify_core::types::EdgeKind::Imports => {
                imports_out.entry(src).or_default().push((tgt, edge));
                imports_in.entry(tgt).or_default().push((src, edge));
            }
            graphify_core::types::EdgeKind::Defines => {
                defines_out.entry(src).or_default().push(tgt);
            }
            graphify_core::types::EdgeKind::Calls => {
                calls_out.entry(src).or_default().push((tgt, edge));
                calls_in.entry(tgt).or_default().push((src, edge));
            }
        }
    }

    // Write one file per node.
    for node in graph.nodes() {
        let mut buf = String::new();

        // YAML frontmatter
        writeln!(buf, "---").unwrap();
        writeln!(buf, "id: \"{}\"", node.id).unwrap();
        writeln!(buf, "kind: {:?}", node.kind).unwrap();
        writeln!(buf, "file_path: \"{}\"", node.file_path.to_string_lossy()).unwrap();
        writeln!(buf, "language: {:?}", node.language).unwrap();
        writeln!(buf, "line: {}", node.line).unwrap();
        writeln!(buf, "is_local: {}", node.is_local).unwrap();

        if let Some(m) = metrics_map.get(node.id.as_str()) {
            writeln!(buf, "score: {:.4}", m.score).unwrap();
            writeln!(buf, "betweenness: {:.4}", m.betweenness).unwrap();
            writeln!(buf, "pagerank: {:.4}", m.pagerank).unwrap();
            writeln!(buf, "in_degree: {}", m.in_degree).unwrap();
            writeln!(buf, "out_degree: {}", m.out_degree).unwrap();
            writeln!(buf, "in_cycle: {}", m.in_cycle).unwrap();
        }

        if let Some(&cid) = community_map.get(node.id.as_str()) {
            writeln!(buf, "community: {cid}").unwrap();
        }

        // Tags for Obsidian graph view filtering
        let kind_tag = format!("{:?}", node.kind).to_lowercase();
        writeln!(buf, "tags: [{kind_tag}, {:?}]", node.language).unwrap();
        writeln!(buf, "---").unwrap();
        writeln!(buf).unwrap();

        // Title
        writeln!(buf, "# {}", node.id).unwrap();
        writeln!(buf).unwrap();

        // Imports (outgoing)
        if let Some(targets) = imports_out.get(node.id.as_str()) {
            writeln!(buf, "## Imports").unwrap();
            writeln!(buf).unwrap();
            for (tgt, _edge) in targets {
                writeln!(buf, "- [[{}]]", wikilink_target(tgt)).unwrap();
            }
            writeln!(buf).unwrap();
        }

        // Imported By (incoming)
        if let Some(sources) = imports_in.get(node.id.as_str()) {
            writeln!(buf, "## Imported By").unwrap();
            writeln!(buf).unwrap();
            for (src, _edge) in sources {
                writeln!(buf, "- [[{}]]", wikilink_target(src)).unwrap();
            }
            writeln!(buf).unwrap();
        }

        // Defines
        if let Some(targets) = defines_out.get(node.id.as_str()) {
            writeln!(buf, "## Defines").unwrap();
            writeln!(buf).unwrap();
            for tgt in targets {
                writeln!(buf, "- [[{}]]", wikilink_target(tgt)).unwrap();
            }
            writeln!(buf).unwrap();
        }

        // Calls (outgoing)
        if let Some(targets) = calls_out.get(node.id.as_str()) {
            writeln!(buf, "## Calls").unwrap();
            writeln!(buf).unwrap();
            for (tgt, _edge) in targets {
                writeln!(buf, "- [[{}]]", wikilink_target(tgt)).unwrap();
            }
            writeln!(buf).unwrap();
        }

        // Called By (incoming)
        if let Some(sources) = calls_in.get(node.id.as_str()) {
            writeln!(buf, "## Called By").unwrap();
            writeln!(buf).unwrap();
            for (src, _edge) in sources {
                writeln!(buf, "- [[{}]]", wikilink_target(src)).unwrap();
            }
            writeln!(buf).unwrap();
        }

        // Cycles
        if let Some(cycle_ids) = node_cycles.get(node.id.as_str()) {
            writeln!(buf, "## Cycles").unwrap();
            writeln!(buf).unwrap();
            for cid in cycle_ids {
                let cycle = &cycles[cid - 1];
                let chain: Vec<String> = cycle
                    .iter()
                    .map(|m| format!("[[{}]]", wikilink_target(m)))
                    .collect();
                writeln!(buf, "- Cycle {cid}: {}", chain.join(" → ")).unwrap();
            }
            writeln!(buf).unwrap();
        }

        // Sanitize filename: replace characters that are invalid in filenames.
        let filename = format!("{}.md", node.id.replace('/', "_"));
        let file_path = path.join(&filename);
        std::fs::write(&file_path, buf).expect("write Obsidian note");
    }

    // Write a vault index file.
    let mut index = String::new();
    writeln!(index, "# Architecture Graph").unwrap();
    writeln!(index).unwrap();
    writeln!(
        index,
        "Nodes: {} | Edges: {} | Communities: {} | Cycles: {}",
        graph.node_count(),
        graph.edge_count(),
        communities.len(),
        cycles.len(),
    )
    .unwrap();
    writeln!(index).unwrap();

    writeln!(index, "## Modules").unwrap();
    writeln!(index).unwrap();
    let mut node_ids: Vec<&str> = graph.nodes().iter().map(|n| n.id.as_str()).collect();
    node_ids.sort();
    for id in node_ids {
        writeln!(index, "- [[{}]]", wikilink_target(id)).unwrap();
    }

    std::fs::write(path.join("_index.md"), index).expect("write Obsidian index");

    // Issue #5: self-contained vault — mirror architecture_report.md into a
    // vault-internal note so users opening the vault in Obsidian get the full
    // analytical context (hotspots, communities, cycles) without leaving the
    // vault. `00-` prefix sorts it first in Obsidian's file explorer.
    write_overview_note(graph, metrics, communities, cycles, path);
}

/// Writes a vault-internal `00-architecture-overview.md` that mirrors the
/// content of `architecture_report.md`, with wiki-links to per-node notes.
fn write_overview_note(
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    path: &Path,
) {
    let mut buf = String::new();

    // Frontmatter tags for Obsidian filtering (e.g. "tag:#graphify/overview").
    writeln!(buf, "---").unwrap();
    writeln!(buf, "tags: [graphify, architecture, overview]").unwrap();
    writeln!(buf, "---").unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "# Architecture Overview").unwrap();
    writeln!(buf).unwrap();

    // Summary
    writeln!(buf, "## Summary").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Metric | Value |").unwrap();
    writeln!(buf, "|--------|-------|").unwrap();
    writeln!(buf, "| Total nodes | {} |", graph.node_count()).unwrap();
    writeln!(buf, "| Total edges | {} |", graph.edge_count()).unwrap();
    writeln!(buf, "| Communities | {} |", communities.len()).unwrap();
    writeln!(buf, "| Circular dependencies | {} |", cycles.len()).unwrap();
    writeln!(buf).unwrap();

    // Top Hotspots (top 20 by score) — wiki-linked for navigation.
    if !metrics.is_empty() {
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
                "| {} | [[{}]] | {:.4} | {:.4} | {:.4} | {} | {} |",
                rank + 1,
                wikilink_target(&m.id),
                m.score,
                m.betweenness,
                m.pagerank,
                m.in_degree,
                if m.in_cycle { "yes" } else { "no" },
            )
            .unwrap();
        }
        writeln!(buf).unwrap();
    }

    // Communities
    if !communities.is_empty() {
        writeln!(buf, "## Communities").unwrap();
        writeln!(buf).unwrap();
        for community in communities {
            writeln!(
                buf,
                "### Community {} ({} members)",
                community.id,
                community.members.len()
            )
            .unwrap();
            writeln!(buf).unwrap();
            for member in &community.members {
                writeln!(buf, "- [[{}]]", wikilink_target(member)).unwrap();
            }
            writeln!(buf).unwrap();
        }
    }

    // Circular Dependencies (only when present)
    if !cycles.is_empty() {
        writeln!(buf, "## Circular Dependencies").unwrap();
        writeln!(buf).unwrap();
        for (i, cycle) in cycles.iter().enumerate() {
            let chain: Vec<String> = cycle
                .iter()
                .map(|m| format!("[[{}]]", wikilink_target(m)))
                .collect();
            writeln!(buf, "{}. {}", i + 1, chain.join(" → ")).unwrap();
        }
        writeln!(buf).unwrap();
    }

    std::fs::write(path.join("00-architecture-overview.md"), buf)
        .expect("write Obsidian overview note");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{Edge, Language, Node};

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
            5,
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
                in_degree: 0,
                out_degree: 1,
                in_cycle: false,
                score: 0.4,
                ..Default::default()
            },
            NodeMetrics {
                id: "app.utils".to_string(),
                betweenness: 0.1,
                pagerank: 0.2,
                in_degree: 1,
                out_degree: 0,
                in_cycle: false,
                score: 0.1,
                ..Default::default()
            },
        ]
    }

    #[test]
    fn write_obsidian_creates_directory_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");
        let graph = make_graph();
        let metrics = make_metrics();
        let communities = vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
        }];
        let cycles: Vec<Cycle> = vec![];

        write_obsidian_vault(&graph, &metrics, &communities, &cycles, &vault_path);

        assert!(vault_path.exists());
        assert!(vault_path.join("app.main.md").exists());
        assert!(vault_path.join("app.utils.md").exists());
        assert!(vault_path.join("_index.md").exists());
    }

    #[test]
    fn obsidian_note_has_frontmatter_and_wikilinks() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");
        let graph = make_graph();
        let metrics = make_metrics();
        let communities = vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
        }];
        let cycles: Vec<Cycle> = vec![];

        write_obsidian_vault(&graph, &metrics, &communities, &cycles, &vault_path);

        let main_content = std::fs::read_to_string(vault_path.join("app.main.md")).unwrap();
        assert!(main_content.contains("---"));
        assert!(main_content.contains("kind: Module"));
        assert!(main_content.contains("language: Python"));
        assert!(main_content.contains("# app.main"));
        assert!(main_content.contains("## Imports"));
        assert!(main_content.contains("- [[app.utils]]"));

        let utils_content = std::fs::read_to_string(vault_path.join("app.utils.md")).unwrap();
        assert!(utils_content.contains("## Imported By"));
        assert!(utils_content.contains("- [[app.main]]"));
    }

    #[test]
    fn obsidian_note_includes_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");
        let graph = make_graph();
        let metrics = make_metrics();

        write_obsidian_vault(&graph, &metrics, &[], &[], &vault_path);

        let content = std::fs::read_to_string(vault_path.join("app.main.md")).unwrap();
        assert!(content.contains("score: 0.4000"));
        assert!(content.contains("betweenness: 0.5000"));
        assert!(
            !content.contains("community: 0"),
            "no community without communities list"
        );
    }

    #[test]
    fn obsidian_note_includes_cycles() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");

        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
        g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));

        let metrics = vec![
            NodeMetrics {
                id: "a".to_string(),
                betweenness: 0.0,
                pagerank: 0.5,
                in_degree: 1,
                out_degree: 1,
                in_cycle: true,
                score: 0.5,
                ..Default::default()
            },
            NodeMetrics {
                id: "b".to_string(),
                betweenness: 0.0,
                pagerank: 0.5,
                in_degree: 1,
                out_degree: 1,
                in_cycle: true,
                score: 0.5,
                ..Default::default()
            },
        ];
        let cycles = vec![vec!["a".to_string(), "b".to_string()]];

        write_obsidian_vault(&g, &metrics, &[], &cycles, &vault_path);

        let content = std::fs::read_to_string(vault_path.join("a.md")).unwrap();
        assert!(content.contains("## Cycles"));
        assert!(content.contains("[[a]]"));
        assert!(content.contains("[[b]]"));
    }

    #[test]
    fn obsidian_index_lists_all_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");
        let graph = make_graph();
        let metrics = make_metrics();

        write_obsidian_vault(&graph, &metrics, &[], &[], &vault_path);

        let index = std::fs::read_to_string(vault_path.join("_index.md")).unwrap();
        assert!(index.contains("# Architecture Graph"));
        assert!(index.contains("- [[app.main]]"));
        assert!(index.contains("- [[app.utils]]"));
        assert!(index.contains("Nodes: 2"));
    }

    // -----------------------------------------------------------------------
    // Issue #4 — wiki-link targets must match sanitized filenames
    // -----------------------------------------------------------------------

    #[test]
    fn vault_includes_self_contained_overview_note() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");

        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
        g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));

        let metrics = vec![
            NodeMetrics {
                id: "a".to_string(),
                score: 0.9,
                in_degree: 1,
                in_cycle: true,
                ..Default::default()
            },
            NodeMetrics {
                id: "b".to_string(),
                score: 0.3,
                in_degree: 1,
                in_cycle: true,
                ..Default::default()
            },
        ];
        let communities = vec![Community {
            id: 0,
            members: vec!["a".to_string(), "b".to_string()],
        }];
        let cycles = vec![vec!["a".to_string(), "b".to_string()]];

        write_obsidian_vault(&g, &metrics, &communities, &cycles, &vault_path);

        let overview_path = vault_path.join("00-architecture-overview.md");
        assert!(overview_path.exists(), "overview note must exist");
        let content = std::fs::read_to_string(&overview_path).unwrap();
        assert!(content.contains("tags: [graphify, architecture, overview]"));
        assert!(content.contains("# Architecture Overview"));
        assert!(content.contains("## Top Hotspots"));
        // Hotspot rows are wiki-linked for navigation.
        assert!(content.contains("[[a]]"));
        assert!(content.contains("[[b]]"));
        assert!(content.contains("## Communities"));
        assert!(content.contains("### Community 0 (2 members)"));
        assert!(content.contains("## Circular Dependencies"));
        assert!(content.contains("[[a]] → [[b]]"));
    }

    #[test]
    fn wikilinks_and_filenames_use_underscore_for_slash() {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault");

        let mut g = CodeGraph::new();
        g.add_node(Node::module(
            "@scope/pkg.a",
            "packages/a.ts",
            Language::TypeScript,
            1,
            true,
        ));
        g.add_node(Node::module(
            "@scope/pkg.b",
            "packages/b.ts",
            Language::TypeScript,
            1,
            true,
        ));
        g.add_edge("@scope/pkg.a", "@scope/pkg.b", Edge::imports(1));

        let metrics = vec![
            NodeMetrics {
                id: "@scope/pkg.a".to_string(),
                ..Default::default()
            },
            NodeMetrics {
                id: "@scope/pkg.b".to_string(),
                ..Default::default()
            },
        ];
        let cycles = vec![vec!["@scope/pkg.a".to_string(), "@scope/pkg.b".to_string()]];

        write_obsidian_vault(&g, &metrics, &[], &cycles, &vault_path);

        // Filenames are underscore-normalized.
        assert!(vault_path.join("@scope_pkg.a.md").exists());
        assert!(vault_path.join("@scope_pkg.b.md").exists());

        // Wiki-links in _index.md use underscores (no slashes), so Obsidian
        // resolves them to the flat filenames above.
        let index = std::fs::read_to_string(vault_path.join("_index.md")).unwrap();
        assert!(index.contains("[[@scope_pkg.a]]"));
        assert!(index.contains("[[@scope_pkg.b]]"));
        assert!(
            !index.contains("[[@scope/pkg."),
            "wiki-links must not contain '/', got: {index}"
        );

        // Cross-node wiki-links (Imports / Imported By / Cycles) do the same.
        let a = std::fs::read_to_string(vault_path.join("@scope_pkg.a.md")).unwrap();
        assert!(a.contains("[[@scope_pkg.b]]"));
        assert!(
            !a.contains("[[@scope/pkg.b]]"),
            "wiki-link to sibling must not contain '/', got: {a}"
        );
    }
}
