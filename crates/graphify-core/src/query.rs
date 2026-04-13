use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::Serialize;

use crate::community::Community;
use crate::cycles::CycleGroup;
use crate::graph::CodeGraph;
use crate::metrics::NodeMetrics;
use crate::types::{ConfidenceKind, EdgeKind, Language, NodeKind};

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
    pub min_confidence: Option<f64>,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            kind: None,
            sort_by: SortField::Score,
            local_only: false,
            min_confidence: None,
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
        Self {
            pattern: pattern.as_bytes().to_vec(),
        }
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
// PathStep
// ---------------------------------------------------------------------------

/// A single step in a graph path, pairing a node ID with the edge used to
/// reach the *next* node.  The final step in a path has `edge_kind = None`
/// and `weight = 0`.
#[derive(Debug, Clone, Serialize)]
pub struct PathStep {
    pub node_id: String,
    pub edge_kind: Option<EdgeKind>,
    pub weight: u32,
}

// ---------------------------------------------------------------------------
// ExplainReport / ExplainMetrics
// ---------------------------------------------------------------------------

/// Detailed explanation of a single node's role in the graph.
#[derive(Debug, Clone, Serialize)]
pub struct ExplainReport {
    pub node_id: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub language: Language,
    pub metrics: ExplainMetrics,
    pub community_id: usize,
    pub in_cycle: bool,
    pub cycle_peers: Vec<String>,
    pub direct_dependents: Vec<String>,
    pub direct_dependencies: Vec<String>,
    pub transitive_dependent_count: usize,
    pub top_transitive_dependents: Vec<String>,
}

/// Numeric metrics included in an [`ExplainReport`].
#[derive(Debug, Clone, Serialize)]
pub struct ExplainMetrics {
    pub score: f64,
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: usize,
    pub out_degree: usize,
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
        Self {
            graph,
            metrics,
            communities,
            cycles,
        }
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
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
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
    /// Each entry is `(source_node_id, edge_kind, confidence, confidence_kind)`.
    /// Returns an empty `Vec` if the node does not exist.
    pub fn dependents(&self, node_id: &str) -> Vec<(String, EdgeKind, f64, ConfidenceKind)> {
        self.graph
            .incoming_edges(node_id)
            .into_iter()
            .map(|(src, edge)| {
                (
                    src.to_string(),
                    edge.kind.clone(),
                    edge.confidence,
                    edge.confidence_kind.clone(),
                )
            })
            .collect()
    }

    /// Returns nodes that `node_id` depends on (outgoing edges).
    ///
    /// Each entry is `(target_node_id, edge_kind, confidence, confidence_kind)`.
    /// Returns an empty `Vec` if the node does not exist.
    pub fn dependencies(&self, node_id: &str) -> Vec<(String, EdgeKind, f64, ConfidenceKind)> {
        self.graph
            .outgoing_edges(node_id)
            .into_iter()
            .map(|(tgt, edge)| {
                (
                    tgt.to_string(),
                    edge.kind.clone(),
                    edge.confidence,
                    edge.confidence_kind.clone(),
                )
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Task 5: shortest_path (BFS)
    // -----------------------------------------------------------------------

    /// Finds the shortest path (by hop count) from `from` to `to` using BFS.
    ///
    /// Returns `None` if either node does not exist or no path is found.
    /// The returned path includes both endpoints as `PathStep` entries.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<PathStep>> {
        let from_idx = self.graph.get_index(from)?;
        let to_idx = self.graph.get_index(to)?;

        if from_idx == to_idx {
            return Some(vec![PathStep {
                node_id: from.to_string(),
                edge_kind: None,
                weight: 0,
            }]);
        }

        let raw = self.graph.raw_graph();

        // BFS: parent map tracks (parent_index, edge connecting parent→child)
        let mut visited: HashMap<NodeIndex, (NodeIndex, EdgeKind, u32)> = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_back(from_idx);
        // sentinel: from_idx has no parent
        let mut found = false;

        while let Some(current) = queue.pop_front() {
            if current == to_idx {
                found = true;
                break;
            }

            for edge_ref in raw.edges_directed(current, Direction::Outgoing) {
                let neighbor = edge_ref.target();
                if neighbor != from_idx && !visited.contains_key(&neighbor) {
                    let ew = edge_ref.weight();
                    visited.insert(neighbor, (current, ew.kind.clone(), ew.weight));
                    queue.push_back(neighbor);
                }
            }
        }

        if !found {
            return None;
        }

        // Reconstruct path from to_idx back to from_idx
        let mut path_indices: Vec<NodeIndex> = Vec::new();
        let mut current = to_idx;
        path_indices.push(current);
        while current != from_idx {
            let (parent, _, _) = visited.get(&current)?;
            current = *parent;
            path_indices.push(current);
        }
        path_indices.reverse();

        // Convert to PathStep with edge info
        let mut steps: Vec<PathStep> = Vec::with_capacity(path_indices.len());
        for i in 0..path_indices.len() {
            let node_id = raw[path_indices[i]].id.clone();
            if i + 1 < path_indices.len() {
                // Look up edge info from outgoing_edges
                let next_id = &raw[path_indices[i + 1]].id;
                let edge_info = self
                    .graph
                    .outgoing_edges(&node_id)
                    .into_iter()
                    .find(|(tgt, _)| tgt == next_id);
                if let Some((_, edge)) = edge_info {
                    steps.push(PathStep {
                        node_id,
                        edge_kind: Some(edge.kind.clone()),
                        weight: edge.weight,
                    });
                } else {
                    steps.push(PathStep {
                        node_id,
                        edge_kind: None,
                        weight: 0,
                    });
                }
            } else {
                // Last node
                steps.push(PathStep {
                    node_id,
                    edge_kind: None,
                    weight: 0,
                });
            }
        }

        Some(steps)
    }

    // -----------------------------------------------------------------------
    // Task 6: all_paths (DFS)
    // -----------------------------------------------------------------------

    /// Finds all paths from `from` to `to`, limited by `max_depth` (max number
    /// of edges) and `max_paths` (max number of results).
    ///
    /// Returns an empty `Vec` if either node does not exist or no path is found.
    pub fn all_paths(
        &self,
        from: &str,
        to: &str,
        max_depth: usize,
        max_paths: usize,
    ) -> Vec<Vec<PathStep>> {
        let from_idx = match self.graph.get_index(from) {
            Some(idx) => idx,
            None => return Vec::new(),
        };
        let to_idx = match self.graph.get_index(to) {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let raw = self.graph.raw_graph();
        let mut results: Vec<Vec<NodeIndex>> = Vec::new();

        // DFS with explicit stack: (current_node, path_so_far)
        let mut stack: Vec<(NodeIndex, Vec<NodeIndex>)> = Vec::new();
        stack.push((from_idx, vec![from_idx]));

        while let Some((current, path)) = stack.pop() {
            if results.len() >= max_paths {
                break;
            }

            if current == to_idx && path.len() > 1 {
                results.push(path);
                continue;
            }

            // Don't explore further if we've already used max_depth edges
            if path.len() > max_depth {
                continue;
            }

            for edge_ref in raw.edges_directed(current, Direction::Outgoing) {
                let neighbor = edge_ref.target();
                // Allow revisiting target, but not other nodes on the current path
                if neighbor == to_idx || !path.contains(&neighbor) {
                    let mut new_path = path.clone();
                    new_path.push(neighbor);
                    stack.push((neighbor, new_path));
                }
            }
        }

        // Convert index paths to PathStep paths
        results
            .into_iter()
            .map(|index_path| self.index_path_to_steps(&index_path))
            .collect()
    }

    /// Converts a path of `NodeIndex` values to a `Vec<PathStep>`.
    fn index_path_to_steps(&self, index_path: &[NodeIndex]) -> Vec<PathStep> {
        let raw = self.graph.raw_graph();
        let mut steps = Vec::with_capacity(index_path.len());

        for i in 0..index_path.len() {
            let node_id = raw[index_path[i]].id.clone();
            if i + 1 < index_path.len() {
                let next_id = &raw[index_path[i + 1]].id;
                let edge_info = self
                    .graph
                    .outgoing_edges(&node_id)
                    .into_iter()
                    .find(|(tgt, _)| tgt == next_id);
                if let Some((_, edge)) = edge_info {
                    steps.push(PathStep {
                        node_id,
                        edge_kind: Some(edge.kind.clone()),
                        weight: edge.weight,
                    });
                } else {
                    steps.push(PathStep {
                        node_id,
                        edge_kind: None,
                        weight: 0,
                    });
                }
            } else {
                steps.push(PathStep {
                    node_id,
                    edge_kind: None,
                    weight: 0,
                });
            }
        }

        steps
    }

    // -----------------------------------------------------------------------
    // Task 7: transitive_dependents (BFS)
    // -----------------------------------------------------------------------

    /// Returns all transitive dependents of `node_id` up to `max_depth` hops
    /// away, following incoming edges.
    ///
    /// Each entry is `(dependent_id, depth)`.  Results are sorted by depth
    /// ascending, then by name ascending.  Returns an empty `Vec` if the node
    /// does not exist.
    pub fn transitive_dependents(&self, node_id: &str, max_depth: usize) -> Vec<(String, usize)> {
        let start_idx = match self.graph.get_index(node_id) {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let raw = self.graph.raw_graph();
        let mut visited: HashMap<NodeIndex, usize> = HashMap::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        queue.push_back((start_idx, 0));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            for edge_ref in raw.edges_directed(current, Direction::Incoming) {
                let neighbor = edge_ref.source();
                if neighbor != start_idx && !visited.contains_key(&neighbor) {
                    let new_depth = depth + 1;
                    visited.insert(neighbor, new_depth);
                    queue.push_back((neighbor, new_depth));
                }
            }
        }

        let mut results: Vec<(String, usize)> = visited
            .into_iter()
            .map(|(idx, depth)| (raw[idx].id.clone(), depth))
            .collect();
        results.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        results
    }

    // -----------------------------------------------------------------------
    // Task 8: explain
    // -----------------------------------------------------------------------

    /// Returns a comprehensive report about a single node, combining graph
    /// structure, metrics, community, and cycle information.
    ///
    /// Returns `None` if the node does not exist.
    pub fn explain(&self, node_id: &str) -> Option<ExplainReport> {
        let node = self.graph.get_node(node_id)?;

        // Find matching NodeMetrics
        let metrics = self.metrics.iter().find(|m| m.id == node_id);

        let explain_metrics = ExplainMetrics {
            score: metrics.map(|m| m.score).unwrap_or(0.0),
            betweenness: metrics.map(|m| m.betweenness).unwrap_or(0.0),
            pagerank: metrics.map(|m| m.pagerank).unwrap_or(0.0),
            in_degree: metrics.map(|m| m.in_degree).unwrap_or(0),
            out_degree: metrics.map(|m| m.out_degree).unwrap_or(0),
        };

        let community_id = metrics.map(|m| m.community_id).unwrap_or(0);
        let in_cycle = metrics.map(|m| m.in_cycle).unwrap_or(false);

        // Find cycle peers
        let cycle_peers: Vec<String> = self
            .cycles
            .iter()
            .filter(|cg| cg.node_ids.iter().any(|id| id == node_id))
            .flat_map(|cg| {
                cg.node_ids
                    .iter()
                    .filter(|id| id.as_str() != node_id)
                    .cloned()
            })
            .collect();

        // Direct dependents and dependencies
        let direct_dependents: Vec<String> = self
            .dependents(node_id)
            .into_iter()
            .map(|(id, _, _, _)| id)
            .collect();

        let direct_dependencies: Vec<String> = self
            .dependencies(node_id)
            .into_iter()
            .map(|(id, _, _, _)| id)
            .collect();

        // Transitive dependents (max_depth=10, take top 10)
        let trans_deps = self.transitive_dependents(node_id, 10);
        let transitive_dependent_count = trans_deps.len();
        let top_transitive_dependents: Vec<String> =
            trans_deps.into_iter().take(10).map(|(id, _)| id).collect();

        Some(ExplainReport {
            node_id: node_id.to_string(),
            kind: node.kind.clone(),
            file_path: node.file_path.clone(),
            language: node.language.clone(),
            metrics: explain_metrics,
            community_id,
            in_cycle,
            cycle_peers,
            direct_dependents,
            direct_dependencies,
            transitive_dependent_count,
            top_transitive_dependents,
        })
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
        Node::module(
            id,
            format!("{}.py", id.replace('.', "/")),
            Language::Python,
            1,
            true,
        )
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

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
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
        assert!(
            results.is_empty(),
            "all nodes are Module, filtering by Function should return empty"
        );
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
        assert!(
            suggestions.len() <= 3,
            "suggest should return at most 3 results, got {}",
            suggestions.len()
        );
    }

    // -----------------------------------------------------------------------
    // Task 4: dependents and dependencies
    // -----------------------------------------------------------------------

    #[test]
    fn dependents_returns_incoming() {
        let engine = build_engine();
        let deps = engine.dependents("app.utils");
        let ids: Vec<&str> = deps.iter().map(|(id, _, _, _)| id.as_str()).collect();
        assert_eq!(ids.len(), 2, "app.utils should have 2 dependents");
        assert!(ids.contains(&"app.main"));
        assert!(ids.contains(&"app.services.llm"));
    }

    #[test]
    fn dependencies_returns_outgoing() {
        let engine = build_engine();
        let deps = engine.dependencies("app.main");
        let ids: Vec<&str> = deps.iter().map(|(id, _, _, _)| id.as_str()).collect();
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

    // -----------------------------------------------------------------------
    // Task 5: shortest_path
    // -----------------------------------------------------------------------

    #[test]
    fn shortest_path_direct() {
        let engine = build_engine();
        let path = engine.shortest_path("app.main", "app.utils").unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].node_id, "app.main");
        assert_eq!(path[0].edge_kind, Some(EdgeKind::Imports));
        assert_eq!(path[1].node_id, "app.utils");
        assert_eq!(path[1].edge_kind, None);
        assert_eq!(path[1].weight, 0);
    }

    #[test]
    fn shortest_path_transitive() {
        // Build a→b→c chain
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_edge("a", "b", Edge::imports(1));
        graph.add_edge("b", "c", Edge::imports(2));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let path = engine.shortest_path("a", "c").unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].node_id, "a");
        assert_eq!(path[1].node_id, "b");
        assert_eq!(path[2].node_id, "c");
    }

    #[test]
    fn shortest_path_no_route() {
        let engine = build_engine();
        // os has no outgoing edges
        let path = engine.shortest_path("os", "app.main");
        assert!(path.is_none());
    }

    #[test]
    fn shortest_path_unknown_node() {
        let engine = build_engine();
        let path = engine.shortest_path("nonexistent", "app.main");
        assert!(path.is_none());
    }

    // -----------------------------------------------------------------------
    // Task 6: all_paths
    // -----------------------------------------------------------------------

    #[test]
    fn all_paths_finds_multiple() {
        // Diamond: a→b→d, a→c→d
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_node(module("d"));
        graph.add_edge("a", "b", Edge::imports(1));
        graph.add_edge("a", "c", Edge::imports(2));
        graph.add_edge("b", "d", Edge::imports(3));
        graph.add_edge("c", "d", Edge::imports(4));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let paths = engine.all_paths("a", "d", 10, 10);
        assert_eq!(paths.len(), 2, "should find 2 paths from a to d");
    }

    #[test]
    fn all_paths_respects_max_depth() {
        // Chain: a→b→c→d→e (4 hops)
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_node(module("d"));
        graph.add_node(module("e"));
        graph.add_edge("a", "b", Edge::imports(1));
        graph.add_edge("b", "c", Edge::imports(2));
        graph.add_edge("c", "d", Edge::imports(3));
        graph.add_edge("d", "e", Edge::imports(4));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let paths = engine.all_paths("a", "e", 2, 10);
        assert!(
            paths.is_empty(),
            "max_depth=2 should not reach e (4 hops away)"
        );
    }

    #[test]
    fn all_paths_respects_max_count() {
        // Diamond: a→b→d, a→c→d
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_node(module("d"));
        graph.add_edge("a", "b", Edge::imports(1));
        graph.add_edge("a", "c", Edge::imports(2));
        graph.add_edge("b", "d", Edge::imports(3));
        graph.add_edge("c", "d", Edge::imports(4));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let paths = engine.all_paths("a", "d", 10, 1);
        assert_eq!(paths.len(), 1, "max_paths=1 should return only 1 path");
    }

    #[test]
    fn all_paths_no_route() {
        let engine = build_engine();
        let paths = engine.all_paths("os", "app.main", 10, 10);
        assert!(paths.is_empty());
    }

    // -----------------------------------------------------------------------
    // Task 7: transitive_dependents
    // -----------------------------------------------------------------------

    #[test]
    fn transitive_dependents_finds_all() {
        let engine = build_engine();
        let deps = engine.transitive_dependents("app.utils", 10);
        let ids: Vec<&str> = deps.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids.len(),
            2,
            "app.utils should have 2 transitive dependents"
        );
        assert!(ids.contains(&"app.main"));
        assert!(ids.contains(&"app.services.llm"));
        // Both are at depth 1
        for (_, depth) in &deps {
            assert_eq!(*depth, 1);
        }
    }

    #[test]
    fn transitive_dependents_tracks_depth() {
        // Build c→b→a
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_edge("c", "b", Edge::imports(1));
        graph.add_edge("b", "a", Edge::imports(2));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let deps = engine.transitive_dependents("a", 10);
        assert_eq!(deps.len(), 2);
        // b at depth 1, c at depth 2
        assert!(deps.iter().any(|(id, d)| id == "b" && *d == 1));
        assert!(deps.iter().any(|(id, d)| id == "c" && *d == 2));
    }

    #[test]
    fn transitive_dependents_respects_max_depth() {
        // Build c→b→a
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_edge("c", "b", Edge::imports(1));
        graph.add_edge("b", "a", Edge::imports(2));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let deps = engine.transitive_dependents("a", 1);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].0, "b");
        assert_eq!(deps[0].1, 1);
    }

    // -----------------------------------------------------------------------
    // Task 8: explain
    // -----------------------------------------------------------------------

    #[test]
    fn explain_known_node() {
        let engine = build_engine();
        let report = engine.explain("app.main").unwrap();
        assert_eq!(report.node_id, "app.main");
        assert_eq!(report.kind, NodeKind::Module);
        assert_eq!(report.direct_dependencies.len(), 3);
        assert!(!report.in_cycle);
    }

    #[test]
    fn explain_unknown_node() {
        let engine = build_engine();
        let report = engine.explain("nonexistent");
        assert!(report.is_none());
    }

    #[test]
    fn explain_shows_cycle_peers() {
        // Build a→b→c→a cycle
        let mut graph = CodeGraph::new();
        graph.add_node(module("a"));
        graph.add_node(module("b"));
        graph.add_node(module("c"));
        graph.add_edge("a", "b", Edge::imports(1));
        graph.add_edge("b", "c", Edge::imports(2));
        graph.add_edge("c", "a", Edge::imports(3));

        let metrics =
            crate::metrics::compute_metrics(&graph, &crate::metrics::ScoringWeights::default());
        let communities = crate::community::detect_communities(&graph);
        let cycles = crate::cycles::find_sccs(&graph);
        let engine = QueryEngine::from_analyzed(graph, metrics, communities, cycles);

        let report = engine.explain("a").unwrap();
        assert!(report.in_cycle, "a should be in a cycle");
        assert_eq!(report.cycle_peers.len(), 2, "a should have 2 cycle peers");
        assert!(report.cycle_peers.contains(&"b".to_string()));
        assert!(report.cycle_peers.contains(&"c".to_string()));
    }
}
