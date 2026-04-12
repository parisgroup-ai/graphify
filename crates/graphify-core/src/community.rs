use std::collections::HashMap;

use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::graph::CodeGraph;

// ---------------------------------------------------------------------------
// Community
// ---------------------------------------------------------------------------

/// A group of nodes that form a community in the graph.
#[derive(Debug, Clone)]
pub struct Community {
    pub id: usize,
    pub members: Vec<String>, // node IDs
}

// ---------------------------------------------------------------------------
// detect_communities  (Louvain algorithm)
// ---------------------------------------------------------------------------

/// Detects communities using the Louvain algorithm.
///
/// Steps:
/// 1. Build an undirected adjacency map (sum edge weights in both directions).
/// 2. Compute total edge weight `m`.
/// 3. Initialize each node in its own community.
/// 4. Phase 1 — local moves: for each node, compute the modularity gain of
///    moving it to each neighbour's community and commit the best positive move.
///    Repeat until no improvement or 20 iterations exhausted.
/// 5. Group nodes by final community assignment, normalize IDs to 0..n.
///
/// Edge cases:
/// - Empty graph → empty result.
/// - No edges → every node in its own community.
pub fn detect_communities(graph: &CodeGraph) -> Vec<Community> {
    let raw = graph.raw_graph();
    let n = raw.node_count();
    if n == 0 {
        return Vec::new();
    }

    // Collect node indices and build ID→position map.
    let all_indices: Vec<_> = raw.node_indices().collect();
    let pos: HashMap<petgraph::graph::NodeIndex, usize> = all_indices
        .iter()
        .enumerate()
        .map(|(i, &idx)| (idx, i))
        .collect();

    // Build undirected adjacency: adj[u][v] = sum of edge weights in both directions.
    let mut adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); n];
    let mut total_weight = 0.0_f64;

    for &src in &all_indices {
        let u = pos[&src];
        for edge_ref in raw.edges_directed(src, Direction::Outgoing) {
            let tgt = edge_ref.target();
            let v = pos[&tgt];
            let w = edge_ref.weight().weight as f64;
            if u != v {
                *adj[u].entry(v).or_insert(0.0) += w;
                *adj[v].entry(u).or_insert(0.0) += w;
                total_weight += w; // each directed edge counted once here…
            }
        }
    }
    // total_weight currently double-counts (added for each direction of the
    // undirected edge). Divide by 2 to get the conventional m.
    let m = total_weight / 2.0;

    // Degree of each node (sum of adjacency weights in the undirected sense).
    let degree: Vec<f64> = (0..n).map(|u| adj[u].values().sum()).collect();

    // community[u] = community label for node u (initially each own label).
    let mut community: Vec<usize> = (0..n).collect();

    if m == 0.0 {
        // No edges — every node is its own community.
        return build_communities(&community, &all_indices, raw);
    }

    // Phase 1: local greedy moves.
    for _iter in 0..20 {
        let mut improved = false;

        for u in 0..n {
            let current_comm = community[u];

            // Compute k_i_in for each neighbouring community:
            // sum of weights from u to nodes in that community.
            let mut comm_weight: HashMap<usize, f64> = HashMap::new();
            for (&v, &w) in &adj[u] {
                *comm_weight.entry(community[v]).or_insert(0.0) += w;
            }

            // Sigma_tot[c] = sum of degrees of all nodes in community c
            // (excluding u temporarily).
            let mut sigma_tot: HashMap<usize, f64> = HashMap::new();
            for v in 0..n {
                if v != u {
                    *sigma_tot.entry(community[v]).or_insert(0.0) += degree[v];
                }
            }

            let ki = degree[u];

            // Modularity gain of removing u from current_comm and placing in
            // community c:  ΔQ = [k_i_in(c) / m] - [sigma_tot(c) * k_i / (2m²)]
            // We compare candidates relative to the gain of staying put (0).
            let mut best_gain = 0.0_f64;
            let mut best_comm = current_comm;

            for (&c, &k_i_in) in &comm_weight {
                if c == current_comm {
                    continue;
                }
                let s_tot = sigma_tot.get(&c).copied().unwrap_or(0.0);
                let gain = k_i_in / m - s_tot * ki / (2.0 * m * m);
                if gain > best_gain {
                    best_gain = gain;
                    best_comm = c;
                }
            }

            if best_comm != current_comm {
                community[u] = best_comm;
                improved = true;
            }
        }

        if !improved {
            break;
        }
    }

    build_communities(&community, &all_indices, raw)
}

/// Groups nodes by their community label, normalises IDs to 0..n, and returns
/// a sorted `Vec<Community>`.
fn build_communities(
    community: &[usize],
    all_indices: &[petgraph::graph::NodeIndex],
    raw: &petgraph::graph::DiGraph<crate::types::Node, crate::types::Edge>,
) -> Vec<Community> {
    // Group node IDs by community label.
    let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
    for (i, &idx) in all_indices.iter().enumerate() {
        groups
            .entry(community[i])
            .or_default()
            .push(raw[idx].id.clone());
    }

    // Collect and sort members within each group for determinism.
    let mut communities: Vec<Community> = groups
        .into_values()
        .enumerate()
        .map(|(new_id, mut members)| {
            members.sort();
            Community { id: new_id, members }
        })
        .collect();

    // Sort communities by their first member for determinism, then re-assign IDs.
    communities.sort_by(|a, b| a.members[0].cmp(&b.members[0]));
    for (i, c) in communities.iter_mut().enumerate() {
        c.id = i;
    }

    communities
}

// ---------------------------------------------------------------------------
// label_propagation  (fallback)
// ---------------------------------------------------------------------------

/// Detects communities using the Label Propagation algorithm.
///
/// Each node starts with a unique label.  Each iteration every node adopts the
/// most common label among its neighbours (both in- and out-neighbours in the
/// directed graph, treating it as undirected).  Repeats until stable or 50
/// iterations exhausted.
pub fn label_propagation(graph: &CodeGraph) -> Vec<Community> {
    let raw = graph.raw_graph();
    let n = raw.node_count();
    if n == 0 {
        return Vec::new();
    }

    let all_indices: Vec<_> = raw.node_indices().collect();
    let pos: HashMap<petgraph::graph::NodeIndex, usize> = all_indices
        .iter()
        .enumerate()
        .map(|(i, &idx)| (idx, i))
        .collect();

    // Build undirected neighbour list.
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &src in &all_indices {
        let u = pos[&src];
        for tgt in raw.neighbors_directed(src, Direction::Outgoing) {
            let v = pos[&tgt];
            if u != v {
                neighbours[u].push(v);
                neighbours[v].push(u);
            }
        }
    }
    // Deduplicate neighbour lists.
    for nbrs in neighbours.iter_mut() {
        nbrs.sort_unstable();
        nbrs.dedup();
    }

    // Initial labels: each node gets its own unique label.
    let mut labels: Vec<usize> = (0..n).collect();

    for _iter in 0..50 {
        let mut changed = false;
        // Iterate in a fixed order (deterministic).
        for u in 0..n {
            if neighbours[u].is_empty() {
                continue;
            }
            // Count neighbour labels.
            let mut label_count: HashMap<usize, usize> = HashMap::new();
            for &v in &neighbours[u] {
                *label_count.entry(labels[v]).or_insert(0) += 1;
            }
            // Pick most common (break ties by smallest label for determinism).
            let best = label_count
                .iter()
                .max_by_key(|&(label, &count)| (count, std::cmp::Reverse(*label)))
                .map(|(&label, _)| label)
                .unwrap_or(labels[u]);

            if best != labels[u] {
                labels[u] = best;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    build_communities(&labels, &all_indices, raw)
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

    /// Two dense clusters connected by a sparse bridge:
    ///
    /// Cluster 1: a ↔ b ↔ c  (bidirectional)
    /// Cluster 2: d ↔ e ↔ f  (bidirectional)
    /// Bridge:    c → d       (unidirectional)
    fn two_cluster_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        for id in &["a", "b", "c", "d", "e", "f"] {
            g.add_node(module(id));
        }
        // Cluster 1
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));
        g.add_edge("b", "c", Edge::imports(3));
        g.add_edge("c", "b", Edge::imports(4));
        // Cluster 2
        g.add_edge("d", "e", Edge::imports(5));
        g.add_edge("e", "d", Edge::imports(6));
        g.add_edge("e", "f", Edge::imports(7));
        g.add_edge("f", "e", Edge::imports(8));
        // Bridge
        g.add_edge("c", "d", Edge::imports(9));
        g
    }

    // -----------------------------------------------------------------------
    // detect_communities (Louvain)
    // -----------------------------------------------------------------------

    #[test]
    fn louvain_finds_at_least_two_communities() {
        let g = two_cluster_graph();
        let communities = detect_communities(&g);
        assert!(
            communities.len() >= 2,
            "expected ≥2 communities, got {}",
            communities.len()
        );
    }

    #[test]
    fn louvain_total_members_equals_node_count() {
        let g = two_cluster_graph();
        let communities = detect_communities(&g);
        let total: usize = communities.iter().map(|c| c.members.len()).sum();
        assert_eq!(total, 6, "total members across all communities must be 6");
    }

    #[test]
    fn louvain_single_node_returns_one_community() {
        let mut g = CodeGraph::new();
        g.add_node(module("solo"));
        let communities = detect_communities(&g);
        assert_eq!(communities.len(), 1);
        assert_eq!(communities[0].members.len(), 1);
        assert_eq!(communities[0].members[0], "solo");
    }

    #[test]
    fn louvain_no_edges_each_node_own_community() {
        let mut g = CodeGraph::new();
        g.add_node(module("x"));
        g.add_node(module("y"));
        let communities = detect_communities(&g);
        assert_eq!(communities.len(), 2, "two isolated nodes → two communities");
        let total: usize = communities.iter().map(|c| c.members.len()).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn louvain_community_ids_are_sequential() {
        let g = two_cluster_graph();
        let communities = detect_communities(&g);
        let mut ids: Vec<usize> = communities.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let expected: Vec<usize> = (0..ids.len()).collect();
        assert_eq!(ids, expected, "community IDs must be sequential starting from 0");
    }

    #[test]
    fn louvain_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let communities = detect_communities(&g);
        assert!(communities.is_empty());
    }

    // -----------------------------------------------------------------------
    // label_propagation
    // -----------------------------------------------------------------------

    #[test]
    fn label_propagation_finds_at_least_one_community() {
        let g = two_cluster_graph();
        let communities = label_propagation(&g);
        assert!(!communities.is_empty(), "should find at least one community");
    }

    #[test]
    fn label_propagation_total_members_equals_node_count() {
        let g = two_cluster_graph();
        let communities = label_propagation(&g);
        let total: usize = communities.iter().map(|c| c.members.len()).sum();
        assert_eq!(total, 6, "total members across all communities must be 6");
    }

    #[test]
    fn label_propagation_community_ids_are_sequential() {
        let g = two_cluster_graph();
        let communities = label_propagation(&g);
        let mut ids: Vec<usize> = communities.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let expected: Vec<usize> = (0..ids.len()).collect();
        assert_eq!(ids, expected, "community IDs must be sequential starting from 0");
    }

    #[test]
    fn label_propagation_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let communities = label_propagation(&g);
        assert!(communities.is_empty());
    }
}
