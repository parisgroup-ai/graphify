//! FEAT-043 — `graphify suggest stubs` core types + scoring + renderers.
//!
//! Pure module: no I/O, no extractor deps. Consumes a `GraphSnapshot`
//! (deserialised from `graph.json`) per project plus the project's
//! already-merged `ExternalStubs`, and emits a `SuggestReport` describing
//! candidate prefixes to add to `[settings].external_stubs` or
//! `[[project]].external_stubs`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    pub current_stubs: &'a graphify_extract::stubs::ExternalStubs,
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
