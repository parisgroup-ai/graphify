---
uid: feat-035
status: done
priority: normal
scheduled: 2026-04-22
completed: 2026-04-22
pomodoros: 0
contexts:
- metrics
- community
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: low
  hintsInferred: false
---

#  FEAT: community cohesion score

Add a per-community `cohesion` metric (intra-community edges ÷ max possible) to `analysis.json`. Diagnoses communities that Louvain grouped but are structurally thin — complementary to the existing singleton-merge in `community.rs`. Cheap (subgraph edge count), directly portable from the Python reference (`graphify/cluster.py:125`).

## Motivation

Louvain outputs communities of wildly different quality. A community of 12 nodes with 3 edges between them is a weak grouping; one with 12 nodes and 40 edges is strong. Today `analysis.json` exposes community membership but no quality signal — a consumer (pr-summary, HTML report, graphify-analyst agent) can't tell which communities are load-bearing vs. which are Louvain noise.

The upstream Python implementation (reference codebase at `/Users/cleitonparis/www/pg/repos_outros/graphify` — unrelated `safishamsi/graphify` project we surveyed 2026-04-22) has a 10-line `cohesion_score` in `graphify/cluster.py:125` that computes `actual_edges / possible_edges` (where possible = n(n-1)/2 for undirected). We should add the same metric for each of our communities. This is a direct port of a proven idea, not a new design.

## Design

Formula (treat edges as undirected for counting, since cohesion is a structural-density metric):

```
cohesion(community) = intra_edges / (n * (n - 1) / 2)    if n >= 2
cohesion(community) = 1.0                                 if n <= 1
```

Range: `[0.0, 1.0]`. Singleton / empty communities defined as `1.0` (no missing connections possible), consistent with the reference implementation.

Add a `cohesion: f64` field to `Community` in `graphify-core/src/community.rs`. Populate during `detect_communities` and `label_propagation` by iterating edges once per community and counting those with both endpoints inside it. O(E) per community call site; reuse the existing `HashMap<usize, usize>` node→community index to look up membership in O(1).

Expose the new field in `analysis.json` via whatever serializer currently writes communities (check `graphify-report/src/analysis_json.rs` or equivalent — the struct derives `Serialize`, so likely automatic). Add to the Markdown report's community section under an existing table/heading.

## Implementation

1. Add `cohesion: f64` to `Community` struct in `graphify-core/src/community.rs` (after the `nodes` field).
2. New private helper `compute_cohesion(nodes: &[usize], graph: &CodeGraph) -> f64` in the same file. Count undirected distinct edges where both endpoints are in `nodes`.
3. Call the helper at the end of `build_communities` and after the Phase 2 singleton-merge. Same for `label_propagation`.
4. Unit tests in `community.rs`:
   - `cohesion_singleton_is_one`
   - `cohesion_pair_with_no_edge_is_zero`
   - `cohesion_pair_with_edge_is_one`
   - `cohesion_triangle_is_one`
   - `cohesion_three_nodes_one_edge_is_one_third`
   - `detect_communities_emits_cohesion_per_community`
5. Writer: verify `analysis.json` includes `cohesion` per community (serde default derive should handle it; add a snapshot test if there isn't one).
6. Markdown report: append `(cohesion N.NN)` next to each community size in the existing communities section.

## Test plan

- `cargo test -p graphify-core community::` for unit coverage.
- `cargo test --workspace` for no regressions.
- Manual: run `graphify run --config graphify.toml` on the self-dogfood; inspect `report/graphify-core/analysis.json` and confirm `cohesion` field present with sensible values (expect ~0.3-0.6 for code-module communities, ~1.0 for singletons).
- `cargo clippy --workspace -- -D warnings` clean.
- `cargo fmt --all -- --check` clean.

## Acceptance criteria

- [ ] `Community.cohesion: f64` field populated by both `detect_communities` and `label_propagation`
- [ ] `analysis.json` includes `cohesion` per community (field name stable, snake_case)
- [ ] 6 unit tests listed above pass
- [ ] Markdown report surfaces cohesion per community
- [ ] Zero regressions on dogfood run (node/edge counts, hotspot scores unchanged)

## Out of scope

- **Action on low-cohesion communities** (split them, re-run Louvain with different resolution, flag in `check`). Follow-up ticket if we decide to act on the signal. This ticket just measures.
- **Conductance / modularity** per community. Cohesion is the simplest structural density metric; more sophisticated ones can come later.
- Weighting by edge confidence. Cohesion treats all edges as unit-weight for now.

## Related

- Reference: `graphify/cluster.py:125` in `/Users/cleitonparis/www/pg/repos_outros/graphify` (survey notes captured in session on 2026-04-22).
- Complement to FEAT-036 (split oversized communities) — cohesion is the diagnostic; splitting is one possible remediation.
- Related to FEAT-037 (architectural smell edges) which may reference community cohesion in its per-edge score.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
