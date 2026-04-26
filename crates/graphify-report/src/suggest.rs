//! FEAT-043 — `graphify suggest stubs` core types + scoring + renderers.
//!
//! Pure module: no I/O, no extractor deps. Consumes a `GraphSnapshot`
//! (deserialised from `graph.json`) per project plus the project's
//! already-merged `ExternalStubs`, and emits a `SuggestReport` describing
//! candidate prefixes to add to `[settings].external_stubs` or
//! `[[project]].external_stubs`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ---------------------------------------------------------------------------
// Input: subset of graph.json the suggester needs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphNode {
    pub id: String,
    /// Debug-formatted `Language` enum: "Rust", "Python", "TypeScript",
    /// "Php", "Go". Matched verbatim by `extract_prefix`.
    pub language: String,
    pub is_local: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    pub weight: u32,
}

pub struct ProjectInput<'a> {
    pub name: &'a str,
    pub local_prefix: &'a str,
    pub current_stubs: &'a graphify_core::ExternalStubs,
    pub graph: &'a GraphSnapshot,
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StubCandidate {
    pub prefix: String,
    pub language: String,
    pub edge_weight: u64,
    pub node_count: usize,
    pub projects: Vec<String>,
    pub example_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestReport {
    pub min_edges: u64,
    pub settings_candidates: Vec<StubCandidate>,
    pub per_project_candidates: BTreeMap<String, Vec<StubCandidate>>,
    pub already_covered_prefixes: Vec<String>,
    pub shadowed_prefixes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Prefix extraction (language-aware)
// ---------------------------------------------------------------------------

/// Extracts the natural stub prefix from a node id, language-aware.
///
/// Rules per language (matches `language` string from `GraphNode`):
/// - `Rust`: first segment before `::`
/// - `Python`: first segment before `.`
/// - `Php`: first segment before `.` (PSR-4 already normalised `\` → `.`)
/// - `TypeScript`: if id starts with `@`, take two `/`-separated segments
///   (`@scope/name`); otherwise first `/`-separated segment.
/// - `Go`: if first `/`-separated segment contains `.` (path-style like
///   `github.com/spf13/cobra`), take first 3 `/`-segments. Otherwise
///   first `.`-separated segment (like `fmt.Println`).
/// - Anything else (unknown language string): return the id verbatim.
///
/// Returns `None` only for an empty id; otherwise returns at least
/// the input itself.
pub fn extract_prefix(node_id: &str, language: &str) -> Option<String> {
    let trimmed = node_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    match language {
        "Rust" => Some(
            trimmed
                .split("::")
                .next()
                .expect("split always yields ≥1 segment for non-empty input")
                .to_string(),
        ),
        "Python" | "Php" => Some(
            trimmed
                .split('.')
                .next()
                .expect("split always yields ≥1 segment for non-empty input")
                .to_string(),
        ),
        "TypeScript" => {
            if let Some(without_at) = trimmed.strip_prefix('@') {
                let mut parts = without_at.splitn(3, '/');
                match (parts.next(), parts.next()) {
                    (Some(scope), Some(pkg)) if !scope.is_empty() && !pkg.is_empty() => {
                        Some(format!("@{}/{}", scope, pkg))
                    }
                    _ => Some(trimmed.to_string()),
                }
            } else {
                Some(
                    trimmed
                        .split('/')
                        .next()
                        .expect("split always yields ≥1 segment for non-empty input")
                        .to_string(),
                )
            }
        }
        "Go" => {
            let parts: Vec<&str> = trimmed.split('/').collect();
            if parts.len() >= 3 && parts[0].contains('.') {
                Some(parts[..3].join("/"))
            } else if parts.len() == 1 {
                Some(
                    parts[0]
                        .split('.')
                        .next()
                        .expect("split always yields ≥1 segment for non-empty input")
                        .to_string(),
                )
            } else {
                Some(parts[0].to_string())
            }
        }
        _ => Some(trimmed.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Scoring entry-point. Aggregates external prefix candidates across
/// `projects`, applying `min_edges` per-project before cross-project
/// auto-classification.
pub fn score_stubs(projects: &[ProjectInput<'_>], min_edges: u64) -> SuggestReport {
    // Build a global index of local-prefixes + local-node-top-segments for
    // shadowing safety (rule (a) + (b) from the spec).
    let mut shadow_set: BTreeSet<String> = BTreeSet::new();
    for p in projects {
        if !p.local_prefix.is_empty() {
            shadow_set.insert(p.local_prefix.to_string());
        }
        for n in &p.graph.nodes {
            if n.is_local {
                if let Some(top) = top_segment(&n.id) {
                    shadow_set.insert(top);
                }
            }
        }
    }

    let mut already_covered: BTreeSet<String> = BTreeSet::new();
    let mut shadowed: BTreeSet<String> = BTreeSet::new();

    // Per-project per-prefix accumulator.
    // Key: (project_name, prefix). Value: aggregated stats.
    struct PerProject {
        edge_weight: u64,
        nodes: BTreeSet<String>,
        language: String,
    }
    let mut per_project: HashMap<(String, String), PerProject> = HashMap::new();

    for p in projects {
        // Index nodes by id for quick is_local + language lookup.
        let node_lang: HashMap<&str, (&str, bool)> = p
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), (n.language.as_str(), n.is_local)))
            .collect();

        for link in &p.graph.links {
            let Some((lang, is_local)) = node_lang.get(link.target.as_str()).copied() else {
                continue;
            };
            if is_local {
                continue;
            }
            // Already covered?
            if p.current_stubs.matches(&link.target) {
                if let Some(prefix) = extract_prefix(&link.target, lang) {
                    already_covered.insert(prefix);
                }
                continue;
            }
            let Some(prefix) = extract_prefix(&link.target, lang) else {
                continue;
            };
            // Shadowing?
            if shadow_set.contains(&prefix) {
                shadowed.insert(prefix);
                continue;
            }

            let entry = per_project
                .entry((p.name.to_string(), prefix.clone()))
                .or_insert(PerProject {
                    edge_weight: 0,
                    nodes: BTreeSet::new(),
                    language: lang.to_string(),
                });
            entry.edge_weight += u64::from(link.weight);
            entry.nodes.insert(link.target.clone());
        }
    }

    // Apply per-project threshold.
    per_project.retain(|_, v| v.edge_weight >= min_edges);

    // Group by prefix → list of (project, stats).
    let mut by_prefix: BTreeMap<String, Vec<(String, PerProject)>> = BTreeMap::new();
    for ((proj, prefix), stats) in per_project {
        by_prefix.entry(prefix).or_default().push((proj, stats));
    }

    let mut settings_candidates: Vec<StubCandidate> = Vec::new();
    let mut per_project_candidates: BTreeMap<String, Vec<StubCandidate>> = BTreeMap::new();

    for (prefix, hits) in by_prefix {
        let mut projects_sorted: Vec<String> = hits.iter().map(|(p, _)| p.clone()).collect();
        projects_sorted.sort();
        let total_weight: u64 = hits.iter().map(|(_, s)| s.edge_weight).sum();
        let mut node_set: BTreeSet<String> = BTreeSet::new();
        for (_, s) in &hits {
            for n in &s.nodes {
                node_set.insert(n.clone());
            }
        }
        let example_nodes: Vec<String> = node_set.iter().take(3).cloned().collect();
        // Language: take from the first hit (all hits for the same prefix
        // are expected to share a language; tied to the prefix-extraction
        // rule which is language-keyed).
        let language = hits[0].1.language.clone();

        let cand = StubCandidate {
            prefix: prefix.clone(),
            language,
            edge_weight: total_weight,
            node_count: node_set.len(),
            projects: projects_sorted.clone(),
            example_nodes,
        };

        if projects_sorted.len() >= 2 {
            settings_candidates.push(cand);
        } else {
            per_project_candidates
                .entry(projects_sorted[0].clone())
                .or_default()
                .push(cand);
        }
    }

    // Sort: edge_weight desc, prefix asc as tie-break.
    settings_candidates.sort_by(|a, b| {
        b.edge_weight
            .cmp(&a.edge_weight)
            .then_with(|| a.prefix.cmp(&b.prefix))
    });
    for v in per_project_candidates.values_mut() {
        v.sort_by(|a, b| {
            b.edge_weight
                .cmp(&a.edge_weight)
                .then_with(|| a.prefix.cmp(&b.prefix))
        });
    }

    SuggestReport {
        min_edges,
        settings_candidates,
        per_project_candidates,
        already_covered_prefixes: already_covered.into_iter().collect(),
        shadowed_prefixes: shadowed.into_iter().collect(),
    }
}

fn top_segment(id: &str) -> Option<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Cheapest cross-language top-segment grab: split on the first of
    // `::`, `/`, `.`, whichever appears first.
    let positions = [
        trimmed.find("::").map(|p| (p, 2)),
        trimmed.find('/').map(|p| (p, 1)),
        trimmed.find('.').map(|p| (p, 1)),
    ];
    let earliest = positions.iter().filter_map(|x| *x).min_by_key(|(p, _)| *p);
    match earliest {
        Some((p, _)) => Some(trimmed[..p].to_string()),
        None => Some(trimmed.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Markdown renderer
// ---------------------------------------------------------------------------

use std::fmt::Write as _;

pub fn render_markdown(report: &SuggestReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Stub Suggestions");
    let _ = writeln!(out);

    let total_candidates = report.settings_candidates.len()
        + report
            .per_project_candidates
            .values()
            .map(|v| v.len())
            .sum::<usize>();

    if total_candidates == 0 {
        let _ = writeln!(
            out,
            "No stub suggestions above threshold (--min-edges={}).",
            report.min_edges
        );
        return finalize_md(out, report);
    }

    let _ = writeln!(
        out,
        "{} candidates above threshold (--min-edges={})",
        total_candidates, report.min_edges
    );
    let _ = writeln!(out);

    if !report.settings_candidates.is_empty() {
        let _ = writeln!(
            out,
            "## Promote to [settings].external_stubs (cross-project)"
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "| Prefix | Edges | Projects | Example |");
        let _ = writeln!(out, "|--------|-------|----------|---------|");
        for c in &report.settings_candidates {
            let projects_disp = format!("{} ({})", c.projects.len(), c.projects.join(", "));
            let example = c.example_nodes.first().map(String::as_str).unwrap_or("");
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} |",
                c.prefix, c.edge_weight, projects_disp, example
            );
        }
        let _ = writeln!(out);
    }

    if !report.per_project_candidates.is_empty() {
        let _ = writeln!(out, "## Per-project candidates");
        let _ = writeln!(out);
        for (proj, cands) in &report.per_project_candidates {
            let _ = writeln!(out, "### {}", proj);
            let _ = writeln!(out, "| Prefix | Edges | Example |");
            let _ = writeln!(out, "|--------|-------|---------|");
            for c in cands {
                let example = c.example_nodes.first().map(String::as_str).unwrap_or("");
                let _ = writeln!(out, "| `{}` | {} | {} |", c.prefix, c.edge_weight, example);
            }
            let _ = writeln!(out);
        }
    }

    finalize_md(out, report)
}

fn finalize_md(mut out: String, report: &SuggestReport) -> String {
    if !report.already_covered_prefixes.is_empty() {
        let _ = writeln!(out, "## Already covered (skipped)");
        let _ = writeln!(out);
        let list = report
            .already_covered_prefixes
            .iter()
            .map(|p| format!("`{}`", p))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "{} prefix(es) already in current external_stubs: {}",
            report.already_covered_prefixes.len(),
            list
        );
        let _ = writeln!(out);
    }

    if !report.shadowed_prefixes.is_empty() {
        let _ = writeln!(out, "## Skipped — shadowing local modules");
        let _ = writeln!(out);
        let list = report
            .shadowed_prefixes
            .iter()
            .map(|p| format!("`{}`", p))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "{} prefix(es) matching local_prefix or known module: {}",
            report.shadowed_prefixes.len(),
            list
        );
        let _ = writeln!(out);
    }

    out
}

pub fn render_toml(report: &SuggestReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Generated by `graphify suggest stubs`");
    let _ = writeln!(out, "# Min edges per project: {}", report.min_edges);
    let _ = writeln!(out);

    if !report.settings_candidates.is_empty() {
        let _ = writeln!(out, "# Append to [settings] block:");
        let _ = writeln!(out, "# [settings]");
        let prefixes = report
            .settings_candidates
            .iter()
            .map(|c| format!("\"{}\"", c.prefix))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "# external_stubs = [{}]", prefixes);
        let _ = writeln!(out);
    }

    for (proj, cands) in &report.per_project_candidates {
        let _ = writeln!(out, "# Append to [[project]] block name=\"{}\":", proj);
        let _ = writeln!(out, "# [[project]]");
        let _ = writeln!(out, "# name = \"{}\"", proj);
        let prefixes = cands
            .iter()
            .map(|c| format!("\"{}\"", c.prefix))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "# external_stubs = [{}]", prefixes);
        let _ = writeln!(out);
    }

    if report.settings_candidates.is_empty() && report.per_project_candidates.is_empty() {
        let _ = writeln!(out, "# (no candidates above threshold)");
    }

    out
}

pub fn render_json(report: &SuggestReport) -> serde_json::Value {
    serde_json::to_value(report).expect("SuggestReport must serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, lang: &str, is_local: bool) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            language: lang.to_string(),
            is_local,
        }
    }

    fn make_link(source: &str, target: &str, weight: u32) -> GraphLink {
        GraphLink {
            source: source.to_string(),
            target: target.to_string(),
            weight,
        }
    }

    #[test]
    fn score_stubs_promotes_cross_project_prefix_to_settings() {
        use graphify_core::ExternalStubs;
        let empty_stubs = ExternalStubs::default();

        let proj_a_graph = GraphSnapshot {
            nodes: vec![
                make_node("crate_a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
            ],
            links: vec![make_link("crate_a::main", "tokio::spawn", 5)],
        };
        let proj_b_graph = GraphSnapshot {
            nodes: vec![
                make_node("crate_b::main", "Rust", true),
                make_node("tokio::sync::mpsc::Sender", "Rust", false),
            ],
            links: vec![make_link("crate_b::main", "tokio::sync::mpsc::Sender", 3)],
        };

        let inputs = vec![
            ProjectInput {
                name: "proj-a",
                local_prefix: "crate_a",
                current_stubs: &empty_stubs,
                graph: &proj_a_graph,
            },
            ProjectInput {
                name: "proj-b",
                local_prefix: "crate_b",
                current_stubs: &empty_stubs,
                graph: &proj_b_graph,
            },
        ];

        let report = score_stubs(&inputs, 1);

        assert_eq!(
            report.settings_candidates.len(),
            1,
            "tokio should be in settings"
        );
        let cand = &report.settings_candidates[0];
        assert_eq!(cand.prefix, "tokio");
        assert_eq!(cand.edge_weight, 8); // 5 + 3
        assert_eq!(
            cand.projects,
            vec!["proj-a".to_string(), "proj-b".to_string()]
        );
        assert!(report.per_project_candidates.is_empty());
    }

    #[test]
    fn extract_prefix_rust_takes_first_double_colon_segment() {
        assert_eq!(
            extract_prefix("tokio::sync::mpsc::Sender", "Rust").as_deref(),
            Some("tokio")
        );
        assert_eq!(extract_prefix("std", "Rust").as_deref(), Some("std"));
    }

    #[test]
    fn extract_prefix_python_takes_first_dot_segment() {
        assert_eq!(
            extract_prefix("numpy.linalg.norm", "Python").as_deref(),
            Some("numpy")
        );
    }

    #[test]
    fn extract_prefix_php_takes_first_dot_segment() {
        assert_eq!(
            extract_prefix("Symfony.Component.HttpFoundation.Request", "Php").as_deref(),
            Some("Symfony")
        );
    }

    #[test]
    fn extract_prefix_ts_scoped_takes_two_segments() {
        assert_eq!(
            extract_prefix("@anthropic-ai/sdk/messages", "TypeScript").as_deref(),
            Some("@anthropic-ai/sdk")
        );
    }

    #[test]
    fn extract_prefix_ts_unscoped_takes_first_segment() {
        assert_eq!(
            extract_prefix("react/jsx-runtime", "TypeScript").as_deref(),
            Some("react")
        );
        assert_eq!(
            extract_prefix("lodash", "TypeScript").as_deref(),
            Some("lodash")
        );
    }

    #[test]
    fn extract_prefix_ts_scoped_single_segment_returns_as_is() {
        assert_eq!(
            extract_prefix("@anthropic-ai", "TypeScript").as_deref(),
            Some("@anthropic-ai")
        );
    }

    #[test]
    fn extract_prefix_go_path_style_takes_three_segments() {
        assert_eq!(
            extract_prefix("github.com/spf13/cobra/cmd", "Go").as_deref(),
            Some("github.com/spf13/cobra")
        );
    }

    #[test]
    fn extract_prefix_go_simple_takes_first_dot_segment() {
        assert_eq!(extract_prefix("fmt.Println", "Go").as_deref(), Some("fmt"));
    }

    #[test]
    fn extract_prefix_empty_returns_none() {
        assert_eq!(extract_prefix("", "Rust"), None);
        assert_eq!(extract_prefix("   ", "Rust"), None);
    }

    #[test]
    fn extract_prefix_unknown_language_returns_verbatim() {
        assert_eq!(
            extract_prefix("foo.bar", "Klingon").as_deref(),
            Some("foo.bar")
        );
    }

    #[test]
    fn score_stubs_keeps_single_project_prefix_per_project() {
        use graphify_core::ExternalStubs;
        let empty = ExternalStubs::default();

        let g = GraphSnapshot {
            nodes: vec![
                make_node("crate_a::main", "Rust", true),
                make_node("rmcp::ServerHandler", "Rust", false),
            ],
            links: vec![make_link("crate_a::main", "rmcp::ServerHandler", 7)],
        };
        let inputs = vec![ProjectInput {
            name: "proj-a",
            local_prefix: "crate_a",
            current_stubs: &empty,
            graph: &g,
        }];

        let report = score_stubs(&inputs, 1);
        assert!(report.settings_candidates.is_empty());
        assert_eq!(report.per_project_candidates.len(), 1);
        let proj_a = report.per_project_candidates.get("proj-a").unwrap();
        assert_eq!(proj_a.len(), 1);
        assert_eq!(proj_a[0].prefix, "rmcp");
    }

    #[test]
    fn score_stubs_threshold_drops_per_project_before_aggregation() {
        use graphify_core::ExternalStubs;
        let empty = ExternalStubs::default();

        // tokio appears in both projects but only weight-4 in each — below
        // threshold of 5, so both drop and the cross-project promotion never
        // happens.
        let g_a = GraphSnapshot {
            nodes: vec![
                make_node("crate_a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
            ],
            links: vec![make_link("crate_a::main", "tokio::spawn", 4)],
        };
        let g_b = GraphSnapshot {
            nodes: vec![
                make_node("crate_b::main", "Rust", true),
                make_node("tokio::sync::mpsc::Sender", "Rust", false),
            ],
            links: vec![make_link("crate_b::main", "tokio::sync::mpsc::Sender", 4)],
        };
        let inputs = vec![
            ProjectInput {
                name: "proj-a",
                local_prefix: "crate_a",
                current_stubs: &empty,
                graph: &g_a,
            },
            ProjectInput {
                name: "proj-b",
                local_prefix: "crate_b",
                current_stubs: &empty,
                graph: &g_b,
            },
        ];

        let report = score_stubs(&inputs, 5);
        assert!(
            report.settings_candidates.is_empty(),
            "below threshold per project"
        );
        assert!(report.per_project_candidates.is_empty());
    }

    #[test]
    fn score_stubs_records_already_covered_and_skips_them() {
        use graphify_core::ExternalStubs;
        let stubs = ExternalStubs::new(["tokio"]);

        let g = GraphSnapshot {
            nodes: vec![
                make_node("crate_a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
            ],
            links: vec![make_link("crate_a::main", "tokio::spawn", 10)],
        };
        let inputs = vec![ProjectInput {
            name: "proj-a",
            local_prefix: "crate_a",
            current_stubs: &stubs,
            graph: &g,
        }];

        let report = score_stubs(&inputs, 1);
        assert!(report.settings_candidates.is_empty());
        assert!(report.per_project_candidates.is_empty());
        assert_eq!(report.already_covered_prefixes, vec!["tokio".to_string()]);
    }

    #[test]
    fn score_stubs_records_shadowing_against_local_prefix() {
        use graphify_core::ExternalStubs;
        let empty = ExternalStubs::default();

        // src is project-a's local_prefix. An external node id "src::foo" from
        // project-b would be shadowing — never suggest.
        let g_a = GraphSnapshot {
            nodes: vec![make_node("src::main", "Rust", true)],
            links: vec![],
        };
        let g_b = GraphSnapshot {
            nodes: vec![
                make_node("crate_b::main", "Rust", true),
                make_node("src::foo::bar", "Rust", false),
            ],
            links: vec![make_link("crate_b::main", "src::foo::bar", 9)],
        };
        let inputs = vec![
            ProjectInput {
                name: "proj-a",
                local_prefix: "src",
                current_stubs: &empty,
                graph: &g_a,
            },
            ProjectInput {
                name: "proj-b",
                local_prefix: "crate_b",
                current_stubs: &empty,
                graph: &g_b,
            },
        ];

        let report = score_stubs(&inputs, 1);
        assert!(report.settings_candidates.is_empty());
        assert!(report.per_project_candidates.is_empty());
        assert_eq!(report.shadowed_prefixes, vec!["src".to_string()]);
    }

    #[test]
    fn score_stubs_ranks_by_edge_weight_then_prefix() {
        use graphify_core::ExternalStubs;
        let empty = ExternalStubs::default();

        // Two prefixes both in 2 projects so they cross to settings; verify
        // sort order: serde (weight 100) before tokio (weight 50), and an
        // equal-weight tie sorts by prefix asc.
        let g_a = GraphSnapshot {
            nodes: vec![
                make_node("a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
                make_node("serde::Deserialize", "Rust", false),
                make_node("clap::Parser", "Rust", false),
            ],
            links: vec![
                make_link("a::main", "tokio::spawn", 25),
                make_link("a::main", "serde::Deserialize", 50),
                make_link("a::main", "clap::Parser", 25),
            ],
        };
        let g_b = GraphSnapshot {
            nodes: vec![
                make_node("b::main", "Rust", true),
                make_node("tokio::sync::mpsc::Sender", "Rust", false),
                make_node("serde::Serialize", "Rust", false),
                make_node("clap::ValueEnum", "Rust", false),
            ],
            links: vec![
                make_link("b::main", "tokio::sync::mpsc::Sender", 25),
                make_link("b::main", "serde::Serialize", 50),
                make_link("b::main", "clap::ValueEnum", 25),
            ],
        };
        let inputs = vec![
            ProjectInput {
                name: "proj-a",
                local_prefix: "a",
                current_stubs: &empty,
                graph: &g_a,
            },
            ProjectInput {
                name: "proj-b",
                local_prefix: "b",
                current_stubs: &empty,
                graph: &g_b,
            },
        ];

        let report = score_stubs(&inputs, 1);
        let prefixes: Vec<&str> = report
            .settings_candidates
            .iter()
            .map(|c| c.prefix.as_str())
            .collect();
        assert_eq!(prefixes, vec!["serde", "clap", "tokio"]);
        // serde first (weight 100); clap and tokio tied at 50 → asc by prefix → clap before tokio.
    }

    #[test]
    fn render_markdown_includes_all_sections() {
        use graphify_core::ExternalStubs;
        let stubs = ExternalStubs::new(["std"]);

        let g_a = GraphSnapshot {
            nodes: vec![
                make_node("a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
                make_node("rmcp::ServerHandler", "Rust", false),
                make_node("std::collections::HashMap", "Rust", false),
            ],
            links: vec![
                make_link("a::main", "tokio::spawn", 5),
                make_link("a::main", "rmcp::ServerHandler", 3),
                make_link("a::main", "std::collections::HashMap", 7),
            ],
        };
        let g_b = GraphSnapshot {
            nodes: vec![
                make_node("b::main", "Rust", true),
                make_node("tokio::sync::mpsc::Sender", "Rust", false),
            ],
            links: vec![make_link("b::main", "tokio::sync::mpsc::Sender", 4)],
        };
        let inputs = vec![
            ProjectInput {
                name: "proj-a",
                local_prefix: "a",
                current_stubs: &stubs,
                graph: &g_a,
            },
            ProjectInput {
                name: "proj-b",
                local_prefix: "b",
                current_stubs: &stubs,
                graph: &g_b,
            },
        ];

        let report = score_stubs(&inputs, 1);
        let md = render_markdown(&report);

        assert!(md.contains("# Stub Suggestions"), "missing header");
        assert!(
            md.contains("## Promote to [settings].external_stubs"),
            "missing settings section"
        );
        assert!(md.contains("`tokio`"), "missing tokio entry");
        assert!(
            md.contains("## Per-project candidates"),
            "missing per-project section"
        );
        assert!(md.contains("### proj-a"), "missing proj-a subheader");
        assert!(md.contains("`rmcp`"), "missing rmcp entry");
        assert!(
            md.contains("## Already covered"),
            "missing already-covered section"
        );
        assert!(
            md.contains("`std`"),
            "std should be listed as already covered"
        );
    }

    #[test]
    fn render_markdown_empty_report_emits_no_suggestions_note() {
        let report = SuggestReport {
            min_edges: 2,
            settings_candidates: vec![],
            per_project_candidates: BTreeMap::new(),
            already_covered_prefixes: vec![],
            shadowed_prefixes: vec![],
        };
        let md = render_markdown(&report);
        assert!(
            md.contains("No stub suggestions"),
            "missing empty-state message"
        );
    }

    #[test]
    fn render_toml_emits_commented_snippet_with_settings_and_per_project() {
        use graphify_core::ExternalStubs;
        let empty = ExternalStubs::default();

        let g_a = GraphSnapshot {
            nodes: vec![
                make_node("a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
                make_node("rmcp::ServerHandler", "Rust", false),
            ],
            links: vec![
                make_link("a::main", "tokio::spawn", 5),
                make_link("a::main", "rmcp::ServerHandler", 3),
            ],
        };
        let g_b = GraphSnapshot {
            nodes: vec![
                make_node("b::main", "Rust", true),
                make_node("tokio::sync::mpsc::Sender", "Rust", false),
            ],
            links: vec![make_link("b::main", "tokio::sync::mpsc::Sender", 4)],
        };
        let inputs = vec![
            ProjectInput {
                name: "proj-a",
                local_prefix: "a",
                current_stubs: &empty,
                graph: &g_a,
            },
            ProjectInput {
                name: "proj-b",
                local_prefix: "b",
                current_stubs: &empty,
                graph: &g_b,
            },
        ];

        let report = score_stubs(&inputs, 1);
        let toml_out = render_toml(&report);

        // Header comment with timestamp/threshold
        assert!(toml_out.contains("# Generated by `graphify suggest stubs`"));
        assert!(toml_out.contains("# Min edges per project: 1"));
        // Settings block (commented)
        assert!(toml_out.contains("# [settings]"));
        assert!(toml_out.contains("# external_stubs = [\"tokio\"]"));
        // Per-project block (commented)
        assert!(toml_out.contains("# [[project]]"));
        assert!(toml_out.contains("# name = \"proj-a\""));
        assert!(toml_out.contains("# external_stubs = [\"rmcp\"]"));
    }

    #[test]
    fn render_json_round_trips() {
        use graphify_core::ExternalStubs;
        let empty = ExternalStubs::default();

        let g = GraphSnapshot {
            nodes: vec![
                make_node("a::main", "Rust", true),
                make_node("tokio::spawn", "Rust", false),
            ],
            links: vec![make_link("a::main", "tokio::spawn", 4)],
        };
        let inputs = vec![ProjectInput {
            name: "proj-a",
            local_prefix: "a",
            current_stubs: &empty,
            graph: &g,
        }];

        let report = score_stubs(&inputs, 1);
        let value = render_json(&report);
        let s = serde_json::to_string(&value).unwrap();

        // Top-level keys present
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["min_edges"], 1);
        assert!(parsed["per_project_candidates"]["proj-a"].is_array());
        assert_eq!(
            parsed["per_project_candidates"]["proj-a"][0]["prefix"],
            "tokio"
        );
    }
}
