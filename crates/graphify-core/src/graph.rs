use std::collections::HashMap;

use petgraph::{
    graph::{DiGraph, EdgeIndex, NodeIndex},
    visit::EdgeRef,
    Direction,
};

use crate::types::{Edge, EdgeKind, Language, Node};

// ---------------------------------------------------------------------------
// CodeGraph
// ---------------------------------------------------------------------------

/// A directed graph of code entities (modules, functions, classes) and their
/// relationships (imports, defines, calls).
///
/// Internally backed by [`petgraph::graph::DiGraph`] with an ID-keyed index
/// for O(1) node look-ups by string ID.
pub struct CodeGraph {
    graph: DiGraph<Node, Edge>,
    index: HashMap<String, NodeIndex>,
}

impl CodeGraph {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Returns an empty `CodeGraph`.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    /// Adds a node to the graph.
    ///
    /// If a node with the same `id` already exists the existing [`NodeIndex`]
    /// is returned without creating a duplicate.
    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        if let Some(&existing) = self.index.get(&node.id) {
            return existing;
        }
        let id = node.id.clone();
        let idx = self.graph.add_node(node);
        self.index.insert(id, idx);
        idx
    }

    /// Adds a directed edge from `source_id` to `target_id`.
    ///
    /// Rules:
    /// - If a node ID is unknown, a placeholder `Module` node is
    ///   auto-created (`is_local = false`, `line = 0`).
    /// - If an edge of the **same kind** already exists between the same two
    ///   nodes, its `weight` is incremented instead of adding a duplicate.
    pub fn add_edge(
        &mut self,
        source_id: &str,
        target_id: &str,
        edge: Edge,
    ) {
        let src = self.get_or_create_placeholder(source_id);
        let tgt = self.get_or_create_placeholder(target_id);

        // Check for an existing edge of the same kind between src → tgt.
        if let Some(existing_idx) = self.find_edge(src, tgt, &edge.kind) {
            self.graph[existing_idx].weight += 1;
        } else {
            self.graph.add_edge(src, tgt, edge);
        }
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns a reference to the node with the given `id`, if it exists.
    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.index.get(id).map(|&idx| &self.graph[idx])
    }

    /// Returns the [`NodeIndex`] for the node with the given `id`, if it exists.
    pub fn get_index(&self, id: &str) -> Option<NodeIndex> {
        self.index.get(id).copied()
    }

    /// Total number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Total number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Number of incoming edges for the node with the given `id`.
    ///
    /// Returns `0` if the node does not exist.
    pub fn in_degree(&self, id: &str) -> usize {
        match self.index.get(id) {
            Some(&idx) => self
                .graph
                .edges_directed(idx, Direction::Incoming)
                .count(),
            None => 0,
        }
    }

    /// Number of outgoing edges for the node with the given `id`.
    ///
    /// Returns `0` if the node does not exist.
    pub fn out_degree(&self, id: &str) -> usize {
        match self.index.get(id) {
            Some(&idx) => self
                .graph
                .edges_directed(idx, Direction::Outgoing)
                .count(),
            None => 0,
        }
    }

    /// Returns all node IDs in the graph (order is not guaranteed).
    pub fn node_ids(&self) -> Vec<&str> {
        self.index.keys().map(|s| s.as_str()).collect()
    }

    /// Returns IDs of nodes where `is_local == true` (order is not guaranteed).
    pub fn local_node_ids(&self) -> Vec<&str> {
        self.index
            .iter()
            .filter(|(_, &idx)| self.graph[idx].is_local)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Returns the index of an existing node or creates a placeholder module
    /// node for unknown IDs.
    fn get_or_create_placeholder(&mut self, id: &str) -> NodeIndex {
        if let Some(&idx) = self.index.get(id) {
            return idx;
        }
        let placeholder = Node::module(id, "", Language::Python, 0, false);
        self.add_node(placeholder)
    }

    /// Finds an edge of the given `kind` between `src` and `tgt`, returning
    /// its [`EdgeIndex`] if found.
    fn find_edge(
        &self,
        src: NodeIndex,
        tgt: NodeIndex,
        kind: &EdgeKind,
    ) -> Option<EdgeIndex> {
        self.graph
            .edges_directed(src, Direction::Outgoing)
            .find(|e| e.target() == tgt && &e.weight().kind == kind)
            .map(|e| e.id())
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language, Node, NodeKind};

    fn python_module(id: &str, is_local: bool) -> Node {
        Node::module(id, format!("{}.py", id.replace('.', "/")), Language::Python, 1, is_local)
    }

    fn python_function(id: &str, is_local: bool) -> Node {
        Node::symbol(id, NodeKind::Function, format!("{}.py", id.replace('.', "/")), Language::Python, 10, is_local)
    }

    // -----------------------------------------------------------------------
    // Basic node/edge counts
    // -----------------------------------------------------------------------

    #[test]
    fn empty_graph_has_zero_counts() {
        let g = CodeGraph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn add_nodes_and_edges_verify_counts() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("app.main", true));
        g.add_node(python_module("app.utils", true));
        assert_eq!(g.node_count(), 2);

        g.add_edge("app.main", "app.utils", Edge::imports(1));
        assert_eq!(g.edge_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Deduplication
    // -----------------------------------------------------------------------

    #[test]
    fn no_duplicate_nodes_same_id() {
        let mut g = CodeGraph::new();
        let idx1 = g.add_node(python_module("app.main", true));
        let idx2 = g.add_node(python_module("app.main", true));
        // Must return the same index
        assert_eq!(idx1, idx2);
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn second_add_node_call_is_idempotent() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("app.a", true));
        g.add_node(python_module("app.a", true));
        g.add_node(python_module("app.a", true));
        assert_eq!(g.node_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Edge weight increment
    // -----------------------------------------------------------------------

    #[test]
    fn edge_weight_increments_on_repeated_same_kind() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("a", true));
        g.add_node(python_module("b", true));

        g.add_edge("a", "b", Edge::calls(1));
        g.add_edge("a", "b", Edge::calls(2));
        g.add_edge("a", "b", Edge::calls(3));

        // Should still be a single edge with weight 3
        assert_eq!(g.edge_count(), 1);

        let idx_a = g.get_index("a").unwrap();
        let idx_b = g.get_index("b").unwrap();
        let weight = g
            .graph
            .edges_connecting(idx_a, idx_b)
            .next()
            .unwrap()
            .weight()
            .weight;
        assert_eq!(weight, 3);
    }

    #[test]
    fn different_edge_kinds_not_merged() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("a", true));
        g.add_node(python_module("b", true));

        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("a", "b", Edge::calls(2));
        g.add_edge("a", "b", Edge::defines(3));

        // Three separate edges, one per kind
        assert_eq!(g.edge_count(), 3);
    }

    #[test]
    fn same_kind_increments_weight_not_edge_count() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("x", true));
        g.add_node(python_module("y", true));

        g.add_edge("x", "y", Edge::imports(1));
        g.add_edge("x", "y", Edge::imports(5));

        assert_eq!(g.edge_count(), 1);

        let xi = g.get_index("x").unwrap();
        let yi = g.get_index("y").unwrap();
        let w = g
            .graph
            .edges_connecting(xi, yi)
            .next()
            .unwrap()
            .weight()
            .weight;
        assert_eq!(w, 2);
    }

    // -----------------------------------------------------------------------
    // Placeholder auto-creation
    // -----------------------------------------------------------------------

    #[test]
    fn add_edge_auto_creates_placeholder_nodes() {
        let mut g = CodeGraph::new();
        // Neither node exists yet
        g.add_edge("unknown.a", "unknown.b", Edge::imports(1));

        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);

        let node_a = g.get_node("unknown.a").unwrap();
        assert!(!node_a.is_local);
        assert_eq!(node_a.kind, NodeKind::Module);
    }

    // -----------------------------------------------------------------------
    // Degree counting
    // -----------------------------------------------------------------------

    #[test]
    fn degree_counting() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("hub", true));
        g.add_node(python_module("dep1", true));
        g.add_node(python_module("dep2", true));
        g.add_node(python_module("dep3", true));

        // dep1, dep2, dep3 all import hub
        g.add_edge("dep1", "hub", Edge::imports(1));
        g.add_edge("dep2", "hub", Edge::imports(1));
        g.add_edge("dep3", "hub", Edge::imports(1));

        assert_eq!(g.in_degree("hub"), 3);
        assert_eq!(g.out_degree("hub"), 0);
        assert_eq!(g.in_degree("dep1"), 0);
        assert_eq!(g.out_degree("dep1"), 1);
    }

    #[test]
    fn degree_returns_zero_for_unknown_id() {
        let g = CodeGraph::new();
        assert_eq!(g.in_degree("nonexistent"), 0);
        assert_eq!(g.out_degree("nonexistent"), 0);
    }

    #[test]
    fn degree_counts_all_edge_kinds() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("src", true));
        g.add_node(python_module("tgt", true));

        g.add_edge("src", "tgt", Edge::imports(1));
        g.add_edge("src", "tgt", Edge::calls(2));
        g.add_edge("src", "tgt", Edge::defines(3));

        // out_degree counts edges, not weight
        assert_eq!(g.out_degree("src"), 3);
        assert_eq!(g.in_degree("tgt"), 3);
    }

    // -----------------------------------------------------------------------
    // Local vs non-local filtering
    // -----------------------------------------------------------------------

    #[test]
    fn local_node_ids_filters_correctly() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("app.main", true));
        g.add_node(python_module("app.utils", true));
        g.add_node(python_module("os", false));
        g.add_node(python_module("json", false));

        let local = g.local_node_ids();
        assert_eq!(local.len(), 2);
        assert!(local.contains(&"app.main"));
        assert!(local.contains(&"app.utils"));
        assert!(!local.contains(&"os"));
        assert!(!local.contains(&"json"));
    }

    #[test]
    fn node_ids_returns_all() {
        let mut g = CodeGraph::new();
        g.add_node(python_module("a", true));
        g.add_node(python_module("b", false));

        let ids = g.node_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
    }

    // -----------------------------------------------------------------------
    // get_node correctness
    // -----------------------------------------------------------------------

    #[test]
    fn get_node_returns_correct_data() {
        let mut g = CodeGraph::new();
        let node = Node::module("app.svc", "app/svc.py", Language::Python, 7, true);
        g.add_node(node.clone());

        let retrieved = g.get_node("app.svc").unwrap();
        assert_eq!(retrieved.id, "app.svc");
        assert_eq!(retrieved.kind, NodeKind::Module);
        assert_eq!(retrieved.file_path.to_str().unwrap(), "app/svc.py");
        assert_eq!(retrieved.line, 7);
        assert!(retrieved.is_local);
    }

    #[test]
    fn get_node_returns_none_for_unknown_id() {
        let g = CodeGraph::new();
        assert!(g.get_node("not.there").is_none());
    }

    #[test]
    fn get_index_returns_none_for_unknown_id() {
        let g = CodeGraph::new();
        assert!(g.get_index("not.there").is_none());
    }

    #[test]
    fn get_index_returns_consistent_index() {
        let mut g = CodeGraph::new();
        let idx = g.add_node(python_module("m", true));
        assert_eq!(g.get_index("m"), Some(idx));
    }

    // -----------------------------------------------------------------------
    // Symbol nodes
    // -----------------------------------------------------------------------

    #[test]
    fn symbol_node_stored_and_retrieved() {
        let mut g = CodeGraph::new();
        let func = python_function("app.helpers.parse", true);
        g.add_node(func);

        let node = g.get_node("app.helpers.parse").unwrap();
        assert_eq!(node.kind, NodeKind::Function);
        assert!(node.is_local);
    }

    // -----------------------------------------------------------------------
    // Default trait
    // -----------------------------------------------------------------------

    #[test]
    fn default_creates_empty_graph() {
        let g = CodeGraph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }
}
