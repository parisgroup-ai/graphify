use std::collections::HashSet;

use petgraph::algo::tarjan_scc;
use petgraph::graph::NodeIndex;
use petgraph::visit::NodeFiltered;

use crate::graph::CodeGraph;

// ---------------------------------------------------------------------------
// CycleGroup
// ---------------------------------------------------------------------------

/// A strongly connected component (SCC) with more than one node — i.e. a real
/// cycle in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleGroup {
    /// Node IDs that participate in the cycle.
    pub node_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// find_sccs
// ---------------------------------------------------------------------------

/// Returns all strongly connected components with more than one node.
///
/// Uses Tarjan's SCC algorithm via [`petgraph::algo::tarjan_scc`].
/// Single-node SCCs (self-loops aside) are filtered out — they are not cycles.
pub fn find_sccs(graph: &CodeGraph) -> Vec<CycleGroup> {
    let raw = graph.raw_graph();
    tarjan_scc(raw)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let mut node_ids: Vec<String> = scc.iter().map(|&idx| raw[idx].id.clone()).collect();
            node_ids.sort(); // deterministic order
            CycleGroup { node_ids }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// find_sccs_excluding
// ---------------------------------------------------------------------------

/// Same as [`find_sccs`] but pretends the nodes in `excluded_ids` do not
/// exist. Cycles whose only cycle-making edges route through an excluded
/// node (i.e. barrel-only cycles, per BUG-015) are dropped; cycles that
/// exist independently of the excluded nodes are preserved.
///
/// Excluded IDs not present in the graph are ignored. An empty set falls
/// through to [`find_sccs`] with no filtering overhead.
pub fn find_sccs_excluding(graph: &CodeGraph, excluded_ids: &HashSet<&str>) -> Vec<CycleGroup> {
    if excluded_ids.is_empty() {
        return find_sccs(graph);
    }
    let raw = graph.raw_graph();
    let excluded_indices: HashSet<NodeIndex> = raw
        .node_indices()
        .filter(|&i| excluded_ids.contains(raw[i].id.as_str()))
        .collect();
    if excluded_indices.is_empty() {
        return find_sccs(graph);
    }
    let filtered = NodeFiltered::from_fn(raw, |n| !excluded_indices.contains(&n));
    tarjan_scc(&filtered)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let mut node_ids: Vec<String> = scc.iter().map(|&idx| raw[idx].id.clone()).collect();
            node_ids.sort();
            CycleGroup { node_ids }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// find_simple_cycles
// ---------------------------------------------------------------------------

/// Returns simple cycles found by DFS, capped at `max_cycles`.
///
/// Each cycle is represented as an ordered `Vec<String>` of node IDs starting
/// with the lexicographically smallest ID (canonical rotation) so that
/// duplicate cycles are detected and removed.
pub fn find_simple_cycles(graph: &CodeGraph, max_cycles: usize) -> Vec<Vec<String>> {
    let raw = graph.raw_graph();
    let node_count = raw.node_count();
    if node_count == 0 || max_cycles == 0 {
        return Vec::new();
    }

    let mut results: Vec<Vec<String>> = Vec::new();
    let mut seen_cycles: HashSet<Vec<String>> = HashSet::new();

    // Collect node indices for iteration.
    let all_indices: Vec<_> = raw.node_indices().collect();

    for &start in &all_indices {
        if results.len() >= max_cycles {
            break;
        }
        // DFS stack: each entry is (current_node_index, path_so_far)
        let mut stack: Vec<(petgraph::graph::NodeIndex, Vec<petgraph::graph::NodeIndex>)> =
            vec![(start, vec![start])];

        while let Some((current, path)) = stack.pop() {
            if results.len() >= max_cycles {
                break;
            }
            for neighbor in raw.neighbors(current) {
                if neighbor == start && path.len() > 1 {
                    // Found a cycle back to start.
                    let cycle_ids: Vec<String> =
                        path.iter().map(|&idx| raw[idx].id.clone()).collect();
                    let canonical = canonical_cycle(cycle_ids);
                    if seen_cycles.insert(canonical.clone()) {
                        results.push(canonical);
                        if results.len() >= max_cycles {
                            break;
                        }
                    }
                } else if !path.contains(&neighbor) && neighbor.index() > start.index() {
                    // Only explore nodes with higher index than start to avoid
                    // reporting the same cycle from multiple starting points.
                    let mut new_path = path.clone();
                    new_path.push(neighbor);
                    stack.push((neighbor, new_path));
                }
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// find_simple_cycles_excluding
// ---------------------------------------------------------------------------

/// Same as [`find_simple_cycles`] but skips nodes whose IDs are in
/// `excluded_ids` during DFS traversal. See [`find_sccs_excluding`] for the
/// cycle-preservation semantics.
pub fn find_simple_cycles_excluding(
    graph: &CodeGraph,
    max_cycles: usize,
    excluded_ids: &HashSet<&str>,
) -> Vec<Vec<String>> {
    if excluded_ids.is_empty() {
        return find_simple_cycles(graph, max_cycles);
    }
    let raw = graph.raw_graph();
    let node_count = raw.node_count();
    if node_count == 0 || max_cycles == 0 {
        return Vec::new();
    }
    let excluded: HashSet<NodeIndex> = raw
        .node_indices()
        .filter(|&i| excluded_ids.contains(raw[i].id.as_str()))
        .collect();
    if excluded.is_empty() {
        return find_simple_cycles(graph, max_cycles);
    }

    let mut results: Vec<Vec<String>> = Vec::new();
    let mut seen_cycles: HashSet<Vec<String>> = HashSet::new();
    let all_indices: Vec<_> = raw.node_indices().collect();

    for &start in &all_indices {
        if excluded.contains(&start) {
            continue;
        }
        if results.len() >= max_cycles {
            break;
        }
        let mut stack: Vec<(NodeIndex, Vec<NodeIndex>)> = vec![(start, vec![start])];

        while let Some((current, path)) = stack.pop() {
            if results.len() >= max_cycles {
                break;
            }
            for neighbor in raw.neighbors(current) {
                if excluded.contains(&neighbor) {
                    continue;
                }
                if neighbor == start && path.len() > 1 {
                    let cycle_ids: Vec<String> =
                        path.iter().map(|&idx| raw[idx].id.clone()).collect();
                    let canonical = canonical_cycle(cycle_ids);
                    if seen_cycles.insert(canonical.clone()) {
                        results.push(canonical);
                        if results.len() >= max_cycles {
                            break;
                        }
                    }
                } else if !path.contains(&neighbor) && neighbor.index() > start.index() {
                    let mut new_path = path.clone();
                    new_path.push(neighbor);
                    stack.push((neighbor, new_path));
                }
            }
        }
    }

    results
}

/// Rotates a cycle so it starts with the lexicographically smallest node ID.
fn canonical_cycle(mut cycle: Vec<String>) -> Vec<String> {
    if cycle.is_empty() {
        return cycle;
    }
    let min_pos = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, id)| id.as_str())
        .map(|(i, _)| i)
        .unwrap_or(0);
    cycle.rotate_left(min_pos);
    cycle
}

// ---------------------------------------------------------------------------
// is_in_cycle
// ---------------------------------------------------------------------------

/// Returns `true` if the node with the given `id` participates in any SCC
/// with more than one node (i.e. is part of a cycle).
pub fn is_in_cycle(graph: &CodeGraph, node_id: &str) -> bool {
    find_sccs(graph)
        .iter()
        .any(|group| group.node_ids.iter().any(|id| id == node_id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language, Node};

    fn module(id: &str) -> Node {
        Node::module(id, format!("{}.py", id), Language::Python, 1, true)
    }

    /// Builds: a → b → c → a  (3-node cycle)
    fn graph_with_cycle() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("c"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(2));
        g.add_edge("c", "a", Edge::imports(3));
        g
    }

    /// Builds: a → b → c  (DAG, no back-edge)
    fn graph_no_cycle() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("c"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(2));
        g
    }

    // -----------------------------------------------------------------------
    // find_sccs
    // -----------------------------------------------------------------------

    #[test]
    fn find_sccs_with_cycle_finds_one_scc_with_three_nodes() {
        let g = graph_with_cycle();
        let sccs = find_sccs(&g);
        assert_eq!(sccs.len(), 1, "expected exactly one SCC");
        let group = &sccs[0];
        assert_eq!(group.node_ids.len(), 3);
        let mut ids = group.node_ids.clone();
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn find_sccs_without_cycle_finds_none() {
        let g = graph_no_cycle();
        let sccs = find_sccs(&g);
        assert_eq!(sccs.len(), 0, "DAG should have no SCCs with >1 node");
    }

    #[test]
    fn find_sccs_on_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        assert!(find_sccs(&g).is_empty());
    }

    // -----------------------------------------------------------------------
    // is_in_cycle
    // -----------------------------------------------------------------------

    #[test]
    fn is_in_cycle_true_for_all_nodes_in_cyclic_graph() {
        let g = graph_with_cycle();
        assert!(is_in_cycle(&g, "a"));
        assert!(is_in_cycle(&g, "b"));
        assert!(is_in_cycle(&g, "c"));
    }

    #[test]
    fn is_in_cycle_false_for_all_nodes_in_dag() {
        let g = graph_no_cycle();
        assert!(!is_in_cycle(&g, "a"));
        assert!(!is_in_cycle(&g, "b"));
        assert!(!is_in_cycle(&g, "c"));
    }

    #[test]
    fn is_in_cycle_false_for_unknown_node() {
        let g = graph_with_cycle();
        assert!(!is_in_cycle(&g, "z"));
    }

    // -----------------------------------------------------------------------
    // find_simple_cycles
    // -----------------------------------------------------------------------

    #[test]
    fn find_simple_cycles_finds_the_three_node_cycle() {
        let g = graph_with_cycle();
        let cycles = find_simple_cycles(&g, 500);
        assert_eq!(cycles.len(), 1, "expected exactly one simple cycle");
        let cycle = &cycles[0];
        assert_eq!(cycle.len(), 3);
        // Canonical form starts with "a"
        assert_eq!(cycle[0], "a");
        // Must contain all three nodes
        let mut sorted = cycle.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }

    #[test]
    fn find_simple_cycles_dag_returns_empty() {
        let g = graph_no_cycle();
        let cycles = find_simple_cycles(&g, 500);
        assert!(cycles.is_empty(), "DAG should have no simple cycles");
    }

    #[test]
    fn find_simple_cycles_on_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let cycles = find_simple_cycles(&g, 500);
        assert!(cycles.is_empty());
    }

    #[test]
    fn find_simple_cycles_respects_max_cycles_cap() {
        // Build a graph with two independent cycles: a→b→a and c→d→c
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("c"));
        g.add_node(module("d"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));
        g.add_edge("c", "d", Edge::imports(3));
        g.add_edge("d", "c", Edge::imports(4));

        // Cap at 1 — should not exceed 1 result
        let cycles = find_simple_cycles(&g, 1);
        assert!(cycles.len() <= 1);
    }

    #[test]
    fn find_simple_cycles_zero_cap_returns_empty() {
        let g = graph_with_cycle();
        let cycles = find_simple_cycles(&g, 0);
        assert!(cycles.is_empty());
    }

    // -----------------------------------------------------------------------
    // find_sccs_excluding / find_simple_cycles_excluding — BUG-015
    // -----------------------------------------------------------------------

    /// Graph: a ↔ x ↔ b — the only cycles (a↔x and b↔x) route through
    /// barrel node x. Independent cycle a ↔ b does NOT exist.
    fn graph_barrel_only() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("x"));
        g.add_edge("a", "x", Edge::imports(1));
        g.add_edge("x", "a", Edge::imports(1));
        g.add_edge("b", "x", Edge::imports(1));
        g.add_edge("x", "b", Edge::imports(1));
        g
    }

    #[test]
    fn bug_015_find_sccs_excluding_drops_barrel_only_cycle() {
        let g = graph_barrel_only();
        // Without exclusion: one SCC spanning {a, b, x}.
        assert_eq!(find_sccs(&g).len(), 1);

        let excluded: HashSet<&str> = ["x"].into_iter().collect();
        let sccs = find_sccs_excluding(&g, &excluded);
        assert!(
            sccs.is_empty(),
            "barrel-only cycle should be dropped when x is excluded, got {:?}",
            sccs
        );
    }

    #[test]
    fn bug_015_find_sccs_excluding_preserves_direct_cycle() {
        // a ↔ b directly plus a dangling barrel node x that is allowlisted.
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("x"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(1));

        let excluded: HashSet<&str> = ["x"].into_iter().collect();
        let sccs = find_sccs_excluding(&g, &excluded);
        assert_eq!(sccs.len(), 1, "direct a↔b cycle must survive x exclusion");
        assert_eq!(sccs[0].node_ids, vec!["a", "b"]);
    }

    #[test]
    fn bug_015_find_simple_cycles_excluding_drops_barrel_only_cycle() {
        let g = graph_barrel_only();
        // Without exclusion: two simple cycles ([a,x] and [b,x]).
        assert_eq!(find_simple_cycles(&g, 500).len(), 2);

        let excluded: HashSet<&str> = ["x"].into_iter().collect();
        let cycles = find_simple_cycles_excluding(&g, 500, &excluded);
        assert!(cycles.is_empty(), "got {:?}", cycles);
    }

    #[test]
    fn bug_015_find_simple_cycles_excluding_preserves_direct_cycle() {
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("x"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(1));

        let excluded: HashSet<&str> = ["x"].into_iter().collect();
        let cycles = find_simple_cycles_excluding(&g, 500, &excluded);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0], vec!["a", "b"]);
    }

    #[test]
    fn find_sccs_excluding_with_empty_set_matches_find_sccs() {
        let g = graph_with_cycle();
        assert_eq!(find_sccs(&g), find_sccs_excluding(&g, &HashSet::new()));
    }

    #[test]
    fn find_sccs_excluding_ignores_unknown_ids() {
        let g = graph_with_cycle();
        let excluded: HashSet<&str> = ["does_not_exist"].into_iter().collect();
        assert_eq!(find_sccs(&g), find_sccs_excluding(&g, &excluded));
    }

    /// Regression fixture mirroring the cursos report shape:
    /// `src` barrel plus `src.context → src` and `src → src.context`.
    #[test]
    fn bug_015_cursos_like_barrel_cycle_is_dropped() {
        let mut g = CodeGraph::new();
        g.add_node(module("src"));
        g.add_node(module("src.context"));
        g.add_node(module("src.trpc"));
        g.add_node(module("src.root"));
        // Barrel re-exports submodules:
        g.add_edge("src", "src.context", Edge::imports(1));
        g.add_edge("src", "src.trpc", Edge::imports(1));
        g.add_edge("src", "src.root", Edge::imports(1));
        // Submodules import back through the barrel (synthetic FEAT-028 edges):
        g.add_edge("src.context", "src", Edge::imports(1));
        g.add_edge("src.trpc", "src.context", Edge::imports(1));
        g.add_edge("src.root", "src.trpc", Edge::imports(1));

        // Without exclusion there is at least one SCC.
        assert!(!find_sccs(&g).is_empty());

        let excluded: HashSet<&str> = ["src"].into_iter().collect();
        assert!(find_sccs_excluding(&g, &excluded).is_empty());
        assert!(find_simple_cycles_excluding(&g, 500, &excluded).is_empty());
    }
}
