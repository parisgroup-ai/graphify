use std::collections::{HashMap, HashSet};

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
    /// Structural density: intra-community edges ÷ max possible (undirected).
    /// Range `[0.0, 1.0]`. Singletons are `1.0` by convention. See `cohesion`.
    pub cohesion: f64,
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
    louvain_local_moves(&mut community, &adj, &degree, m, 20);

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

    // Phase 3: split oversized communities (FEAT-036).
    //
    // Louvain sometimes lumps much of the graph into one dominant community,
    // especially when the graph has a hub-and-spoke topology or when global
    // structure overshadows local substructure. Any community larger than
    // `MAX_COMMUNITY_FRACTION` of the total (floor `MIN_SPLIT_SIZE`) gets a
    // second local-moves pass on its induced subgraph. Recursion depth is
    // capped at 1 — the sub-pass does not trigger another split.
    split_oversized(&mut community, &adj, n);

    build_communities(&community, &all_indices, raw)
}

/// Run Louvain-style local-moves on a graph described by `adj` + `degree`.
/// Iterates at most `max_iters` times; stops early when no node changed.
///
/// Pure helper shared by the main pass (Phase 1) and the oversized-split
/// sub-pass (Phase 3 / FEAT-036). Caller provides the current `community`
/// labels and the modularity normalizer `m`.
fn louvain_local_moves(
    community: &mut [usize],
    adj: &[HashMap<usize, f64>],
    degree: &[f64],
    m: f64,
    max_iters: usize,
) {
    let n = community.len();
    if m == 0.0 || n == 0 {
        return;
    }
    for _iter in 0..max_iters {
        let mut improved = false;

        for u in 0..n {
            let current_comm = community[u];

            // Compute k_i_in for each neighbouring community.
            let mut comm_weight: HashMap<usize, f64> = HashMap::new();
            for (&v, &w) in &adj[u] {
                *comm_weight.entry(community[v]).or_insert(0.0) += w;
            }

            // Sigma_tot[c] = sum of degrees of all nodes in community c,
            // excluding u temporarily.
            let mut sigma_tot: HashMap<usize, f64> = HashMap::new();
            for v in 0..n {
                if v != u {
                    *sigma_tot.entry(community[v]).or_insert(0.0) += degree[v];
                }
            }

            let ki = degree[u];

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

/// Run label-propagation on a pre-built neighbour list. Pure helper used by
/// both the top-level `label_propagation` and the oversized-split fallback.
fn label_prop_local(labels: &mut [usize], neighbours: &[Vec<usize>], max_iters: usize) {
    let n = labels.len();
    for _iter in 0..max_iters {
        let mut changed = false;
        for u in 0..n {
            if neighbours[u].is_empty() {
                continue;
            }
            let mut label_count: HashMap<usize, usize> = HashMap::new();
            for &v in &neighbours[u] {
                *label_count.entry(labels[v]).or_insert(0) += 1;
            }
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
}

/// Split oversized communities (Phase 3 / FEAT-036).
///
/// A community is "oversized" when its size exceeds `threshold =
/// max(MIN_SPLIT_SIZE, round(n * MAX_COMMUNITY_FRACTION))`. For each such
/// community, run a local-moves pass on its induced subgraph, starting every
/// member in its own label. If the sub-pass converges on two or more distinct
/// labels, rewrite the global `community` slice so the split is reflected.
/// Otherwise leave the community untouched.
///
/// Determinism: oversized communities are processed in ascending order of
/// their minimum member position, and new sub-labels are allocated from a
/// monotonically increasing counter seeded at `max(community) + 1`.
///
/// Recursion depth: 1. The sub-pass never re-enters `split_oversized`.
fn split_oversized(community: &mut [usize], adj: &[HashMap<usize, f64>], n: usize) {
    const MAX_COMMUNITY_FRACTION: f64 = 0.25;
    const MIN_SPLIT_SIZE: usize = 10;

    if n == 0 {
        return;
    }
    let threshold = ((n as f64) * MAX_COMMUNITY_FRACTION).round() as usize;
    let threshold = threshold.max(MIN_SPLIT_SIZE);

    // Group node positions by community label.
    let mut by_label: HashMap<usize, Vec<usize>> = HashMap::new();
    for (u, &c) in community.iter().enumerate() {
        by_label.entry(c).or_default().push(u);
    }

    // Collect oversized communities, deterministically ordered by earliest
    // member position so sub-label allocation is reproducible.
    let mut oversized: Vec<(usize, Vec<usize>)> = by_label
        .into_iter()
        .filter(|(_, members)| members.len() > threshold)
        .collect();
    if oversized.is_empty() {
        return;
    }
    oversized.sort_by_key(|(_, members)| *members.iter().min().unwrap_or(&usize::MAX));

    let mut next_label = community.iter().copied().max().unwrap_or(0) + 1;

    for (original_label, mut members) in oversized {
        members.sort_unstable();

        // Build induced adjacency restricted to this community's members,
        // re-indexed into 0..members.len().
        let member_to_local: HashMap<usize, usize> = members
            .iter()
            .enumerate()
            .map(|(local, &global)| (global, local))
            .collect();

        let sub_n = members.len();
        let mut sub_adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); sub_n];
        let mut sub_total_weight = 0.0_f64;
        for (&global, &local) in &member_to_local {
            for (&v, &w) in &adj[global] {
                if let Some(&v_local) = member_to_local.get(&v) {
                    sub_adj[local].insert(v_local, w);
                    // Count each undirected edge once.
                    if local < v_local {
                        sub_total_weight += w;
                    }
                }
            }
        }
        let sub_m = sub_total_weight;
        if sub_m == 0.0 {
            continue; // No internal structure to exploit.
        }

        let sub_degree: Vec<f64> = (0..sub_n).map(|u| sub_adj[u].values().sum()).collect();

        // Start every member in its own label (0..sub_n) so the sub-pass
        // is free to rediscover structure from scratch.
        let mut sub_labels: Vec<usize> = (0..sub_n).collect();
        louvain_local_moves(&mut sub_labels, &sub_adj, &sub_degree, sub_m, 20);

        // Fallback: Louvain's greedy local moves can coalesce a sparse
        // community back into a single sub-label. Label-propagation has a
        // different (often finer-grained) failure mode. If Louvain produced
        // no split, try label-propagation on the same induced subgraph.
        let louvain_split_count: usize = sub_labels
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if louvain_split_count <= 1 {
            let mut neighbours: Vec<Vec<usize>> = sub_adj
                .iter()
                .map(|row| {
                    let mut v: Vec<usize> = row.keys().copied().collect();
                    v.sort_unstable();
                    v
                })
                .collect();
            // Dedup already sorted keys (no-op but kept for symmetry with
            // `label_propagation`'s canonical handling).
            for nbrs in neighbours.iter_mut() {
                nbrs.dedup();
            }
            sub_labels = (0..sub_n).collect();
            label_prop_local(&mut sub_labels, &neighbours, 50);
        }

        // Count unique resulting labels.
        let mut unique_labels: std::collections::BTreeSet<usize> =
            sub_labels.iter().copied().collect();
        if unique_labels.len() <= 1 {
            continue; // No meaningful split.
        }

        // Deterministically map sub-labels to global labels.
        // - The sub-label that contains the smallest member keeps the
        //   community's original global label.
        // - Remaining sub-labels each get a fresh global label.
        let first_sub = sub_labels[0]; // members[0] is the smallest global index
        let mut mapping: HashMap<usize, usize> = HashMap::new();
        mapping.insert(first_sub, original_label);
        unique_labels.remove(&first_sub);
        for sub in unique_labels {
            mapping.insert(sub, next_label);
            next_label += 1;
        }

        // Write back into the global community slice.
        for (local, &global) in members.iter().enumerate() {
            community[global] = mapping[&sub_labels[local]];
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
            let cohesion = cohesion(&members, raw);
            Community {
                id: new_id,
                members,
                cohesion,
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
// Cohesion (FEAT-035)
// ---------------------------------------------------------------------------

/// Structural density of a community: ratio of actual intra-community edges
/// to the maximum possible (complete graph on `n` nodes, treated as undirected).
///
/// Range: `[0.0, 1.0]`. A community of 0 or 1 nodes returns `1.0` — there are
/// no missing connections possible, so calling it "perfectly cohesive" is the
/// natural singleton convention (matches `graphify/cluster.py::cohesion_score`
/// in the unrelated `safishamsi/graphify` reference implementation).
fn cohesion_from_counts(n: usize, intra_edges: usize) -> f64 {
    if n <= 1 {
        return 1.0;
    }
    let possible = n * (n - 1) / 2;
    if possible == 0 {
        return 0.0;
    }
    intra_edges as f64 / possible as f64
}

/// Counts distinct unordered intra-community edges by walking `raw` once, then
/// feeds the result into `cohesion_from_counts`.
///
/// Edges `a→b` and `b→a` between the same unordered pair collapse to a single
/// undirected edge. Self-loops and edges touching non-member nodes are ignored.
fn cohesion(
    members: &[String],
    raw: &petgraph::graph::DiGraph<crate::types::Node, crate::types::Edge>,
) -> f64 {
    let n = members.len();
    if n <= 1 {
        return 1.0;
    }
    let member_set: HashSet<&str> = members.iter().map(String::as_str).collect();
    let mut pairs: HashSet<(&str, &str)> = HashSet::new();
    for edge_ref in raw.edge_references() {
        let src = raw[edge_ref.source()].id.as_str();
        let tgt = raw[edge_ref.target()].id.as_str();
        if src == tgt {
            continue;
        }
        if !(member_set.contains(src) && member_set.contains(tgt)) {
            continue;
        }
        let key = if src < tgt { (src, tgt) } else { (tgt, src) };
        pairs.insert(key);
    }
    cohesion_from_counts(n, pairs.len())
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
    label_prop_local(&mut labels, &neighbours, 50);

    // Phase 3: split oversized communities (FEAT-036). Convert the unweighted
    // neighbour lists into the weighted adjacency shape `split_oversized`
    // expects (all weights = 1.0 since label-propagation ignores weights).
    let adj: Vec<HashMap<usize, f64>> = neighbours
        .iter()
        .map(|nbrs| nbrs.iter().map(|&v| (v, 1.0)).collect())
        .collect();
    split_oversized(&mut labels, &adj, n);

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

    // -----------------------------------------------------------------------
    // FEAT-035: cohesion_from_counts (pure formula)
    // -----------------------------------------------------------------------

    #[test]
    fn cohesion_singleton_is_one() {
        assert_eq!(cohesion_from_counts(1, 0), 1.0);
    }

    #[test]
    fn cohesion_empty_is_one() {
        assert_eq!(cohesion_from_counts(0, 0), 1.0);
    }

    #[test]
    fn cohesion_pair_with_no_edge_is_zero() {
        assert_eq!(cohesion_from_counts(2, 0), 0.0);
    }

    #[test]
    fn cohesion_pair_with_edge_is_one() {
        assert_eq!(cohesion_from_counts(2, 1), 1.0);
    }

    #[test]
    fn cohesion_triangle_is_one() {
        assert_eq!(cohesion_from_counts(3, 3), 1.0);
    }

    #[test]
    fn cohesion_three_nodes_one_edge_is_one_third() {
        let expected = 1.0_f64 / 3.0_f64;
        assert!(
            (cohesion_from_counts(3, 1) - expected).abs() < 1e-12,
            "expected ~{expected}, got {}",
            cohesion_from_counts(3, 1)
        );
    }

    // -----------------------------------------------------------------------
    // FEAT-035: cohesion (walks the raw graph)
    // -----------------------------------------------------------------------

    #[test]
    fn cohesion_walker_singleton_is_one() {
        let mut g = CodeGraph::new();
        g.add_node(module("solo"));
        let members = vec!["solo".to_string()];
        assert_eq!(cohesion(&members, g.raw_graph()), 1.0);
    }

    #[test]
    fn cohesion_walker_pair_without_edge_is_zero() {
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        let members = vec!["a".to_string(), "b".to_string()];
        assert_eq!(cohesion(&members, g.raw_graph()), 0.0);
    }

    #[test]
    fn cohesion_walker_treats_directed_pair_as_one_undirected_edge() {
        // A→B and B→A counted as a single unordered {A,B} pair.
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));
        let members = vec!["a".to_string(), "b".to_string()];
        assert_eq!(cohesion(&members, g.raw_graph()), 1.0);
    }

    #[test]
    fn cohesion_walker_ignores_edges_to_non_members() {
        // 3 nodes, edge a→b (intra) and edge b→c (c not a member).
        // Members = [a, b]. Expected: 1 intra edge / 1 possible = 1.0.
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("c"));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(2));
        let members = vec!["a".to_string(), "b".to_string()];
        assert_eq!(cohesion(&members, g.raw_graph()), 1.0);
    }

    #[test]
    fn cohesion_walker_three_nodes_one_edge_is_one_third() {
        let mut g = CodeGraph::new();
        g.add_node(module("a"));
        g.add_node(module("b"));
        g.add_node(module("c"));
        g.add_edge("a", "b", Edge::imports(1));
        let members = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let got = cohesion(&members, g.raw_graph());
        let expected = 1.0_f64 / 3.0_f64;
        assert!((got - expected).abs() < 1e-12, "got {got}");
    }

    // -----------------------------------------------------------------------
    // FEAT-035: integration — Community.cohesion is populated
    // -----------------------------------------------------------------------

    #[test]
    fn detect_communities_emits_cohesion_per_community() {
        let g = two_cluster_graph();
        let communities = detect_communities(&g);
        for c in &communities {
            assert!(
                (0.0..=1.0).contains(&c.cohesion),
                "community {} cohesion {} out of range",
                c.id,
                c.cohesion
            );
        }
    }

    #[test]
    fn label_propagation_emits_cohesion_per_community() {
        let g = two_cluster_graph();
        let communities = label_propagation(&g);
        for c in &communities {
            assert!(
                (0.0..=1.0).contains(&c.cohesion),
                "community {} cohesion {} out of range",
                c.id,
                c.cohesion
            );
        }
    }

    #[test]
    fn detect_communities_singleton_has_cohesion_one() {
        let mut g = CodeGraph::new();
        g.add_node(module("solo"));
        let communities = detect_communities(&g);
        assert_eq!(communities.len(), 1);
        assert_eq!(communities[0].cohesion, 1.0);
    }

    // -----------------------------------------------------------------------
    // FEAT-036: split_oversized (Phase 3)
    // -----------------------------------------------------------------------
    //
    // The helpers operate on `community: &mut [usize]` and `adj: &[...]`
    // directly, so we can unit-test the split logic with hand-crafted input
    // without relying on Louvain's Phase-1 decisions.

    /// Build an adjacency map where nodes are grouped into two dense cliques.
    /// Nodes `[0, first_half)` form one clique; `[first_half, n)` form another.
    /// `cross_edges` adds sparse bridges between the two cliques.
    fn bimodal_adj(
        n: usize,
        first_half: usize,
        cross_edges: &[(usize, usize)],
    ) -> Vec<HashMap<usize, f64>> {
        let mut adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); n];
        // Clique A: 0..first_half
        for u in 0..first_half {
            for v in (u + 1)..first_half {
                adj[u].insert(v, 1.0);
                adj[v].insert(u, 1.0);
            }
        }
        // Clique B: first_half..n
        for u in first_half..n {
            for v in (u + 1)..n {
                adj[u].insert(v, 1.0);
                adj[v].insert(u, 1.0);
            }
        }
        // Sparse cross edges
        for &(a, b) in cross_edges {
            adj[a].insert(b, 1.0);
            adj[b].insert(a, 1.0);
        }
        adj
    }

    #[test]
    fn split_oversized_no_op_below_threshold() {
        // n=20, threshold = max(10, 5) = 10.
        // Two communities of size 5 each — both below threshold → untouched.
        let adj = bimodal_adj(10, 5, &[]);
        let mut community: Vec<usize> = (0..5).map(|_| 0).chain((0..5).map(|_| 1)).collect();
        let before = community.clone();
        split_oversized(&mut community, &adj, 10);
        assert_eq!(community, before);
    }

    #[test]
    fn split_oversized_splits_bimodal_hub_community() {
        // n=20, threshold = max(10, 5) = 10.
        // All 20 nodes initially share label 0 (simulates one big community
        // that Louvain failed to subdivide). Internal structure: two cliques
        // of 10 each with a single bridge edge. Expect: the split produces
        // ≥2 distinct labels and the two cliques land on opposite sides
        // (majority vote — Louvain is greedy, not guaranteed optimal).
        let adj = bimodal_adj(20, 10, &[(0, 10)]);
        let mut community: Vec<usize> = vec![0; 20];
        split_oversized(&mut community, &adj, 20);

        let unique: std::collections::HashSet<usize> = community.iter().copied().collect();
        assert!(
            unique.len() >= 2,
            "expected split into ≥2 sub-communities, got {:?}",
            community
        );

        // Clique A label = mode of community[0..10]; clique B label =
        // mode of community[10..20]. Majority is enough: Louvain may leave
        // a boundary node on the "wrong" side without invalidating the split.
        let mode = |slice: &[usize]| -> usize {
            let mut counts: HashMap<usize, usize> = HashMap::new();
            for &c in slice {
                *counts.entry(c).or_insert(0) += 1;
            }
            *counts
                .iter()
                .max_by_key(|&(_, &count)| count)
                .map(|(label, _)| label)
                .unwrap()
        };
        let clique_a = mode(&community[0..10]);
        let clique_b = mode(&community[10..20]);
        assert_ne!(
            clique_a, clique_b,
            "cliques should land on distinct labels, got {:?}",
            community
        );
        // At least 80% of each clique sits with its own clique's mode.
        let a_majority = community[0..10].iter().filter(|&&c| c == clique_a).count();
        let b_majority = community[10..20].iter().filter(|&&c| c == clique_b).count();
        assert!(
            a_majority >= 8,
            "clique A majority too weak: {a_majority}/10"
        );
        assert!(
            b_majority >= 8,
            "clique B majority too weak: {b_majority}/10"
        );
    }

    #[test]
    fn split_oversized_leaves_edgeless_community_alone() {
        // n=20, threshold = 10.
        // All 20 nodes share label 0, but no edges at all.
        // Sub-pass finds no substructure → community untouched.
        let adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); 20];
        let mut community: Vec<usize> = vec![0; 20];
        let before = community.clone();
        split_oversized(&mut community, &adj, 20);
        assert_eq!(community, before);
    }

    #[test]
    fn split_oversized_respects_size_floor() {
        // n=15, threshold = max(10, round(3.75)) = 10.
        // A 6-member community (40% of total, but below 10 floor) → untouched.
        let adj = bimodal_adj(15, 6, &[]);
        let mut community: Vec<usize> = (0..6).map(|_| 0).chain((0..9).map(|_| 1)).collect();
        // Clique B (label 1, size 9) is also below threshold — same result.
        let before = community.clone();
        split_oversized(&mut community, &adj, 15);
        assert_eq!(community, before);
    }

    #[test]
    fn split_oversized_is_deterministic() {
        // Running twice with the same input should produce identical output.
        let adj = bimodal_adj(20, 10, &[(0, 10)]);

        let mut a: Vec<usize> = vec![0; 20];
        split_oversized(&mut a, &adj, 20);

        let mut b: Vec<usize> = vec![0; 20];
        split_oversized(&mut b, &adj, 20);

        assert_eq!(a, b);
    }

    #[test]
    fn split_oversized_uses_fresh_labels_no_collision() {
        // Existing labels occupy 0..=3 across other communities.
        // When splitting the oversized community (label 0), new sub-labels
        // must not collide with the reserved labels 1, 2, 3.
        let n = 23;
        // Indices 0..20 → oversized community, label 0.
        // Indices 20, 21, 22 → three small communities, labels 1, 2, 3.
        let mut adj = bimodal_adj(20, 10, &[(0, 10)]);
        adj.resize(n, HashMap::new());
        let mut community: Vec<usize> = (0..20).map(|_| 0).chain([1, 2, 3]).collect();

        split_oversized(&mut community, &adj, n);

        // Labels 1, 2, 3 for outside nodes stay put.
        assert_eq!(community[20], 1);
        assert_eq!(community[21], 2);
        assert_eq!(community[22], 3);

        // Split produced at least one new label that is not 1, 2, or 3.
        let split_labels: std::collections::HashSet<usize> =
            community[..20].iter().copied().collect();
        for &lbl in &split_labels {
            if lbl != 0 {
                assert!(
                    !matches!(lbl, 1 | 2 | 3),
                    "new sub-label {lbl} collided with reserved outside labels"
                );
            }
        }
    }

    #[test]
    fn detect_communities_integrates_phase3_split() {
        // Build a graph that Louvain's main pass tends to collapse into one
        // big community but whose internal structure is two clique-like
        // halves connected by a single bridge. Phase-3 should find the
        // bimodal structure on the induced subgraph and split.
        let mut g = CodeGraph::new();
        let total = 20;
        for i in 0..total {
            g.add_node(module(&format!("n{i}")));
        }
        // Clique A: n0..n9 fully connected both directions.
        for u in 0..10 {
            for v in (u + 1)..10 {
                g.add_edge(&format!("n{u}"), &format!("n{v}"), Edge::imports(1));
                g.add_edge(&format!("n{v}"), &format!("n{u}"), Edge::imports(1));
            }
        }
        // Clique B: n10..n19 fully connected both directions.
        for u in 10..20 {
            for v in (u + 1)..20 {
                g.add_edge(&format!("n{u}"), &format!("n{v}"), Edge::imports(1));
                g.add_edge(&format!("n{v}"), &format!("n{u}"), Edge::imports(1));
            }
        }
        // Single bridge.
        g.add_edge("n0", "n10", Edge::imports(1));

        let communities = detect_communities(&g);
        // With Phase-3 wired in, we expect the two cliques to land in
        // distinct communities. Without Phase-3, Louvain main pass still
        // handles this cleanly — but the assertion guards the wiring so a
        // future refactor that drops the call gets caught.
        assert!(
            communities.len() >= 2,
            "expected ≥2 communities, got {:?}",
            communities
        );
        // Find each clique's dominant community.
        let find_comm_for = |member: &str| -> usize {
            communities
                .iter()
                .find(|c| c.members.contains(&member.to_string()))
                .expect("every node must be in some community")
                .id
        };
        let comm_a = find_comm_for("n5");
        let comm_b = find_comm_for("n15");
        assert_ne!(
            comm_a, comm_b,
            "cliques should land in distinct communities"
        );
    }

    #[test]
    fn detect_communities_isolated_singletons_merge_to_zero_cohesion() {
        // Connected cluster a↔b keeps m > 0 so `merge_singletons` runs.
        // The two isolated nodes x and y get grouped into a single
        // "unclustered" community with 0 intra-edges → cohesion 0.0.
        let mut g = CodeGraph::new();
        for id in &["a", "b", "x", "y"] {
            g.add_node(module(id));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(2));

        let communities = detect_communities(&g);
        let isolate_community = communities
            .iter()
            .find(|c| c.members.contains(&"x".to_string()))
            .expect("x should be in some community");
        assert!(
            isolate_community.members.contains(&"y".to_string()),
            "x and y should share a community, got {:?}",
            isolate_community
        );
        assert_eq!(isolate_community.members.len(), 2);
        assert_eq!(isolate_community.cohesion, 0.0);
    }
}
