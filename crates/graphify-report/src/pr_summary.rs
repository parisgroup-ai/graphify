//! PR Summary renderer.
//!
//! Produces a concise Markdown summary of architectural change for a single
//! project's Graphify output directory. Pure function: no I/O. Consumers are
//! expected to load inputs separately and pass them in as structs.

use graphify_core::diff::{AnalysisSnapshot, DiffReport};

use crate::check_report::CheckReport;

/// Max rows emitted per bulleted list in the Drift in this PR section before
/// an "…and N more" overflow hint takes over.
const MAX_ROWS_PER_LIST: usize = 5;

/// Render a PR summary Markdown string.
///
/// * `project_name` — resolved project name (caller provides; in the CLI,
///   this is the basename of the output directory).
/// * `analysis` — required; yields the header stats line.
/// * `drift` — optional; produces the "Drift in this PR" section. When `None`,
///   a hint line directs the reader to run `graphify diff`.
/// * `check` — optional; produces the "Outstanding issues" section from its
///   project violations and embedded contract result.
pub fn render(
    project_name: &str,
    analysis: &AnalysisSnapshot,
    drift: Option<&DiffReport>,
    check: Option<&CheckReport>,
) -> String {
    let mut out = String::new();
    render_header(&mut out, project_name);
    render_stats_line(&mut out, analysis, drift);
    render_drift_section(&mut out, drift);
    render_outstanding_section(&mut out, check);
    render_footer(&mut out);
    out
}

fn render_header(out: &mut String, project_name: &str) {
    out.push_str(&format!(
        "### Graphify — Architecture Delta for `{}`\n\n",
        project_name
    ));
}

fn render_stats_line(out: &mut String, analysis: &AnalysisSnapshot, drift: Option<&DiffReport>) {
    match drift {
        Some(d) => {
            let nb = d.summary_delta.nodes.before;
            let na = d.summary_delta.nodes.after;
            let eb = d.summary_delta.edges.before;
            let ea = d.summary_delta.edges.after;
            out.push_str(&format!(
                "{} → {} nodes ({:+}) · {} → {} edges ({:+})\n\n",
                nb, na, na as i64 - nb as i64, eb, ea, ea as i64 - eb as i64,
            ));
        }
        None => {
            out.push_str(&format!(
                "{} nodes · {} edges\n\n",
                analysis.summary.total_nodes, analysis.summary.total_edges,
            ));
        }
    }
}

fn render_drift_section(out: &mut String, drift: Option<&DiffReport>) {
    let Some(drift) = drift else { return; };
    // Collect any-finding flag
    let has_any_drift = !drift.cycles.introduced.is_empty()
        || !drift.cycles.resolved.is_empty()
        || !drift.hotspots.rising.is_empty()
        || !drift.hotspots.new_hotspots.is_empty()
        || !drift.communities.moved_nodes.is_empty();

    out.push_str("#### Drift in this PR\n\n");
    if !has_any_drift {
        out.push_str("_No architectural changes vs baseline._\n\n");
        return;
    }

    render_cycle_rows(out, drift);
    render_hotspot_rows(out, drift);
    render_community_shift_row(out, drift);
}

fn render_community_shift_row(out: &mut String, drift: &DiffReport) {
    let moved = &drift.communities.moved_nodes;
    if moved.is_empty() {
        return;
    }
    let communities_before = drift.summary_delta.communities.before;
    let communities_after = drift.summary_delta.communities.after;
    out.push_str(&format!(
        "- **Community shift** — {} node{} moved across community boundaries (communities: {} → {})\n",
        moved.len(),
        if moved.len() == 1 { "" } else { "s" },
        communities_before,
        communities_after,
    ));
}

fn render_hotspot_rows(out: &mut String, drift: &DiffReport) {
    if !drift.hotspots.rising.is_empty() {
        out.push_str(&format!(
            "- **Escalated hotspots ({})**\n",
            drift.hotspots.rising.len()
        ));
        for change in drift.hotspots.rising.iter().take(MAX_ROWS_PER_LIST) {
            out.push_str(&format!(
                "  - `{}` ({:.2} → {:.2})  `→ graphify explain {}`\n",
                change.id, change.before, change.after, change.id
            ));
        }
        if drift.hotspots.rising.len() > MAX_ROWS_PER_LIST {
            let extra = drift.hotspots.rising.len() - MAX_ROWS_PER_LIST;
            out.push_str(&format!(
                "  _…and {} more (see drift-report.md)_\n",
                extra
            ));
        }
    }

    if !drift.hotspots.new_hotspots.is_empty() {
        out.push_str(&format!(
            "- **New hotspots ({})**\n",
            drift.hotspots.new_hotspots.len()
        ));
        for change in drift.hotspots.new_hotspots.iter().take(MAX_ROWS_PER_LIST) {
            out.push_str(&format!(
                "  - `{}` (score {:.2})  `→ graphify explain {}`\n",
                change.id, change.after, change.id
            ));
        }
        if drift.hotspots.new_hotspots.len() > MAX_ROWS_PER_LIST {
            let extra = drift.hotspots.new_hotspots.len() - MAX_ROWS_PER_LIST;
            out.push_str(&format!(
                "  _…and {} more (see drift-report.md)_\n",
                extra
            ));
        }
    }
}

fn render_cycle_rows(out: &mut String, drift: &DiffReport) {
    for cycle in drift.cycles.introduced.iter().take(MAX_ROWS_PER_LIST) {
        let pair = cycle_pair_label(cycle);
        out.push_str(&format!("- **New cycle** — {}\n", pair));
        if let Some((a, b)) = cycle_first_pair(cycle) {
            out.push_str(&format!("  `→ graphify path {} {}`\n", a, b));
        }
    }
    if drift.cycles.introduced.len() > MAX_ROWS_PER_LIST {
        let extra = drift.cycles.introduced.len() - MAX_ROWS_PER_LIST;
        out.push_str(&format!(
            "  _…and {} more (see drift-report.md)_\n",
            extra
        ));
    }

    for cycle in drift.cycles.resolved.iter().take(MAX_ROWS_PER_LIST) {
        let pair = cycle_pair_label(cycle);
        out.push_str(&format!("- **Broken cycle** — {}\n", pair));
    }
    if drift.cycles.resolved.len() > MAX_ROWS_PER_LIST {
        let extra = drift.cycles.resolved.len() - MAX_ROWS_PER_LIST;
        out.push_str(&format!(
            "  _…and {} more (see drift-report.md)_\n",
            extra
        ));
    }
}

fn cycle_pair_label(cycle: &[String]) -> String {
    // A cycle is a list of nodes; show the first two joined with ↔, or
    // fall back to "↔"-joining all nodes if the cycle has <2 members.
    match cycle.len() {
        0 => "(empty cycle)".to_string(),
        1 => format!("`{}` ↔ `{}`", cycle[0], cycle[0]),
        _ => format!("`{}` ↔ `{}`", cycle[0], cycle[1]),
    }
}

fn cycle_first_pair(cycle: &[String]) -> Option<(&str, &str)> {
    if cycle.len() >= 2 {
        Some((&cycle[0], &cycle[1]))
    } else {
        None
    }
}

fn render_outstanding_section(_out: &mut String, _check: Option<&CheckReport>) {
    // Task 11+ implements this.
}

fn render_footer(out: &mut String) {
    out.push_str(&format!(
        "\n<sub>Graphify v{} · `graphify pr-summary <dir>` to regenerate</sub>\n",
        env!("CARGO_PKG_VERSION")
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Concrete minimal `AnalysisSnapshot` builder for tests.
    /// JSON literal matches the real snapshot shape exactly.
    pub(super) fn minimal_analysis() -> AnalysisSnapshot {
        let json = r#"{
            "nodes": [],
            "communities": [],
            "cycles": [],
            "summary": {
                "total_nodes": 0,
                "total_edges": 0,
                "total_communities": 0,
                "total_cycles": 0
            }
        }"#;
        serde_json::from_str(json).expect("minimal snapshot")
    }

    /// Analysis snapshot with specific node/edge counts (used by later tests).
    pub(super) fn analysis_with_counts(nodes: usize, edges: usize) -> AnalysisSnapshot {
        let json = format!(
            r#"{{
                "nodes": [],
                "communities": [],
                "cycles": [],
                "summary": {{
                    "total_nodes": {},
                    "total_edges": {},
                    "total_communities": 0,
                    "total_cycles": 0
                }}
            }}"#,
            nodes, edges
        );
        serde_json::from_str(&json).expect("sized snapshot")
    }

    fn drift_with_cycles(introduced: Vec<Vec<&str>>, resolved: Vec<Vec<&str>>) -> DiffReport {
        use graphify_core::diff::{CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta};
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta { before: 0, after: 0, change: 0 },
                edges: Delta { before: 0, after: 0, change: 0 },
                communities: Delta { before: 0, after: 0, change: 0 },
                cycles: Delta { before: 0, after: 0, change: 0 },
            },
            edges: EdgeDiff { added_nodes: vec![], removed_nodes: vec![], degree_changes: vec![] },
            cycles: CycleDiff {
                introduced: introduced.into_iter().map(|c| c.into_iter().map(String::from).collect()).collect(),
                resolved: resolved.into_iter().map(|c| c.into_iter().map(String::from).collect()).collect(),
            },
            hotspots: HotspotDiff { rising: vec![], falling: vec![], new_hotspots: vec![], removed_hotspots: vec![] },
            communities: CommunityDiff { moved_nodes: vec![], stable_count: 0 },
        }
    }

    #[test]
    fn renders_header_with_project_name() {
        let a = minimal_analysis();
        let out = render("my-app", &a, None, None);
        assert!(out.contains("### Graphify — Architecture Delta for `my-app`"));
    }

    #[test]
    fn renders_footer_with_version() {
        let a = minimal_analysis();
        let out = render("my-app", &a, None, None);
        assert!(out.contains(&format!("Graphify v{}", env!("CARGO_PKG_VERSION"))));
        assert!(out.contains("`graphify pr-summary <dir>` to regenerate"));
    }

    #[test]
    fn renders_stats_line_without_drift_shows_absolute_counts() {
        let a = analysis_with_counts(142, 301);
        let out = render("my-app", &a, None, None);
        assert!(out.contains("142 nodes · 301 edges"));
    }

    #[test]
    fn renders_new_cycle_rows_with_path_hint() {
        let a = minimal_analysis();
        let d = drift_with_cycles(
            vec![vec!["app.services.auth", "app.repositories.user"]],
            vec![],
        );
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("#### Drift in this PR"));
        assert!(out.contains("**New cycle**"));
        assert!(out.contains("`app.services.auth`"));
        assert!(out.contains("`app.repositories.user`"));
        assert!(out.contains("`→ graphify path app.services.auth app.repositories.user`"));
    }

    #[test]
    fn renders_broken_cycle_rows_without_next_step_hint() {
        let a = minimal_analysis();
        let d = drift_with_cycles(
            vec![],
            vec![vec!["app.a", "app.b"]],
        );
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**Broken cycle**"));
        assert!(out.contains("`app.a`"));
        assert!(out.contains("`app.b`"));
        // Broken cycles are already resolved; no investigation hint needed.
        assert!(!out.contains("graphify path app.a app.b"));
    }

    #[test]
    fn omits_cycle_rows_when_no_cycles_changed() {
        let a = minimal_analysis();
        let d = drift_with_cycles(vec![], vec![]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(!out.contains("**New cycle**"));
        assert!(!out.contains("**Broken cycle**"));
    }

    fn drift_with_hotspots(rising: Vec<(&str, f64, f64)>, new_hotspots: Vec<(&str, f64)>) -> DiffReport {
        use graphify_core::diff::{CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, ScoreChange, SummaryDelta};
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta { before: 0, after: 0, change: 0 },
                edges: Delta { before: 0, after: 0, change: 0 },
                communities: Delta { before: 0, after: 0, change: 0 },
                cycles: Delta { before: 0, after: 0, change: 0 },
            },
            edges: EdgeDiff { added_nodes: vec![], removed_nodes: vec![], degree_changes: vec![] },
            cycles: CycleDiff { introduced: vec![], resolved: vec![] },
            hotspots: HotspotDiff {
                rising: rising.into_iter().map(|(id, before, after)| ScoreChange {
                    id: id.into(), before, after, delta: after - before,
                }).collect(),
                falling: vec![],
                new_hotspots: new_hotspots.into_iter().map(|(id, after)| ScoreChange {
                    id: id.into(), before: 0.0, after, delta: after,
                }).collect(),
                removed_hotspots: vec![],
            },
            communities: CommunityDiff { moved_nodes: vec![], stable_count: 0 },
        }
    }

    #[test]
    fn renders_escalated_hotspots_with_explain_hint() {
        let a = minimal_analysis();
        let d = drift_with_hotspots(
            vec![("app.services.auth", 0.71, 0.83), ("app.api.routes", 0.48, 0.52)],
            vec![],
        );
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**Escalated hotspots (2)**"));
        assert!(out.contains("`app.services.auth`"));
        assert!(out.contains("0.71"));
        assert!(out.contains("0.83"));
        assert!(out.contains("`→ graphify explain app.services.auth`"));
        assert!(out.contains("`app.api.routes`"));
    }

    #[test]
    fn renders_new_hotspots_with_explain_hint() {
        let a = minimal_analysis();
        let d = drift_with_hotspots(
            vec![],
            vec![("app.core.new_mod", 0.66)],
        );
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**New hotspots (1)**"));
        assert!(out.contains("`app.core.new_mod`"));
        assert!(out.contains("score 0.66"));
        assert!(out.contains("`→ graphify explain app.core.new_mod`"));
    }

    #[test]
    fn caps_escalated_hotspots_at_5_rows_with_more_hint() {
        let a = minimal_analysis();
        let rising: Vec<(&str, f64, f64)> = (0..7)
            .map(|i| (Box::leak(format!("app.mod_{}", i).into_boxed_str()) as &str, 0.5, 0.7))
            .collect();
        let d = drift_with_hotspots(rising, vec![]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**Escalated hotspots (7)**"));
        // First 5 shown, then "…and 2 more"
        assert!(out.contains("app.mod_0"));
        assert!(out.contains("app.mod_4"));
        assert!(!out.contains("app.mod_5")); // 6th and 7th hidden
        assert!(out.contains("_…and 2 more"));
    }

    fn drift_with_community_moves(moves: Vec<(&str, usize, usize)>) -> DiffReport {
        use graphify_core::diff::{CommunityDiff, CommunityMove, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta};
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta { before: 0, after: 0, change: 0 },
                edges: Delta { before: 0, after: 0, change: 0 },
                communities: Delta { before: 1, after: 2, change: 1 },
                cycles: Delta { before: 0, after: 0, change: 0 },
            },
            edges: EdgeDiff { added_nodes: vec![], removed_nodes: vec![], degree_changes: vec![] },
            cycles: CycleDiff { introduced: vec![], resolved: vec![] },
            hotspots: HotspotDiff { rising: vec![], falling: vec![], new_hotspots: vec![], removed_hotspots: vec![] },
            communities: CommunityDiff {
                moved_nodes: moves.into_iter().map(|(id, from_c, to_c)| CommunityMove {
                    id: id.into(), from_community: from_c, to_community: to_c,
                }).collect(),
                stable_count: 0,
            },
        }
    }

    #[test]
    fn renders_community_shift_row_when_nodes_moved() {
        let a = minimal_analysis();
        let d = drift_with_community_moves(vec![
            ("app.services.auth", 0, 1),
            ("app.services.user", 0, 1),
        ]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**Community shift**"));
        assert!(out.contains("2 nodes moved across community boundaries"));
        assert!(out.contains("communities: 1 → 2"));
    }

    #[test]
    fn renders_singular_community_shift_row_when_one_node_moved() {
        let a = minimal_analysis();
        let d = drift_with_community_moves(vec![("app.services.auth", 0, 1)]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**Community shift**"));
        assert!(out.contains("1 node moved"));
        // Guard against accidental pluralization
        assert!(!out.contains("1 nodes moved"));
    }

    #[test]
    fn renders_no_changes_message_when_drift_empty() {
        let a = minimal_analysis();
        let d = drift_with_community_moves(vec![]);  // also no cycles, no hotspots
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("#### Drift in this PR"));
        assert!(out.contains("_No architectural changes vs baseline._"));
        // Bullet headers should not appear when there is nothing
        assert!(!out.contains("**New cycle**"));
        assert!(!out.contains("**Escalated hotspots"));
    }
}
