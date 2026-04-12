use std::path::PathBuf;

use serde::Serialize;

use crate::community::Community;
use crate::cycles::CycleGroup;
use crate::graph::CodeGraph;
use crate::metrics::NodeMetrics;
use crate::types::{EdgeKind, NodeKind};

// ---------------------------------------------------------------------------
// GraphStats
// ---------------------------------------------------------------------------

/// High-level statistics about the analyzed graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub local_node_count: usize,
    pub community_count: usize,
    pub cycle_count: usize,
}

// ---------------------------------------------------------------------------
// QueryMatch
// ---------------------------------------------------------------------------

/// A single search result from the query engine.
#[derive(Debug, Clone, Serialize)]
pub struct QueryMatch {
    pub node_id: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub score: f64,
    pub community_id: usize,
    pub in_cycle: bool,
}

// ---------------------------------------------------------------------------
// SearchFilters / SortField
// ---------------------------------------------------------------------------

/// Controls how search results are filtered and sorted.
pub struct SearchFilters {
    pub kind: Option<NodeKind>,
    pub sort_by: SortField,
    pub local_only: bool,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            kind: None,
            sort_by: SortField::Score,
            local_only: false,
        }
    }
}

/// Field used to sort search results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortField {
    Score,
    Name,
    InDegree,
}

// ---------------------------------------------------------------------------
// GlobMatcher
// ---------------------------------------------------------------------------

/// A simple glob pattern matcher supporting `*` (any sequence of characters)
/// and `?` (any single character).  No external crate required.
struct GlobMatcher {
    pattern: Vec<u8>,
}

impl GlobMatcher {
    fn new(pattern: &str) -> Self {
        Self { pattern: pattern.as_bytes().to_vec() }
    }

    fn is_match(&self, input: &str) -> bool {
        Self::do_match(&self.pattern, input.as_bytes())
    }

    fn do_match(pattern: &[u8], input: &[u8]) -> bool {
        match (pattern.first(), input.first()) {
            (None, None) => true,
            (Some(b'*'), _) => {
                // Try matching rest of pattern with current input (skip the *)
                // or try advancing input by one character.
                Self::do_match(&pattern[1..], input)
                    || (!input.is_empty() && Self::do_match(pattern, &input[1..]))
            }
            (Some(b'?'), Some(_)) => Self::do_match(&pattern[1..], &input[1..]),
            (Some(&p), Some(&i)) if p == i => Self::do_match(&pattern[1..], &input[1..]),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// QueryEngine
// ---------------------------------------------------------------------------

/// Wraps a `CodeGraph` together with pre-computed analysis results and provides
/// query methods for interactive exploration.
pub struct QueryEngine {
    graph: CodeGraph,
    metrics: Vec<NodeMetrics>,
    communities: Vec<Community>,
    cycles: Vec<CycleGroup>,
}

impl QueryEngine {
    /// Constructs a `QueryEngine` by taking ownership of the graph and all
    /// analysis results.
    pub fn from_analyzed(
        graph: CodeGraph,
        metrics: Vec<NodeMetrics>,
        communities: Vec<Community>,
        cycles: Vec<CycleGroup>,
    ) -> Self {
        Self { graph, metrics, communities, cycles }
    }

    /// Returns high-level statistics about the graph.
    pub fn stats(&self) -> GraphStats {
        GraphStats {
            node_count: self.graph.node_count(),
            edge_count: self.graph.edge_count(),
            local_node_count: self.graph.local_node_ids().len(),
            community_count: self.communities.len(),
            cycle_count: self.cycles.len(),
        }
    }

    /// Searches for nodes matching a glob pattern, applying optional filters.
    ///
    /// The pattern is matched against node IDs using `*` (any chars) and `?`
    /// (single char) wildcards.  Results are filtered by `kind` and
    /// `local_only`, then sorted according to `sort_by`.
    pub fn search(&self, pattern: &str, filters: &SearchFilters) -> Vec<QueryMatch> {
        let matcher = GlobMatcher::new(pattern);

        let mut results: Vec<QueryMatch> = self
            .graph
            .nodes()
            .iter()
            .filter(|node| matcher.is_match(&node.id))
            .filter(|node| {
                if let Some(ref kind) = filters.kind {
                    &node.kind == kind
                } else {
                    true
                }
            })
            .filter(|node| {
                if filters.local_only {
                    node.is_local
                } else {
                    true
                }
            })
            .map(|node| {
                let metrics = self.metrics.iter().find(|m| m.id == node.id);
                let score = metrics.map(|m| m.score).unwrap_or(0.0);
                let community_id = metrics.map(|m| m.community_id).unwrap_or(0);
                let in_cycle = metrics.map(|m| m.in_cycle).unwrap_or(false);

                // Override community_id from actual community detection results
                let community_id = self
                    .communities
                    .iter()
                    .find(|c| c.members.iter().any(|mid| mid == &node.id))
                    .map(|c| c.id)
                    .unwrap_or(community_id);

                QueryMatch {
                    node_id: node.id.clone(),
                    kind: node.kind.clone(),
                    file_path: node.file_path.clone(),
                    score,
                    community_id,
                    in_cycle,
                }
            })
            .collect();

        match filters.sort_by {
            SortField::Score => {
                results.sort_by(|a, b| {
                    b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SortField::Name => {
                results.sort_by(|a, b| a.node_id.cmp(&b.node_id));
            }
            SortField::InDegree => {
                results.sort_by(|a, b| {
                    let a_deg = self.graph.in_degree(&a.node_id);
                    let b_deg = self.graph.in_degree(&b.node_id);
                    b_deg.cmp(&a_deg)
                });
            }
        }

        results
    }

    /// Returns up to 3 node IDs that contain `input` as a case-insensitive
    /// substring, sorted alphabetically.
    pub fn suggest(&self, input: &str) -> Vec<String> {
        let lower = input.to_lowercase();
        let mut matches: Vec<String> = self
            .graph
            .node_ids()
            .into_iter()
            .filter(|id| id.to_lowercase().contains(&lower))
            .map(|id| id.to_string())
            .collect();
        matches.sort();
        matches.truncate(3);
        matches
    }

    /// Returns nodes that depend on `node_id` (incoming edges).
    ///
    /// Each entry is `(source_node_id, edge_kind)`.  Returns an empty `Vec`
    /// if the node does not exist.
    pub fn dependents(&self, node_id: &str) -> Vec<(String, EdgeKind)> {
        self.graph
            .incoming_edges(node_id)
            .into_iter()
            .map(|(src, edge)| (src.to_string(), edge.kind.clone()))
            .collect()
    }

    /// Returns nodes that `node_id` depends on (outgoing edges).
    ///
    /// Each entry is `(target_node_id, edge_kind)`.  Returns an empty `Vec`
    /// if the node does not exist.
    pub fn dependencies(&self, node_id: &str) -> Vec<(String, EdgeKind)> {
        self.graph
            .outgoing_edges(node_id)
            .into_iter()
            .map(|(tgt, edge)| (tgt.to_string(), edge.kind.clone()))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language, Node};

    fn module(id: &str) -> Node {
        Node::module(id, format!("{}.py", id.replace('.', "/")), Language::Python, 1, true)
    }

    fn build_engine() -> QueryEngine {
        let mut graph = CodeGraph::new();
        graph.add_node(module("app.main"));
        graph.add_node(module("app.utils"));
        graph.add_node(module("app.services.llm"));
        graph.add_node(Node::module("os", "", Language::Python, 0, false));
        graph.add_edge("app.main", "app.utils", Edge::imports(1));
        graph.add_edge("app.main", "app.services.llm", Edge::imports(2));
        graph.add_edge("app.services.llm", "app.utils", Edge::imports(3));
        graph.add_edge("app.main", "os", Edge::imports(4));

        let metrics = crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);

        QueryEngine::from_analyzed(graph, metrics, communities, cycles)
    }

    // -----------------------------------------------------------------------
    // Task 1: stats
    // -----------------------------------------------------------------------

    #[test]
    fn stats_returns_correct_counts() {
        let engine = build_engine();
        let stats = engine.stats();

        assert_eq!(stats.node_count, 4);
        assert_eq!(stats.edge_count, 4);
        assert_eq!(stats.local_node_count, 3);
        // Community count depends on Louvain, but must be >= 1
        assert!(stats.community_count >= 1);
        // No cycles in this DAG
        assert_eq!(stats.cycle_count, 0);
    }

    // -----------------------------------------------------------------------
    // Task 2: search
    // -----------------------------------------------------------------------

    #[test]
    fn search_glob_matches() {
        let engine = build_engine();
        let results = engine.search("app.services.*", &SearchFilters::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, "app.services.llm");
    }

    #[test]
    fn search_exact_match() {
        let engine = build_engine();
        let results = engine.search("app.main", &SearchFilters::default());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, "app.main");
    }

    #[test]
    fn search_no_matches() {
        let engine = build_engine();
        let results = engine.search("nonexistent.*", &SearchFilters::default());
        assert!(results.is_empty());
    }

    #[test]
    fn search_filter_by_kind() {
        let engine = build_engine();
        let filters = SearchFilters {
            kind: Some(NodeKind::Function),
            ..SearchFilters::default()
        };
        let results = engine.search("*", &filters);
        assert!(results.is_empty(), "all nodes are Module, filtering by Function should return empty");
    }

    #[test]
    fn search_star_matches_all() {
        let engine = build_engine();
        let results = engine.search("*", &SearchFilters::default());
        assert_eq!(results.len(), 4);
    }

    // -----------------------------------------------------------------------
    // Task 3: suggest
    // -----------------------------------------------------------------------

    #[test]
    fn suggest_substring_match() {
        let engine = build_engine();
        let suggestions = engine.suggest("service");
        assert_eq!(suggestions, vec!["app.services.llm"]);
    }

    #[test]
    fn suggest_no_match() {
        let engine = build_engine();
        let suggestions = engine.suggest("zzzzz");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggest_caps_at_three() {
        let engine = build_engine();
        // "app" matches app.main, app.utils, app.services.llm (3+ nodes)
        let suggestions = engine.suggest("app");
        assert!(suggestions.len() <= 3, "suggest should return at most 3 results, got {}", suggestions.len());
    }

    // -----------------------------------------------------------------------
    // Task 4: dependents and dependencies
    // -----------------------------------------------------------------------

    #[test]
    fn dependents_returns_incoming() {
        let engine = build_engine();
        let deps = engine.dependents("app.utils");
        let ids: Vec<&str> = deps.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids.len(), 2, "app.utils should have 2 dependents");
        assert!(ids.contains(&"app.main"));
        assert!(ids.contains(&"app.services.llm"));
    }

    #[test]
    fn dependencies_returns_outgoing() {
        let engine = build_engine();
        let deps = engine.dependencies("app.main");
        let ids: Vec<&str> = deps.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids.len(), 3, "app.main should have 3 dependencies");
        assert!(ids.contains(&"app.utils"));
        assert!(ids.contains(&"app.services.llm"));
        assert!(ids.contains(&"os"));
    }

    #[test]
    fn dependents_unknown_node_returns_empty() {
        let engine = build_engine();
        let deps = engine.dependents("nonexistent");
        assert!(deps.is_empty());
    }
}
