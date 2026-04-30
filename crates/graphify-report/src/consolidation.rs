//! Consolidation report renderer.
//!
//! Produces a per-project (and optional cross-project) listing of symbols whose
//! leaf name is shared by multiple nodes — the raw candidates for a
//! consolidation refactor. The grouping logic is a pure function over
//! `AnalysisSnapshot` + a minimal view of `graph.json` + a compiled
//! [`ConsolidationConfig`]; no I/O happens in this module. Callers are expected
//! to load those inputs separately and pass them in.
//!
//! The JSON shape is intentionally additive:
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "project": "my-service",
//!   "generated_at": "2026-04-18T10:00:00Z",
//!   "allowlist_applied": 3,
//!   "candidates": [
//!     {
//!       "leaf_name": "TokenUsage",
//!       "group_size": 2,
//!       "members": [
//!         {"id": "app.a.TokenUsage", "kind": "class", "fan_in": 5,
//!          "file": "app/a.py", "line": 10, "alternative_paths": []}
//!       ],
//!       "allowlisted": false
//!     }
//!   ]
//! }
//! ```
//!
//! `alternative_paths` on each member is always present (empty by default) so
//! FEAT-021 can populate it without a schema bump.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use graphify_core::consolidation::{leaf_symbol, ConsolidationConfig};
use graphify_core::diff::AnalysisSnapshot;

use crate::time_utils::now_iso8601;

/// Current wire-format version of the consolidation report.
pub const SCHEMA_VERSION: u32 = 1;

/// Minimal view of `graph.json` nodes — only the fields we need to annotate a
/// candidate member. Additional fields on disk are ignored by serde.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub line: usize,
}

/// Minimal view of the top-level `graph.json` payload.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphSnapshot {
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
}

/// Per-project consolidation report.
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationReport {
    pub schema_version: u32,
    pub project: String,
    pub generated_at: String,
    /// Number of candidate groups dropped because their leaf name matched the
    /// allowlist. Always `0` when no allowlist is configured (or when
    /// `--ignore-allowlist` was passed).
    pub allowlist_applied: usize,
    pub candidates: Vec<Candidate>,
}

/// Aggregate consolidation report across 2+ projects.
///
/// Groupings are computed on leaf symbol name only (deliberately simple — any
/// semantic-equivalence logic belongs to FEAT-023).
#[derive(Debug, Clone, Serialize)]
pub struct AggregateReport {
    pub schema_version: u32,
    pub generated_at: String,
    pub allowlist_applied: usize,
    pub candidates: Vec<AggregateCandidate>,
}

/// One candidate group in a per-project report.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub leaf_name: String,
    pub group_size: usize,
    pub members: Vec<Member>,
    /// True when the allowlist matched this leaf. Candidates with
    /// `allowlisted: true` only appear in output when the caller passed
    /// `--ignore-allowlist`; otherwise they are filtered out upstream.
    pub allowlisted: bool,
}

/// One candidate group in the cross-project aggregate. Members carry a
/// `project` tag; everything else mirrors [`Member`].
#[derive(Debug, Clone, Serialize)]
pub struct AggregateCandidate {
    pub leaf_name: String,
    pub group_size: usize,
    /// Count of distinct projects touched by this leaf. Aggregate reports
    /// usually only list candidates with `project_count >= 2`.
    pub project_count: usize,
    pub members: Vec<AggregateMember>,
    pub allowlisted: bool,
}

/// One node in a per-project candidate group.
#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub id: String,
    pub kind: String,
    pub fan_in: usize,
    pub file: String,
    pub line: usize,
    /// Reserved for FEAT-021. Always serialized (empty by default) so the
    /// schema stays stable when that feature fills it in.
    pub alternative_paths: Vec<String>,
}

/// One node in a cross-project candidate group.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateMember {
    pub project: String,
    pub id: String,
    pub kind: String,
    pub fan_in: usize,
    pub file: String,
    pub line: usize,
    pub alternative_paths: Vec<String>,
}

/// Options controlling candidate selection.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Minimum group size. Groups with fewer members are dropped. Defaults to 2.
    pub min_group_size: usize,
    /// When `true`, allowlist hits are *retained* with `allowlisted: true`
    /// (debug mode). When `false`, they are dropped from output and counted in
    /// `allowlist_applied`.
    pub ignore_allowlist: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            min_group_size: 2,
            ignore_allowlist: false,
        }
    }
}

/// Renders a per-project consolidation report.
///
/// * `project_name` — caller-supplied (typically from the config entry).
/// * `analysis` — parsed `analysis.json` for the project. Supplies node ids and
///   fan-in.
/// * `graph` — parsed `graph.json` for the project. Supplies kind, file_path,
///   and line per node. May be empty — members get stub values.
/// * `allowlist` — compiled consolidation config. `is_empty()` = no filtering.
/// * `opts` — grouping knobs.
pub fn render(
    project_name: &str,
    analysis: &AnalysisSnapshot,
    graph: &GraphSnapshot,
    allowlist: &ConsolidationConfig,
    opts: RenderOptions,
) -> ConsolidationReport {
    let graph_lookup = build_graph_lookup(graph);
    let mut groups: BTreeMap<String, Vec<Member>> = BTreeMap::new();

    for node in &analysis.nodes {
        let leaf = leaf_symbol(&node.id).to_string();
        let entry = groups.entry(leaf).or_default();
        let gnode = graph_lookup.get(node.id.as_str());
        entry.push(Member {
            id: node.id.clone(),
            kind: gnode.map(|g| g.kind.clone()).unwrap_or_default(),
            fan_in: node.in_degree,
            file: gnode.map(|g| g.file_path.clone()).unwrap_or_default(),
            line: gnode.map(|g| g.line).unwrap_or(0),
            alternative_paths: Vec::new(),
        });
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut allowlist_applied = 0usize;
    for (leaf, mut members) in groups {
        if members.len() < opts.min_group_size {
            continue;
        }
        // Stable order within a group: highest fan_in first, then id ascending.
        members.sort_by(|a, b| b.fan_in.cmp(&a.fan_in).then_with(|| a.id.cmp(&b.id)));
        let is_allowlisted = allowlist.matches(&leaf);
        if is_allowlisted {
            allowlist_applied += 1;
            if !opts.ignore_allowlist {
                continue;
            }
        }
        candidates.push(Candidate {
            leaf_name: leaf,
            group_size: members.len(),
            members,
            allowlisted: is_allowlisted,
        });
    }

    // Deterministic output ordering: largest group first, then leaf name.
    candidates.sort_by(|a, b| {
        b.group_size
            .cmp(&a.group_size)
            .then_with(|| a.leaf_name.cmp(&b.leaf_name))
    });

    ConsolidationReport {
        schema_version: SCHEMA_VERSION,
        project: project_name.to_string(),
        generated_at: now_iso8601(),
        allowlist_applied,
        candidates,
    }
}

/// One `(project, analysis, graph)` triple for aggregate rendering.
pub struct ProjectInput<'a> {
    pub name: &'a str,
    pub analysis: &'a AnalysisSnapshot,
    pub graph: &'a GraphSnapshot,
}

/// Renders the cross-project aggregate. Only groups whose members span 2+
/// projects are emitted — per-project duplication is captured by the
/// per-project reports.
pub fn render_aggregate(
    projects: &[ProjectInput<'_>],
    allowlist: &ConsolidationConfig,
    opts: RenderOptions,
) -> AggregateReport {
    let mut groups: BTreeMap<String, Vec<AggregateMember>> = BTreeMap::new();
    for p in projects {
        let graph_lookup = build_graph_lookup(p.graph);
        for node in &p.analysis.nodes {
            let leaf = leaf_symbol(&node.id).to_string();
            let entry = groups.entry(leaf).or_default();
            let gnode = graph_lookup.get(node.id.as_str());
            entry.push(AggregateMember {
                project: p.name.to_string(),
                id: node.id.clone(),
                kind: gnode.map(|g| g.kind.clone()).unwrap_or_default(),
                fan_in: node.in_degree,
                file: gnode.map(|g| g.file_path.clone()).unwrap_or_default(),
                line: gnode.map(|g| g.line).unwrap_or(0),
                alternative_paths: Vec::new(),
            });
        }
    }

    let mut candidates: Vec<AggregateCandidate> = Vec::new();
    let mut allowlist_applied = 0usize;
    for (leaf, mut members) in groups {
        let project_count = count_distinct_projects(&members);
        if project_count < 2 {
            continue;
        }
        if members.len() < opts.min_group_size {
            continue;
        }
        members.sort_by(|a, b| {
            b.fan_in
                .cmp(&a.fan_in)
                .then_with(|| a.project.cmp(&b.project))
                .then_with(|| a.id.cmp(&b.id))
        });
        let is_allowlisted = allowlist.matches(&leaf);
        if is_allowlisted {
            allowlist_applied += 1;
            if !opts.ignore_allowlist {
                continue;
            }
        }
        candidates.push(AggregateCandidate {
            leaf_name: leaf,
            group_size: members.len(),
            project_count,
            members,
            allowlisted: is_allowlisted,
        });
    }

    candidates.sort_by(|a, b| {
        b.project_count
            .cmp(&a.project_count)
            .then_with(|| b.group_size.cmp(&a.group_size))
            .then_with(|| a.leaf_name.cmp(&b.leaf_name))
    });

    AggregateReport {
        schema_version: SCHEMA_VERSION,
        generated_at: now_iso8601(),
        allowlist_applied,
        candidates,
    }
}

/// Minimal Markdown renderer for per-project reports. One table listing each
/// candidate's leaf, size, and member ids — sufficient for human eyeballing
/// without competing with the full `architecture_report.md`.
pub fn render_markdown(report: &ConsolidationReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Consolidation Candidates — `{}`\n\n",
        report.project
    ));
    out.push_str(&format!("_Generated at {}._\n\n", report.generated_at));
    out.push_str(&format!(
        "- **Candidates:** {}\n- **Allowlist hits filtered:** {}\n\n",
        report.candidates.len(),
        report.allowlist_applied
    ));

    if report.candidates.is_empty() {
        out.push_str("_No candidate groups meet the current `min_group_size`._\n");
        return out;
    }

    out.push_str("| Leaf | Size | Members |\n");
    out.push_str("|------|-----:|---------|\n");
    for c in &report.candidates {
        let members = c
            .members
            .iter()
            .map(|m| format!("`{}` (fan_in={})", m.id, m.fan_in))
            .collect::<Vec<_>>()
            .join("<br>");
        let leaf_cell = if c.allowlisted {
            format!("`{}` _(allowlisted)_", c.leaf_name)
        } else {
            format!("`{}`", c.leaf_name)
        };
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            leaf_cell, c.group_size, members
        ));
    }

    out
}

/// Minimal Markdown renderer for the cross-project aggregate.
pub fn render_aggregate_markdown(report: &AggregateReport) -> String {
    let mut out = String::new();
    out.push_str("# Consolidation Candidates — Cross-Project Aggregate\n\n");
    out.push_str(&format!("_Generated at {}._\n\n", report.generated_at));
    out.push_str(&format!(
        "- **Candidates:** {}\n- **Allowlist hits filtered:** {}\n\n",
        report.candidates.len(),
        report.allowlist_applied
    ));

    if report.candidates.is_empty() {
        out.push_str("_No cross-project candidate groups found._\n");
        return out;
    }

    out.push_str("| Leaf | Projects | Size | Members |\n");
    out.push_str("|------|---------:|-----:|---------|\n");
    for c in &report.candidates {
        let members = c
            .members
            .iter()
            .map(|m| format!("`{}:{}` (fan_in={})", m.project, m.id, m.fan_in))
            .collect::<Vec<_>>()
            .join("<br>");
        let leaf_cell = if c.allowlisted {
            format!("`{}` _(allowlisted)_", c.leaf_name)
        } else {
            format!("`{}`", c.leaf_name)
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            leaf_cell, c.project_count, c.group_size, members
        ));
    }

    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_graph_lookup(graph: &GraphSnapshot) -> std::collections::HashMap<&str, &GraphNode> {
    graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect()
}

fn count_distinct_projects(members: &[AggregateMember]) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for m in members {
        seen.insert(m.project.as_str());
    }
    seen.len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::consolidation::{ConsolidationConfig, ConsolidationConfigRaw};
    use graphify_core::diff::{AnalysisSnapshot, NodeSnapshot, SummarySnapshot};

    fn empty_allowlist() -> ConsolidationConfig {
        ConsolidationConfig::default()
    }

    fn make_analysis(node_ids: &[&str]) -> AnalysisSnapshot {
        AnalysisSnapshot {
            generated_at: None,
            nodes: node_ids
                .iter()
                .map(|id| NodeSnapshot {
                    id: id.to_string(),
                    betweenness: 0.0,
                    pagerank: 0.0,
                    in_degree: 0,
                    out_degree: 0,
                    in_cycle: false,
                    score: 0.0,
                    community_id: 0,
                    hotspot_type: None,
                })
                .collect(),
            communities: vec![],
            cycles: vec![],
            summary: SummarySnapshot {
                total_nodes: node_ids.len(),
                total_edges: 0,
                total_communities: 0,
                total_cycles: 0,
            },
            allowlisted_symbols: None,
            edges: vec![],
        }
    }

    fn empty_graph() -> GraphSnapshot {
        GraphSnapshot { nodes: vec![] }
    }

    #[test]
    fn groups_by_leaf_symbol() {
        let analysis = make_analysis(&["app.a.TokenUsage", "app.b.TokenUsage", "app.a.Other"]);
        let report = render(
            "svc",
            &analysis,
            &empty_graph(),
            &empty_allowlist(),
            RenderOptions::default(),
        );
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].leaf_name, "TokenUsage");
        assert_eq!(report.candidates[0].group_size, 2);
        assert_eq!(report.allowlist_applied, 0);
    }

    #[test]
    fn singletons_are_dropped_by_default() {
        let analysis = make_analysis(&["app.a.OnlyOne"]);
        let report = render(
            "svc",
            &analysis,
            &empty_graph(),
            &empty_allowlist(),
            RenderOptions::default(),
        );
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn min_group_size_filters() {
        let analysis = make_analysis(&[
            "app.a.TokenUsage",
            "app.b.TokenUsage",
            "app.a.Response",
            "app.b.Response",
            "app.c.Response",
        ]);
        let opts = RenderOptions {
            min_group_size: 3,
            ignore_allowlist: false,
        };
        let report = render("svc", &analysis, &empty_graph(), &empty_allowlist(), opts);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].leaf_name, "Response");
    }

    #[test]
    fn allowlist_filters_by_default() {
        let analysis = make_analysis(&[
            "app.a.TokenUsage",
            "app.b.TokenUsage",
            "app.a.Real",
            "app.b.Real",
        ]);
        let allowlist = ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec!["TokenUsage".into()],
            ..Default::default()
        })
        .unwrap();
        let report = render(
            "svc",
            &analysis,
            &empty_graph(),
            &allowlist,
            RenderOptions::default(),
        );
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].leaf_name, "Real");
        assert_eq!(report.allowlist_applied, 1);
    }

    #[test]
    fn ignore_allowlist_retains_hits() {
        let analysis = make_analysis(&["app.a.TokenUsage", "app.b.TokenUsage"]);
        let allowlist = ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec!["TokenUsage".into()],
            ..Default::default()
        })
        .unwrap();
        let report = render(
            "svc",
            &analysis,
            &empty_graph(),
            &allowlist,
            RenderOptions {
                min_group_size: 2,
                ignore_allowlist: true,
            },
        );
        assert_eq!(report.candidates.len(), 1);
        assert!(report.candidates[0].allowlisted);
        assert_eq!(report.allowlist_applied, 1);
    }

    #[test]
    fn members_include_graph_metadata_when_available() {
        let analysis = make_analysis(&["app.a.TokenUsage", "app.b.TokenUsage"]);
        let graph = GraphSnapshot {
            nodes: vec![
                GraphNode {
                    id: "app.a.TokenUsage".into(),
                    kind: "class".into(),
                    file_path: "a.py".into(),
                    line: 12,
                },
                GraphNode {
                    id: "app.b.TokenUsage".into(),
                    kind: "class".into(),
                    file_path: "b.py".into(),
                    line: 44,
                },
            ],
        };
        let report = render(
            "svc",
            &analysis,
            &graph,
            &empty_allowlist(),
            RenderOptions::default(),
        );
        let members = &report.candidates[0].members;
        assert_eq!(members.len(), 2);
        for m in members {
            assert_eq!(m.kind, "class");
            assert!(!m.file.is_empty());
            assert!(m.alternative_paths.is_empty());
        }
    }

    #[test]
    fn aggregate_only_includes_cross_project_groups() {
        // Project A: two TokenUsage; Project B: one TokenUsage. Aggregate
        // should surface the cross-project group.
        let a = make_analysis(&["app.a.TokenUsage", "app.b.TokenUsage"]);
        let b = make_analysis(&["svc.TokenUsage"]);
        let g = empty_graph();
        let projects = [
            ProjectInput {
                name: "A",
                analysis: &a,
                graph: &g,
            },
            ProjectInput {
                name: "B",
                analysis: &b,
                graph: &g,
            },
        ];
        let agg = render_aggregate(&projects, &empty_allowlist(), RenderOptions::default());
        assert_eq!(agg.candidates.len(), 1);
        assert_eq!(agg.candidates[0].leaf_name, "TokenUsage");
        assert_eq!(agg.candidates[0].project_count, 2);
        assert_eq!(agg.candidates[0].group_size, 3);
    }

    #[test]
    fn aggregate_skips_single_project_groups() {
        let a = make_analysis(&["app.a.LocalOnly", "app.b.LocalOnly"]);
        let b = make_analysis(&["svc.Other"]);
        let g = empty_graph();
        let projects = [
            ProjectInput {
                name: "A",
                analysis: &a,
                graph: &g,
            },
            ProjectInput {
                name: "B",
                analysis: &b,
                graph: &g,
            },
        ];
        let agg = render_aggregate(&projects, &empty_allowlist(), RenderOptions::default());
        assert!(agg.candidates.is_empty());
    }

    #[test]
    fn markdown_renderer_produces_table() {
        let analysis = make_analysis(&["app.a.TokenUsage", "app.b.TokenUsage"]);
        let report = render(
            "svc",
            &analysis,
            &empty_graph(),
            &empty_allowlist(),
            RenderOptions::default(),
        );
        let md = render_markdown(&report);
        assert!(md.contains("# Consolidation Candidates"));
        assert!(md.contains("| Leaf |"));
        assert!(md.contains("TokenUsage"));
    }
}
