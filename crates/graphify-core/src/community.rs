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
            let mut candidates: Vec<(usize, f64)> = comm_weight
                .iter()
                .map(|(&c, &k_i_in)| (c, k_i_in))
                .collect();
            candidates.sort_by_key(|(c, _)| *c);

            for (c, k_i_in) in candidates {
                if c == current_comm {
                    continue;
                }
                let s_tot = sigma_tot.get(&c).copied().unwrap_or(0.0);
                let gain = k_i_in / m - s_tot * ki / (2.0 * m * m);
                if gain > best_gain + 1e-12
                    || ((gain - best_gain).abs() <= 1e-12 && gain > 0.0 && c < best_comm)
                {
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

    // Phase 2: merge singleton communities.
    //
    // On sparse graphs Louvain Phase 1 often leaves many nodes in singleton
    // communities because there is no modularity gain from merging isolated
    // nodes.  Two post-processing steps reduce noise:
    //
    // (a) Singletons that DO have neighbours → absorb into the community of
    //     the highest-weight neighbour.
    // (b) Remaining singletons (truly isolated, zero edges) → group together
    //     into a single "unclustered" community.
    merge_singletons(&mut community, &adj, n);

    build_communities(&community, &all_indices, raw)
}

/// Merge singleton communities after Louvain Phase 1.
///
/// Step (a): any singleton whose node has at least one neighbour is absorbed
/// into the community of its highest-weight neighbour.
///
/// Step (b): any remaining singletons (isolated nodes with no edges) are all
/// assigned to a single shared community so the report is not cluttered with
/// dozens of one-node groups.
fn merge_singletons(community: &mut [usize], adj: &[HashMap<usize, f64>], n: usize) {
    // Count members per community.
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for &c in community.iter() {
        *sizes.entry(c).or_insert(0) += 1;
    }

    // (a) Singletons with neighbours → absorb into best neighbour's community.
    for u in 0..n {
        if sizes[&community[u]] > 1 {
            continue; // not a singleton
        }
        let mut best_comm = community[u];
        let mut best_w = 0.0_f64;
        let mut neighbors: Vec<(usize, f64, usize)> =
            adj[u].iter().map(|(&v, &w)| (community[v], w, v)).collect();
        neighbors.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.2.cmp(&b.2))
        });
        for (neighbor_comm, w, _neighbor_idx) in neighbors {
            if w > best_w + 1e-12
                || ((w - best_w).abs() <= 1e-12 && w > 0.0 && neighbor_comm < best_comm)
            {
                best_w = w;
                best_comm = neighbor_comm;
            }
        }
        if best_comm != community[u] {
            let old = community[u];
            community[u] = best_comm;
            *sizes.get_mut(&old).unwrap() -= 1;
            *sizes.entry(best_comm).or_insert(0) += 1;
        }
    }

    // (b) Remaining singletons (isolated) → group into one shared community.
    let singletons: Vec<usize> = (0..n).filter(|&u| sizes[&community[u]] == 1).collect();

    if singletons.len() > 1 {
        // Use the label of the first singleton as the shared label.
        let shared_label = community[singletons[0]];
        for &u in &singletons[1..] {
            let old = community[u];
            community[u] = shared_label;
            *sizes.get_mut(&old).unwrap() -= 1;
            *sizes.entry(shared_label).or_insert(0) += 1;
        }
    }
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
            Community {
                id: new_id,
                members,
            }
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
        assert_eq!(
            ids, expected,
            "community IDs must be sequential starting from 0"
        );
    }

    #[test]
    fn louvain_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let communities = detect_communities(&g);
        assert!(communities.is_empty());
    }

    // -----------------------------------------------------------------------
    // BUG-008: Sparse graph singleton merging
    // -----------------------------------------------------------------------

    #[test]
    fn louvain_sparse_graph_merges_singletons() {
        // Simulate a sparse graph like pkg-types: a small connected cluster
        // plus many isolated nodes. Without singleton merging, each isolated
        // node would be its own community (~1:1 ratio).
        let mut g = CodeGraph::new();

        // Connected cluster: a → b → c
        for id in &["a", "b", "c"] {
            g.add_node(module(id));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(2));

        // 20 isolated nodes (no edges) — simulates type-only modules
        for i in 0..20 {
            g.add_node(module(&format!("isolated_{}", i)));
        }

        let communities = detect_communities(&g);

        // Without the fix: 23 communities (1:1 ratio).
        // With the fix: ≤3 communities (connected cluster + 1 unclustered group).
        assert!(
            communities.len() <= 5,
            "sparse graph should produce few communities, got {} for 23 nodes",
            communities.len()
        );
    }

    #[test]
    fn louvain_singleton_with_neighbor_absorbed() {
        // Node d has one edge to the a-b-c cluster but isn't strongly
        // connected. It should be absorbed into the cluster's community
        // rather than staying singleton.
        let mut g = CodeGraph::new();
        for id in &["a", "b", "c", "d"] {
            g.add_node(module(id));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));
        g.add_edge("b", "c", Edge::imports(3));
        g.add_edge("c", "b", Edge::imports(4));
        g.add_edge("d", "a", Edge::imports(5)); // d's only connection

        let communities = detect_communities(&g);

        // d should be merged into the a-b-c community, giving 1 community total.
        assert!(
            communities.len() <= 2,
            "weakly-connected singleton should be absorbed, got {} communities",
            communities.len()
        );

        // Verify d is in the same community as a
        let d_comm = communities
            .iter()
            .find(|c| c.members.contains(&"d".to_string()));
        let a_comm = communities
            .iter()
            .find(|c| c.members.contains(&"a".to_string()));
        assert!(d_comm.is_some() && a_comm.is_some());
        assert_eq!(
            d_comm.unwrap().id,
            a_comm.unwrap().id,
            "d should be in the same community as a"
        );
    }

    #[test]
    fn louvain_is_deterministic_on_symmetric_graph() {
        // Symmetric cycle graph: several partitions can have equal modularity,
        // so tie-breaking must still be deterministic across repeated runs.
        let mut g = CodeGraph::new();
        for id in &["a", "b", "c", "d"] {
            g.add_node(module(id));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));
        g.add_edge("b", "c", Edge::imports(3));
        g.add_edge("c", "b", Edge::imports(4));
        g.add_edge("c", "d", Edge::imports(5));
        g.add_edge("d", "c", Edge::imports(6));
        g.add_edge("d", "a", Edge::imports(7));
        g.add_edge("a", "d", Edge::imports(8));

        let mut fingerprints = std::collections::BTreeSet::new();
        for _ in 0..50 {
            let mut groups: Vec<Vec<String>> = detect_communities(&g)
                .into_iter()
                .map(|community| community.members)
                .collect();
            groups.sort();
            fingerprints.insert(format!("{groups:?}"));
        }

        assert_eq!(
            fingerprints.len(),
            1,
            "community detection should be deterministic, got variants: {:?}",
            fingerprints
        );
    }

    // -----------------------------------------------------------------------
    // label_propagation
    // -----------------------------------------------------------------------

    #[test]
    fn label_propagation_finds_at_least_one_community() {
        let g = two_cluster_graph();
        let communities = label_propagation(&g);
        assert!(
            !communities.is_empty(),
            "should find at least one community"
        );
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
        assert_eq!(
            ids, expected,
            "community IDs must be sequential starting from 0"
        );
    }

    #[test]
    fn label_propagation_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let communities = label_propagation(&g);
        assert!(communities.is_empty());
    }
}
