//! PR Summary renderer.
//!
//! Produces a concise Markdown summary of architectural change for a single
//! project's Graphify output directory. Pure function: no I/O. Consumers are
//! expected to load inputs separately and pass them in as structs.

use graphify_core::diff::{AnalysisSnapshot, DiffReport};

use crate::check_report::CheckReport;

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

fn render_drift_section(_out: &mut String, _drift: Option<&DiffReport>) {
    // Task 7+ implements this.
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
}
