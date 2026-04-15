use std::collections::HashMap;

use petgraph::Direction;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::cycles::find_sccs;
use crate::graph::CodeGraph;

// ---------------------------------------------------------------------------
// ScoringWeights
// ---------------------------------------------------------------------------

/// Weights used to compute the composite hotspot score for each node.
///
/// All weights should sum to 1.0 for a well-calibrated score, but this is
/// not enforced — callers may use any positive values.
pub struct ScoringWeights {
    pub betweenness: f64, // default 0.4
    pub pagerank: f64,    // default 0.2
    pub in_degree: f64,   // default 0.2
    pub in_cycle: f64,    // default 0.2
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            betweenness: 0.4,
            pagerank: 0.2,
            in_degree: 0.2,
            in_cycle: 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// HotspotType + HotspotThresholds
// ---------------------------------------------------------------------------

/// Classification of *why* a node is a hotspot.
///
/// The composite score ranks structural importance, but two nodes with similar
/// scores can require very different refactors. This enum disambiguates them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotspotType {
    /// High in-degree — many modules import this. Fix: split or invert deps.
    Hub,
    /// High betweenness-per-incoming-edge — chokepoint between layers. Fix:
    /// inject the cross-layer dependency.
    Bridge,
    /// Both pressures present, or neither dominates — needs human judgment.
    #[default]
    Mixed,
}

/// Thresholds controlling [`classify`].
///
/// Defaults are calibrated against medium-sized polyglot repos (~1k modules).
/// Tune via `--hub-threshold` / `--bridge-ratio` for very small or very large
/// graphs.
#[derive(Debug, Clone, Copy)]
pub struct HotspotThresholds {
    /// Minimum in-degree to qualify as a hub. Default: 50.
    pub hub_threshold: usize,
    /// Minimum `betweenness / max(in_degree, 1)` to qualify as a bridge.
    /// Default: 3000.
    pub bridge_ratio: f64,
}

impl Default for HotspotThresholds {
    fn default() -> Self {
        Self {
            hub_threshold: 50,
            bridge_ratio: 3000.0,
        }
    }
}

/// Classifies a node as `hub`, `bridge`, or `mixed` from its raw metrics.
///
/// - **Hub** — in-degree exceeds `hub_threshold` *and* the bridge ratio does not.
/// - **Bridge** — bridge ratio exceeds `bridge_ratio` *and* in-degree does not
///   exceed `hub_threshold`.
/// - **Mixed** — neither threshold fires, or both fire (severe hub *and*
///   bridge), so the refactor strategy needs human judgment.
pub fn classify(
    in_degree: usize,
    betweenness: f64,
    thresholds: &HotspotThresholds,
) -> HotspotType {
    let is_hub = in_degree > thresholds.hub_threshold;
    let ratio = betweenness / in_degree.max(1) as f64;
    let is_bridge = ratio > thresholds.bridge_ratio;

    match (is_hub, is_bridge) {
        (true, false) => HotspotType::Hub,
        (false, true) => HotspotType::Bridge,
        _ => HotspotType::Mixed,
    }
}

// ---------------------------------------------------------------------------
// NodeMetrics
// ---------------------------------------------------------------------------

/// Computed metrics for a single graph node.
#[derive(Debug, Clone, Default)]
pub struct NodeMetrics {
    pub id: String,
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: usize,
    pub out_degree: usize,
    pub in_cycle: bool,
    /// Weighted composite score combining all normalized metrics.
    pub score: f64,
    /// Community identifier — filled later by community detection; defaults to 0.
    pub community_id: usize,
    /// Hotspot classification — see [`HotspotType`].
    pub hotspot_type: HotspotType,
}

// ---------------------------------------------------------------------------
// betweenness_centrality
// ---------------------------------------------------------------------------

/// Computes betweenness centrality for all nodes using Brandes' BFS algorithm.
///
/// For large graphs (n > 200) a random sample of `k = min(200, n)` source
/// nodes is used to keep runtime tractable; scores are scaled back up to
/// approximate full-graph values.
///
/// Returns a map of node ID → raw (unnormalized) betweenness score.
pub fn betweenness_centrality(graph: &CodeGraph) -> HashMap<String, f64> {
    let raw = graph.raw_graph();
    let n = raw.node_count();
    if n == 0 {
        return HashMap::new();
    }

    // Collect all node indices.
    let all_indices: Vec<_> = raw.node_indices().collect();

    // Initialize centrality scores to 0 for every node.
    let mut centrality: HashMap<String, f64> = all_indices
        .iter()
        .map(|&idx| (raw[idx].id.clone(), 0.0))
        .collect();

    // Choose source nodes — sample k when n > 200.
    let k = n.min(200);
    let sources: Vec<_> = if k < n {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut shuffled = all_indices.clone();
        shuffled.shuffle(&mut rng);
        shuffled[..k].to_vec()
    } else {
        all_indices.clone()
    };

    // Brandes' algorithm — one BFS per source.
    for &s in &sources {
        // Stack of nodes in order of non-increasing distance from s.
        let mut stack: Vec<petgraph::graph::NodeIndex> = Vec::new();
        // Predecessors on shortest paths from s.
        let mut pred: HashMap<petgraph::graph::NodeIndex, Vec<petgraph::graph::NodeIndex>> =
            all_indices.iter().map(|&v| (v, Vec::new())).collect();
        // Number of shortest paths from s to v.
        let mut sigma: HashMap<petgraph::graph::NodeIndex, f64> =
            all_indices.iter().map(|&v| (v, 0.0)).collect();
        sigma.insert(s, 1.0);
        // Distance from s.
        let mut dist: HashMap<petgraph::graph::NodeIndex, i64> =
            all_indices.iter().map(|&v| (v, -1)).collect();
        dist.insert(s, 0);

        // BFS queue.
        let mut queue: std::collections::VecDeque<petgraph::graph::NodeIndex> =
            std::collections::VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let v_dist = dist[&v];
            let v_sigma = sigma[&v];

            for w in raw.neighbors_directed(v, Direction::Outgoing) {
                // First time we reach w?
                if dist[&w] < 0 {
                    queue.push_back(w);
                    *dist.get_mut(&w).unwrap() = v_dist + 1;
                }
                // Is v on a shortest path to w?
                if dist[&w] == v_dist + 1 {
                    *sigma.get_mut(&w).unwrap() += v_sigma;
                    pred.get_mut(&w).unwrap().push(v);
                }
            }
        }

        // Accumulation phase.
        let mut delta: HashMap<petgraph::graph::NodeIndex, f64> =
            all_indices.iter().map(|&v| (v, 0.0)).collect();

        // Process in reverse BFS order.
        for &w in stack.iter().rev() {
            for &v in &pred[&w] {
                let coeff = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                *delta.get_mut(&v).unwrap() += coeff;
            }
            if w != s {
                let node_id = raw[w].id.clone();
                *centrality.get_mut(&node_id).unwrap() += delta[&w];
            }
        }
    }

    // If we sampled, scale up to approximate full-graph betweenness.
    if k < n {
        let scale = n as f64 / k as f64;
        for val in centrality.values_mut() {
            *val *= scale;
        }
    }

    centrality
}

// ---------------------------------------------------------------------------
// pagerank
// ---------------------------------------------------------------------------

/// Computes PageRank for all nodes.
///
/// Uses damping factor d = 0.85, up to 100 iterations, convergence
/// threshold ε = 1e-6.
pub fn pagerank(graph: &CodeGraph) -> HashMap<String, f64> {
    let raw = graph.raw_graph();
    let n = raw.node_count();
    if n == 0 {
        return HashMap::new();
    }

    let all_indices: Vec<_> = raw.node_indices().collect();
    let damping = 0.85_f64;
    let initial = 1.0 / n as f64;
    let epsilon = 1e-6_f64;

    // Initialize ranks.
    let mut rank: HashMap<petgraph::graph::NodeIndex, f64> =
        all_indices.iter().map(|&idx| (idx, initial)).collect();

    for _ in 0..100 {
        let mut new_rank: HashMap<petgraph::graph::NodeIndex, f64> = all_indices
            .iter()
            .map(|&idx| (idx, (1.0 - damping) / n as f64))
            .collect();

        for &v in &all_indices {
            // Distribute v's rank to all its out-neighbors.
            let out_edges: Vec<_> = raw.neighbors_directed(v, Direction::Outgoing).collect();
            let out_deg = out_edges.len();
            if out_deg > 0 {
                let contribution = damping * rank[&v] / out_deg as f64;
                for w in out_edges {
                    *new_rank.get_mut(&w).unwrap() += contribution;
                }
            } else {
                // Dangling node: distribute rank equally to all nodes.
                let contribution = damping * rank[&v] / n as f64;
                for &w in &all_indices {
                    *new_rank.get_mut(&w).unwrap() += contribution;
                }
            }
        }

        // Check convergence.
        let delta: f64 = all_indices
            .iter()
            .map(|&idx| (new_rank[&idx] - rank[&idx]).abs())
            .sum();

        rank = new_rank;

        if delta < epsilon {
            break;
        }
    }

    // Convert to ID-keyed map.
    rank.into_iter()
        .map(|(idx, val)| (raw[idx].id.clone(), val))
        .collect()
}

// ---------------------------------------------------------------------------
// normalize
// ---------------------------------------------------------------------------

/// Min-max normalizes a map of values to [0, 1].
///
/// If all values are equal (range = 0), all normalized values are set to 0.0.
pub fn normalize(values: &HashMap<String, f64>) -> HashMap<String, f64> {
    if values.is_empty() {
        return HashMap::new();
    }

    let min = values.values().cloned().fold(f64::INFINITY, f64::min);
    let max = values.values().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range == 0.0 {
        return values.keys().map(|k| (k.clone(), 0.0)).collect();
    }

    values
        .iter()
        .map(|(k, &v)| (k.clone(), (v - min) / range))
        .collect()
}

// ---------------------------------------------------------------------------
// compute_metrics
// ---------------------------------------------------------------------------

/// Computes all metrics for every node in `graph` and returns them as a
/// `Vec<NodeMetrics>` sorted by score descending (highest first).
///
/// Uses default [`HotspotThresholds`]. Call [`compute_metrics_with_thresholds`]
/// to override.
pub fn compute_metrics(graph: &CodeGraph, weights: &ScoringWeights) -> Vec<NodeMetrics> {
    compute_metrics_with_thresholds(graph, weights, &HotspotThresholds::default())
}

/// Like [`compute_metrics`] but lets the caller override the hotspot
/// classification thresholds.
///
/// Steps:
/// 1. Compute raw betweenness and PageRank.
/// 2. Build raw in/out degree and in_cycle boolean maps.
/// 3. Normalize betweenness, PageRank, and in_degree to [0, 1].
/// 4. Score = weighted sum: `w.betweenness * bt + w.pagerank * pr + w.in_degree * id + w.in_cycle * ic`.
/// 5. Classify each node via [`classify`] over its raw `in_degree` and `betweenness`.
pub fn compute_metrics_with_thresholds(
    graph: &CodeGraph,
    weights: &ScoringWeights,
    thresholds: &HotspotThresholds,
) -> Vec<NodeMetrics> {
    let ids: Vec<String> = graph.node_ids().iter().map(|s| s.to_string()).collect();
    if ids.is_empty() {
        return Vec::new();
    }

    // Raw metrics.
    let raw_bt = betweenness_centrality(graph);
    let raw_pr = pagerank(graph);

    // Precompute cycle membership ONCE (O(V+E)), not per-node.
    let sccs = find_sccs(graph);
    let cycle_members: std::collections::HashSet<&str> = sccs
        .iter()
        .flat_map(|scc| scc.node_ids.iter().map(|s| s.as_str()))
        .collect();

    // Build raw in_degree map (as f64 for normalization).
    let raw_id_f64: HashMap<String, f64> = ids
        .iter()
        .map(|id| (id.clone(), graph.in_degree(id) as f64))
        .collect();

    // Normalize.
    let norm_bt = normalize(&raw_bt);
    let norm_pr = normalize(&raw_pr);
    let norm_id = normalize(&raw_id_f64);

    let mut metrics: Vec<NodeMetrics> = ids
        .iter()
        .map(|id| {
            let bt = norm_bt.get(id).copied().unwrap_or(0.0);
            let pr = norm_pr.get(id).copied().unwrap_or(0.0);
            let id_norm = norm_id.get(id).copied().unwrap_or(0.0);
            let in_cycle = cycle_members.contains(id.as_str());
            let ic = if in_cycle { 1.0 } else { 0.0 };

            let score = weights.betweenness * bt
                + weights.pagerank * pr
                + weights.in_degree * id_norm
                + weights.in_cycle * ic;

            let raw_in_deg = graph.in_degree(id);
            let raw_bt_val = raw_bt.get(id).copied().unwrap_or(0.0);
            let hotspot_type = classify(raw_in_deg, raw_bt_val, thresholds);

            NodeMetrics {
                id: id.clone(),
                betweenness: raw_bt_val,
                pagerank: raw_pr.get(id).copied().unwrap_or(0.0),
                in_degree: raw_in_deg,
                out_degree: graph.out_degree(id),
                in_cycle,
                score,
                community_id: 0,
                hotspot_type,
            }
        })
        .collect();

    // Sort by score descending.
    metrics.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    metrics
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

    /// Star graph: b, c, d, e all point TO hub "a".
    fn star_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        for id in &["a", "b", "c", "d", "e"] {
            g.add_node(module(id));
        }
        g.add_edge("b", "a", Edge::imports(1));
        g.add_edge("c", "a", Edge::imports(2));
        g.add_edge("d", "a", Edge::imports(3));
        g.add_edge("e", "a", Edge::imports(4));
        g
    }

    /// Chain graph: a → b → c → d
    fn chain_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        for id in &["a", "b", "c", "d"] {
            g.add_node(module(id));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(2));
        g.add_edge("c", "d", Edge::imports(3));
        g
    }

    // -----------------------------------------------------------------------
    // betweenness_centrality
    // -----------------------------------------------------------------------

    #[test]
    fn betweenness_star_graph_computes_for_all_nodes() {
        let g = star_graph();
        let bt = betweenness_centrality(&g);
        assert_eq!(bt.len(), 5, "should have scores for all 5 nodes");
        // All values must be finite non-negative.
        for (_, &v) in bt.iter().collect::<Vec<_>>() {
            assert!(
                v >= 0.0 && v.is_finite(),
                "betweenness must be non-negative finite"
            );
        }
    }

    #[test]
    fn betweenness_chain_middle_nodes_higher() {
        let g = chain_graph();
        let bt = betweenness_centrality(&g);
        // b is at position 1 in chain a→b→c→d; it lies on paths a→c, a→d.
        // b should have betweenness >= a and >= d.
        let b = bt["b"];
        let a = bt["a"];
        let d = bt["d"];
        assert!(b >= a, "b betweenness ({b}) should be >= a ({a})");
        assert!(b >= d, "b betweenness ({b}) should be >= d ({d})");
    }

    #[test]
    fn betweenness_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let bt = betweenness_centrality(&g);
        assert!(bt.is_empty());
    }

    // -----------------------------------------------------------------------
    // pagerank
    // -----------------------------------------------------------------------

    #[test]
    fn pagerank_star_hub_has_highest_rank() {
        let g = star_graph();
        let pr = pagerank(&g);
        let hub_rank = pr["a"];
        for (id, &v) in &pr {
            if id != "a" {
                assert!(
                    hub_rank >= v,
                    "hub 'a' rank ({hub_rank}) should be >= '{id}' rank ({v})"
                );
            }
        }
    }

    #[test]
    fn pagerank_sum_approx_one() {
        let g = star_graph();
        let pr = pagerank(&g);
        let total: f64 = pr.values().sum();
        assert!(
            (total - 1.0).abs() < 1e-4,
            "PageRank sum should be ≈ 1.0, got {total}"
        );
    }

    #[test]
    fn pagerank_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let pr = pagerank(&g);
        assert!(pr.is_empty());
    }

    // -----------------------------------------------------------------------
    // normalize
    // -----------------------------------------------------------------------

    #[test]
    fn normalize_three_values() {
        let mut values = HashMap::new();
        values.insert("x".to_string(), 10.0);
        values.insert("y".to_string(), 20.0);
        values.insert("z".to_string(), 30.0);

        let normed = normalize(&values);
        assert!(
            (normed["x"] - 0.0).abs() < 1e-9,
            "x should normalize to 0.0"
        );
        assert!(
            (normed["y"] - 0.5).abs() < 1e-9,
            "y should normalize to 0.5"
        );
        assert!(
            (normed["z"] - 1.0).abs() < 1e-9,
            "z should normalize to 1.0"
        );
    }

    #[test]
    fn normalize_all_equal_returns_zeros() {
        let values: HashMap<String, f64> = [("a", 5.0), ("b", 5.0), ("c", 5.0)]
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect();
        let normed = normalize(&values);
        for v in normed.values() {
            assert_eq!(*v, 0.0, "equal values should all normalize to 0.0");
        }
    }

    #[test]
    fn normalize_empty_returns_empty() {
        let normed = normalize(&HashMap::new());
        assert!(normed.is_empty());
    }

    // -----------------------------------------------------------------------
    // compute_metrics
    // -----------------------------------------------------------------------

    #[test]
    fn compute_metrics_hub_scores_highest_with_default_weights() {
        let g = star_graph();
        let metrics = compute_metrics(&g, &ScoringWeights::default());
        // Metrics are sorted descending by score — hub "a" should be first.
        assert_eq!(
            metrics[0].id, "a",
            "hub 'a' should have the highest score; got '{}'",
            metrics[0].id
        );
    }

    #[test]
    fn compute_metrics_custom_weights_in_degree_only() {
        // With in_degree weight = 1 and all others = 0, the node with the most
        // incoming edges wins.
        let g = star_graph();
        let weights = ScoringWeights {
            betweenness: 0.0,
            pagerank: 0.0,
            in_degree: 1.0,
            in_cycle: 0.0,
        };
        let metrics = compute_metrics(&g, &weights);
        // "a" has 4 incoming edges — must be highest.
        assert_eq!(
            metrics[0].id, "a",
            "with in_degree-only weights, hub 'a' must score highest"
        );
        // Leaves b,c,d,e have 0 incoming edges — their scores should be 0.
        for m in &metrics[1..] {
            assert_eq!(m.score, 0.0, "leaf '{}' should have score 0.0", m.id);
        }
    }

    #[test]
    fn compute_metrics_returns_all_nodes() {
        let g = star_graph();
        let metrics = compute_metrics(&g, &ScoringWeights::default());
        assert_eq!(metrics.len(), 5);
    }

    #[test]
    fn compute_metrics_empty_graph_returns_empty() {
        let g = CodeGraph::new();
        let metrics = compute_metrics(&g, &ScoringWeights::default());
        assert!(metrics.is_empty());
    }

    #[test]
    fn compute_metrics_community_id_defaults_to_zero() {
        let g = star_graph();
        let metrics = compute_metrics(&g, &ScoringWeights::default());
        for m in &metrics {
            assert_eq!(m.community_id, 0, "community_id should default to 0");
        }
    }

    // -----------------------------------------------------------------------
    // classify
    // -----------------------------------------------------------------------

    #[test]
    fn classify_hub_when_in_degree_dominates() {
        // 60 incoming edges, modest betweenness → hub
        let t = HotspotThresholds::default();
        assert_eq!(classify(60, 100.0, &t), HotspotType::Hub);
    }

    #[test]
    fn classify_bridge_when_betweenness_per_incoming_dominates() {
        // 2 incoming, huge betweenness → ratio 5000 > 3000 → bridge
        let t = HotspotThresholds::default();
        assert_eq!(classify(2, 10_000.0, &t), HotspotType::Bridge);
    }

    #[test]
    fn classify_mixed_when_both_thresholds_fire() {
        // High in-degree AND high ratio → human judgment needed
        let t = HotspotThresholds::default();
        assert_eq!(classify(200, 1_000_000.0, &t), HotspotType::Mixed);
    }

    #[test]
    fn classify_mixed_when_neither_threshold_fires() {
        let t = HotspotThresholds::default();
        assert_eq!(classify(5, 100.0, &t), HotspotType::Mixed);
    }

    #[test]
    fn classify_zero_in_degree_uses_max_one_in_denominator() {
        // 0 incoming, betweenness 4000 → ratio 4000 > 3000 → bridge (no /0)
        let t = HotspotThresholds::default();
        assert_eq!(classify(0, 4_000.0, &t), HotspotType::Bridge);
    }

    #[test]
    fn classify_respects_custom_thresholds() {
        let t = HotspotThresholds {
            hub_threshold: 10,
            bridge_ratio: 100.0,
        };
        assert_eq!(classify(11, 50.0, &t), HotspotType::Hub);
        assert_eq!(classify(2, 300.0, &t), HotspotType::Bridge);
    }

    #[test]
    fn compute_metrics_populates_hotspot_type() {
        let g = star_graph();
        let metrics = compute_metrics(&g, &ScoringWeights::default());
        // Every node must have a classification — default thresholds make small
        // graphs all "mixed", but the field must exist.
        for m in &metrics {
            assert!(matches!(
                m.hotspot_type,
                HotspotType::Hub | HotspotType::Bridge | HotspotType::Mixed
            ));
        }
    }

    #[test]
    fn compute_metrics_with_thresholds_classifies_hub() {
        // Force "a" (in_degree=4) above a tiny hub threshold.
        let g = star_graph();
        let thresholds = HotspotThresholds {
            hub_threshold: 3,
            bridge_ratio: 1e9, // unreachable
        };
        let metrics = compute_metrics_with_thresholds(
            &g,
            &ScoringWeights::default(),
            &thresholds,
        );
        let a = metrics.iter().find(|m| m.id == "a").unwrap();
        assert_eq!(a.hotspot_type, HotspotType::Hub);
    }

    #[test]
    fn compute_metrics_in_cycle_populated() {
        // Build a→b→c→a cycle plus an isolated node d.
        let mut g = CodeGraph::new();
        for id in &["a", "b", "c", "d"] {
            g.add_node(module(id));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(2));
        g.add_edge("c", "a", Edge::imports(3));

        let metrics = compute_metrics(&g, &ScoringWeights::default());
        let in_cycle_ids: Vec<&str> = metrics
            .iter()
            .filter(|m| m.in_cycle)
            .map(|m| m.id.as_str())
            .collect();

        assert!(in_cycle_ids.contains(&"a"));
        assert!(in_cycle_ids.contains(&"b"));
        assert!(in_cycle_ids.contains(&"c"));

        let d_metrics = metrics.iter().find(|m| m.id == "d").unwrap();
        assert!(!d_metrics.in_cycle, "'d' should not be in a cycle");
    }
}
