//! PR Summary renderer.
//!
//! Produces a concise Markdown summary of architectural change for a single
//! project's Graphify output directory. Pure function: no I/O. Consumers are
//! expected to load inputs separately and pass them in as structs.

use std::collections::HashMap;

use graphify_core::diff::{AnalysisSnapshot, DiffReport};
use graphify_core::metrics::HotspotType;

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
    let type_lookup = build_hotspot_type_lookup(analysis);
    render_header(&mut out, project_name);
    render_stats_line(&mut out, analysis, drift);
    render_drift_section(&mut out, drift, &type_lookup);
    render_outstanding_section(&mut out, check);
    render_footer(&mut out);
    out
}

/// Builds a node-id → classification map from the snapshot's nodes.
/// Returns an empty map for older snapshots that don't carry the field.
fn build_hotspot_type_lookup(analysis: &AnalysisSnapshot) -> HashMap<String, HotspotType> {
    analysis
        .nodes
        .iter()
        .filter_map(|n| n.hotspot_type.map(|t| (n.id.clone(), t)))
        .collect()
}

fn hotspot_type_label(t: HotspotType) -> &'static str {
    match t {
        HotspotType::Hub => "hub",
        HotspotType::Bridge => "bridge",
        HotspotType::Mixed => "mixed",
    }
}

/// Renders ` [hub]` / ` [bridge]` / ` [mixed]` (with leading space) when the
/// id is classified, or an empty string otherwise.
fn type_annotation(id: &str, lookup: &HashMap<String, HotspotType>) -> String {
    match lookup.get(id) {
        Some(t) => format!(" [{}]", hotspot_type_label(*t)),
        None => String::new(),
    }
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
                nb,
                na,
                na as i64 - nb as i64,
                eb,
                ea,
                ea as i64 - eb as i64,
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

fn render_drift_section(
    out: &mut String,
    drift: Option<&DiffReport>,
    type_lookup: &HashMap<String, HotspotType>,
) {
    out.push_str("#### Drift in this PR\n\n");
    let Some(drift) = drift else {
        out.push_str(
            "_No drift baseline — run `graphify diff --baseline <path> --config graphify.toml` to populate._\n\n",
        );
        return;
    };

    let has_any_drift = !drift.cycles.introduced.is_empty()
        || !drift.cycles.resolved.is_empty()
        || !drift.hotspots.rising.is_empty()
        || !drift.hotspots.new_hotspots.is_empty()
        || !drift.communities.moved_nodes.is_empty();

    if !has_any_drift {
        out.push_str("_No architectural changes vs baseline._\n\n");
        return;
    }

    render_cycle_rows(out, drift);
    render_hotspot_rows(out, drift, type_lookup);
    render_community_shift_row(out, drift);
    out.push('\n');
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

fn render_hotspot_rows(
    out: &mut String,
    drift: &DiffReport,
    type_lookup: &HashMap<String, HotspotType>,
) {
    if !drift.hotspots.rising.is_empty() {
        out.push_str(&format!(
            "- **Escalated hotspots ({})**\n",
            drift.hotspots.rising.len()
        ));
        for change in drift.hotspots.rising.iter().take(MAX_ROWS_PER_LIST) {
            out.push_str(&format!(
                "  - `{}`{} ({:.2} → {:.2})  `→ graphify explain {}`\n",
                change.id,
                type_annotation(&change.id, type_lookup),
                change.before,
                change.after,
                change.id,
            ));
        }
        if drift.hotspots.rising.len() > MAX_ROWS_PER_LIST {
            let extra = drift.hotspots.rising.len() - MAX_ROWS_PER_LIST;
            out.push_str(&format!("  _…and {} more (see drift-report.md)_\n", extra));
        }
    }

    if !drift.hotspots.new_hotspots.is_empty() {
        out.push_str(&format!(
            "- **New hotspots ({})**\n",
            drift.hotspots.new_hotspots.len()
        ));
        for change in drift.hotspots.new_hotspots.iter().take(MAX_ROWS_PER_LIST) {
            out.push_str(&format!(
                "  - `{}`{} (score {:.2})  `→ graphify explain {}`\n",
                change.id,
                type_annotation(&change.id, type_lookup),
                change.after,
                change.id,
            ));
        }
        if drift.hotspots.new_hotspots.len() > MAX_ROWS_PER_LIST {
            let extra = drift.hotspots.new_hotspots.len() - MAX_ROWS_PER_LIST;
            out.push_str(&format!("  _…and {} more (see drift-report.md)_\n", extra));
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
        out.push_str(&format!("  _…and {} more (see drift-report.md)_\n", extra));
    }

    for cycle in drift.cycles.resolved.iter().take(MAX_ROWS_PER_LIST) {
        let pair = cycle_pair_label(cycle);
        out.push_str(&format!("- **Broken cycle** — {}\n", pair));
    }
    if drift.cycles.resolved.len() > MAX_ROWS_PER_LIST {
        let extra = drift.cycles.resolved.len() - MAX_ROWS_PER_LIST;
        out.push_str(&format!("  _…and {} more (see drift-report.md)_\n", extra));
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

fn render_outstanding_section(out: &mut String, check: Option<&CheckReport>) {
    let Some(check) = check else {
        return;
    };

    let rule_count: usize = check.projects.iter().map(|p| p.violations.len()).sum();
    let contract_count = check
        .contracts
        .as_ref()
        .map(|c| c.pairs.iter().map(|p| p.violations.len()).sum::<usize>())
        .unwrap_or(0);

    if rule_count == 0 && contract_count == 0 {
        return;
    }

    out.push_str("#### Outstanding issues\n\n");
    render_rules_violations(out, check, rule_count);
    render_contract_violations(out, check, contract_count);
}

fn render_rules_violations(out: &mut String, check: &CheckReport, total_rule_count: usize) {
    use crate::check_report::CheckViolation;

    if total_rule_count == 0 {
        return;
    }

    out.push_str(&format!(
        "**Rules violations ({})** — `graphify check --config graphify.toml`\n",
        total_rule_count
    ));

    let mut shown = 0usize;
    'outer: for project in &check.projects {
        for v in &project.violations {
            if shown >= MAX_ROWS_PER_LIST {
                break 'outer;
            }
            match v {
                CheckViolation::Policy {
                    rule,
                    source_node,
                    target_node,
                    ..
                } => {
                    out.push_str(&format!(
                        "- `{}` — `{}` → `{}`\n",
                        rule, source_node, target_node
                    ));
                }
                CheckViolation::Limit {
                    kind,
                    actual,
                    expected_max,
                    node_id,
                } => match node_id {
                    Some(n) => out.push_str(&format!(
                        "- `{}` — `{}`: {} > {}\n",
                        kind, n, actual, expected_max
                    )),
                    None => {
                        out.push_str(&format!("- `{}` — {} > {}\n", kind, actual, expected_max))
                    }
                },
            }
            shown += 1;
        }
    }

    if total_rule_count > MAX_ROWS_PER_LIST {
        let extra = total_rule_count - MAX_ROWS_PER_LIST;
        out.push_str(&format!("_…and {} more (see check-report.json)_\n", extra));
    }
    out.push('\n');
}

fn render_contract_violations(out: &mut String, check: &CheckReport, total_contract_count: usize) {
    if total_contract_count == 0 {
        return;
    }
    let Some(contracts) = check.contracts.as_ref() else {
        return;
    };

    out.push_str(&format!(
        "**Contract drift ({})** — `graphify check --config graphify.toml`\n",
        total_contract_count
    ));

    let mut shown = 0usize;
    'outer: for pair in &contracts.pairs {
        for entry in &pair.violations {
            if shown >= MAX_ROWS_PER_LIST {
                break 'outer;
            }
            let summary = summarize_contract_violation(&entry.violation);
            out.push_str(&format!(
                "- `{}` (ORM `{}`) ↔ `{}` (TS): {}\n",
                pair.name, pair.orm.symbol, pair.ts.symbol, summary,
            ));
            shown += 1;
        }
    }

    if total_contract_count > MAX_ROWS_PER_LIST {
        let extra = total_contract_count - MAX_ROWS_PER_LIST;
        out.push_str(&format!("_…and {} more (see check-report.json)_\n", extra));
    }
    out.push('\n');
}

fn summarize_contract_violation(v: &graphify_core::contract::ContractViolation) -> String {
    use graphify_core::contract::ContractViolation as V;
    match v {
        V::ContractFieldMissingOnTs { field, .. } => {
            format!("field `{}` missing on TS side", field)
        }
        V::ContractFieldMissingOnOrm { field, .. } => {
            format!("field `{}` missing on ORM side", field)
        }
        V::ContractTypeMismatch { field, .. } => format!("type mismatch on field `{}`", field),
        V::ContractNullabilityMismatch {
            field,
            orm_nullable,
            ts_nullable,
            ..
        } => {
            format!(
                "nullability mismatch on `{}` (ORM={}, TS={})",
                field, orm_nullable, ts_nullable
            )
        }
        V::ContractRelationMissingOnTs { relation, .. } => {
            format!("relation `{}` missing on TS side", relation)
        }
        V::ContractRelationMissingOnOrm { relation, .. } => {
            format!("relation `{}` missing on ORM side", relation)
        }
        V::ContractCardinalityMismatch { relation, .. } => {
            format!("cardinality mismatch on relation `{}`", relation)
        }
        V::ContractUnmappedOrmType {
            field, raw_type, ..
        } => format!("unmapped ORM type `{}` on field `{}`", raw_type, field),
    }
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
        use graphify_core::diff::{
            CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta,
        };
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                edges: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                communities: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                cycles: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
            },
            edges: EdgeDiff {
                added_nodes: vec![],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: introduced
                    .into_iter()
                    .map(|c| c.into_iter().map(String::from).collect())
                    .collect(),
                resolved: resolved
                    .into_iter()
                    .map(|c| c.into_iter().map(String::from).collect())
                    .collect(),
            },
            hotspots: HotspotDiff {
                rising: vec![],
                falling: vec![],
                new_hotspots: vec![],
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: vec![],
                stable_count: 0,
            },
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
        let d = drift_with_cycles(vec![], vec![vec!["app.a", "app.b"]]);
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

    fn drift_with_hotspots(
        rising: Vec<(&str, f64, f64)>,
        new_hotspots: Vec<(&str, f64)>,
    ) -> DiffReport {
        use graphify_core::diff::{
            CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, ScoreChange,
            SummaryDelta,
        };
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                edges: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                communities: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                cycles: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
            },
            edges: EdgeDiff {
                added_nodes: vec![],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![],
                resolved: vec![],
            },
            hotspots: HotspotDiff {
                rising: rising
                    .into_iter()
                    .map(|(id, before, after)| ScoreChange {
                        id: id.into(),
                        before,
                        after,
                        delta: after - before,
                        intentional_mirror: None,
                    })
                    .collect(),
                falling: vec![],
                new_hotspots: new_hotspots
                    .into_iter()
                    .map(|(id, after)| ScoreChange {
                        id: id.into(),
                        before: 0.0,
                        after,
                        delta: after,
                        intentional_mirror: None,
                    })
                    .collect(),
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: vec![],
                stable_count: 0,
            },
        }
    }

    #[test]
    fn renders_escalated_hotspots_with_explain_hint() {
        let a = minimal_analysis();
        let d = drift_with_hotspots(
            vec![
                ("app.services.auth", 0.71, 0.83),
                ("app.api.routes", 0.48, 0.52),
            ],
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
        let d = drift_with_hotspots(vec![], vec![("app.core.new_mod", 0.66)]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("**New hotspots (1)**"));
        assert!(out.contains("`app.core.new_mod`"));
        assert!(out.contains("score 0.66"));
        assert!(out.contains("`→ graphify explain app.core.new_mod`"));
    }

    /// Snapshot helper that injects classified nodes for type-annotation tests.
    fn analysis_with_classified_nodes(nodes: Vec<(&str, &str)>) -> AnalysisSnapshot {
        let entries: Vec<String> = nodes
            .into_iter()
            .map(|(id, ht)| {
                format!(
                    r#"{{"id":"{}","betweenness":0.0,"pagerank":0.0,"in_degree":0,"out_degree":0,"in_cycle":false,"score":0.0,"community_id":0,"hotspot_type":"{}"}}"#,
                    id, ht
                )
            })
            .collect();
        let json = format!(
            r#"{{
                "nodes": [{}],
                "communities": [],
                "cycles": [],
                "summary": {{
                    "total_nodes": 0,
                    "total_edges": 0,
                    "total_communities": 0,
                    "total_cycles": 0
                }}
            }}"#,
            entries.join(",")
        );
        serde_json::from_str(&json).expect("classified snapshot")
    }

    #[test]
    fn annotates_escalated_hotspot_with_classification() {
        let a = analysis_with_classified_nodes(vec![
            ("app.services.auth", "hub"),
            ("app.api.routes", "bridge"),
        ]);
        let d = drift_with_hotspots(
            vec![
                ("app.services.auth", 0.71, 0.83),
                ("app.api.routes", 0.48, 0.52),
            ],
            vec![],
        );
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("`app.services.auth` [hub]"));
        assert!(out.contains("`app.api.routes` [bridge]"));
    }

    #[test]
    fn annotates_new_hotspot_with_classification() {
        let a = analysis_with_classified_nodes(vec![("app.core.new_mod", "mixed")]);
        let d = drift_with_hotspots(vec![], vec![("app.core.new_mod", 0.66)]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("`app.core.new_mod` [mixed]"));
    }

    #[test]
    fn omits_annotation_when_node_not_in_snapshot() {
        // Empty snapshot => no lookup hits => no annotation.
        let a = minimal_analysis();
        let d = drift_with_hotspots(vec![("app.x", 0.3, 0.5)], vec![]);
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("`app.x` (0.30 → 0.50)"));
        assert!(!out.contains("[hub]"));
        assert!(!out.contains("[bridge]"));
        assert!(!out.contains("[mixed]"));
    }

    #[test]
    fn caps_escalated_hotspots_at_5_rows_with_more_hint() {
        let a = minimal_analysis();
        let rising: Vec<(&str, f64, f64)> = (0..7)
            .map(|i| {
                (
                    Box::leak(format!("app.mod_{}", i).into_boxed_str()) as &str,
                    0.5,
                    0.7,
                )
            })
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
        use graphify_core::diff::{
            CommunityDiff, CommunityMove, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff,
            SummaryDelta,
        };
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                edges: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                communities: Delta {
                    before: 1,
                    after: 2,
                    change: 1,
                },
                cycles: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
            },
            edges: EdgeDiff {
                added_nodes: vec![],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![],
                resolved: vec![],
            },
            hotspots: HotspotDiff {
                rising: vec![],
                falling: vec![],
                new_hotspots: vec![],
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: moves
                    .into_iter()
                    .map(|(id, from_c, to_c)| CommunityMove {
                        id: id.into(),
                        from_community: from_c,
                        to_community: to_c,
                    })
                    .collect(),
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
        let d = drift_with_community_moves(vec![]); // also no cycles, no hotspots
        let out = render("my-app", &a, Some(&d), None);
        assert!(out.contains("#### Drift in this PR"));
        assert!(out.contains("_No architectural changes vs baseline._"));
        // Bullet headers should not appear when there is nothing
        assert!(!out.contains("**New cycle**"));
        assert!(!out.contains("**Escalated hotspots"));
    }

    #[test]
    fn renders_missing_drift_hint_when_drift_is_none() {
        let a = minimal_analysis();
        let out = render("my-app", &a, None, None);
        assert!(out.contains("#### Drift in this PR"));
        assert!(out.contains("_No drift baseline"));
        assert!(out.contains("graphify diff"));
        // No empty bullets / section-complete message
        assert!(!out.contains("_No architectural changes vs baseline._"));
    }

    fn check_with_rule_violations(violations: Vec<(&str, &str, &str)>) -> CheckReport {
        use crate::check_report::{
            CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
            ProjectCheckSummary,
        };
        CheckReport {
            ok: false,
            violations: violations.len(),
            projects: vec![ProjectCheckResult {
                name: "my-app".into(),
                ok: false,
                summary: ProjectCheckSummary {
                    nodes: 0,
                    edges: 0,
                    communities: 0,
                    cycles: 0,
                    max_hotspot_score: 0.0,
                    max_hotspot_id: None,
                },
                limits: CheckLimits::default(),
                policy_summary: PolicyCheckSummary {
                    rules_evaluated: violations.len(),
                    policy_violations: violations.len(),
                },
                violations: violations
                    .into_iter()
                    .map(|(rule, src, tgt)| CheckViolation::Policy {
                        kind: "policy_rule".into(),
                        rule: rule.into(),
                        source_node: src.into(),
                        target_node: tgt.into(),
                        source_project: "my-app".into(),
                        target_project: "my-app".into(),
                        source_selectors: vec![],
                        target_selectors: vec![],
                    })
                    .collect(),
            }],
            contracts: None,
        }
    }

    #[test]
    fn renders_rules_violations_subsection() {
        let a = minimal_analysis();
        let c = check_with_rule_violations(vec![(
            "no_cross_layer_imports",
            "app.api.routes",
            "app.repositories.user",
        )]);
        let out = render("my-app", &a, None, Some(&c));
        assert!(out.contains("#### Outstanding issues"));
        assert!(out.contains("**Rules violations (1)**"));
        assert!(out.contains("`no_cross_layer_imports`"));
        assert!(out.contains("`app.api.routes`"));
        assert!(out.contains("`app.repositories.user`"));
    }

    #[test]
    fn renders_rules_violations_limit_variants_with_and_without_node_id() {
        use crate::check_report::{
            CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
            ProjectCheckSummary,
        };

        let a = minimal_analysis();
        let c = CheckReport {
            ok: false,
            violations: 2,
            projects: vec![ProjectCheckResult {
                name: "my-app".into(),
                ok: false,
                summary: ProjectCheckSummary {
                    nodes: 0,
                    edges: 0,
                    communities: 0,
                    cycles: 0,
                    max_hotspot_score: 0.0,
                    max_hotspot_id: None,
                },
                limits: CheckLimits::default(),
                policy_summary: PolicyCheckSummary {
                    rules_evaluated: 0,
                    policy_violations: 0,
                },
                violations: vec![
                    // Limit with node_id (e.g. max_hotspot_score)
                    CheckViolation::Limit {
                        kind: "max_hotspot_score".into(),
                        actual: serde_json::json!(0.9),
                        expected_max: serde_json::json!(0.5),
                        node_id: Some("app.core".into()),
                    },
                    // Limit without node_id (e.g. max_cycles)
                    CheckViolation::Limit {
                        kind: "max_cycles".into(),
                        actual: serde_json::json!(3),
                        expected_max: serde_json::json!(0),
                        node_id: None,
                    },
                ],
            }],
            contracts: None,
        };

        let out = render("my-app", &a, None, Some(&c));
        assert!(out.contains("**Rules violations (2)**"));

        // Limit with node_id: "- `<kind>` — `<n>`: <actual> > <expected>"
        assert!(out.contains("`max_hotspot_score`"));
        assert!(out.contains("`app.core`"));
        assert!(out.contains("0.9 > 0.5"));

        // Limit without node_id: "- `<kind>` — <actual> > <expected>"
        assert!(out.contains("`max_cycles`"));
        assert!(out.contains("3 > 0"));
        // Guard against accidentally including an empty node_id slot
        assert!(!out.contains("`max_cycles` — ``"));
    }

    #[test]
    fn omits_outstanding_section_when_check_is_none() {
        let a = minimal_analysis();
        let out = render("my-app", &a, None, None);
        assert!(!out.contains("#### Outstanding issues"));
        assert!(!out.contains("**Rules violations"));
    }

    #[test]
    fn omits_outstanding_section_when_no_violations() {
        use crate::check_report::CheckReport;
        let a = minimal_analysis();
        let c = CheckReport {
            ok: true,
            violations: 0,
            projects: vec![],
            contracts: None,
        };
        let out = render("my-app", &a, None, Some(&c));
        assert!(!out.contains("#### Outstanding issues"));
    }

    fn check_with_contract_violations(violations_per_pair: Vec<(&str, &str, &str)>) -> CheckReport {
        use crate::check_report::CheckReport;
        use crate::contract_json::{
            ContractCheckResult, ContractPairResult, ContractSideInfo, ViolationEntry,
        };
        use graphify_core::contract::Severity;
        use graphify_core::contract::{ContractViolation, FieldType, PrimitiveType};
        use std::path::PathBuf;

        let pairs = violations_per_pair
            .into_iter()
            .map(|(pair_name, orm_tbl, ts_ty)| ContractPairResult {
                name: pair_name.into(),
                orm: ContractSideInfo {
                    file: PathBuf::from("schema.ts"),
                    symbol: orm_tbl.into(),
                    line: 1,
                },
                ts: ContractSideInfo {
                    file: PathBuf::from("types.ts"),
                    symbol: ts_ty.into(),
                    line: 1,
                },
                violations: vec![ViolationEntry {
                    severity: Severity::Error,
                    violation: ContractViolation::ContractFieldMissingOnTs {
                        field: "created_at".into(),
                        orm_type: FieldType::Primitive {
                            value: PrimitiveType::Date,
                        },
                        orm_line: 10,
                    },
                }],
            })
            .collect::<Vec<_>>();

        CheckReport {
            ok: false,
            violations: pairs.len(),
            projects: vec![],
            contracts: Some(ContractCheckResult {
                ok: false,
                error_count: pairs.len(),
                warning_count: 0,
                pairs,
            }),
        }
    }

    #[test]
    fn renders_contract_drift_subsection() {
        let a = minimal_analysis();
        let c = check_with_contract_violations(vec![("users_pair", "users", "User")]);
        let out = render("my-app", &a, None, Some(&c));
        assert!(out.contains("#### Outstanding issues"));
        assert!(out.contains("**Contract drift (1)**"));
        assert!(out.contains("`users`"));
        assert!(out.contains("`User`"));
    }

    #[test]
    fn renders_both_rules_and_contract_subsections() {
        let a = minimal_analysis();
        let mut c = check_with_rule_violations(vec![("r1", "a", "b")]);
        let extra = check_with_contract_violations(vec![("pair", "users", "User")]);
        c.contracts = extra.contracts;
        let out = render("my-app", &a, None, Some(&c));
        assert!(out.contains("**Rules violations"));
        assert!(out.contains("**Contract drift"));
    }

    #[test]
    fn renders_only_contract_subsection_when_no_rules() {
        let a = minimal_analysis();
        let c = check_with_contract_violations(vec![("pair", "users", "User")]);
        let out = render("my-app", &a, None, Some(&c));
        assert!(out.contains("**Contract drift"));
        assert!(!out.contains("**Rules violations"));
    }
}
