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
}
