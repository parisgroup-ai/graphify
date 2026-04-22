---
uid: feat-036
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
  uncertainty: med
  hintsInferred: false
---

# FEAT: split oversized communities after Louvain

Add a Phase-3 pass to Louvain that splits any community larger than 25% of the graph (min 10 nodes) by running a second Louvain/LabelProp pass on its induced subgraph. Complements the existing Phase-2 singleton-merge — merge fixes fragmentation, split fixes the opposite failure mode: the "catch-all giant community" that Louvain sometimes produces on sparse graphs or weak-clustering inputs.

## Motivation

Louvain is greedy modularity optimization. On graphs with no clear community structure (or with a hub-and-spoke topology), it frequently produces one huge community containing most of the graph plus a tail of small ones. Our current Phase-2 merges connected singletons into their best neighbor — that helps low-degree fragmentation, but does nothing about oversized hubs.

The upstream Python reference implementation (`graphify/cluster.py::_split_community` in the surveyed `safishamsi/graphify` repo) handles this with a simple rule: any community > 25% of total nodes (floor 10) is re-partitioned by a second Leiden pass on its subgraph. In practice this matters most on monorepo analyses where one team's package is the connective tissue — a single `ui-kit`-style community of 300+ nodes provides no architectural signal.

Self-dogfood evidence (graphify's own 5-crate workspace): today's communities are well-distributed, so we won't see impact on our own repo. But `parisgroup-ai/cursos` analysis (see FEAT-029 benchmark) had one community swallowing 40%+ of nodes post-FEAT-028. That's where this pays off.

## Design

After `build_communities` completes Phase 2 (singleton-merge), run Phase 3:

```
THRESHOLD = max(10, round(total_nodes * 0.25))
for each community c in communities:
    if len(c.nodes) > THRESHOLD:
        subgraph = induced_subgraph(c.nodes)
        if subgraph.edge_count() == 0:
            leave as-is (no basis to split)
        else:
            sub_partition = run_louvain(subgraph)
            if sub_partition produces >= 2 non-trivial communities:
                replace c with the resulting communities
            else:
                leave as-is
```

Constants:
- 25% threshold — matches the Python reference (`_MAX_COMMUNITY_FRACTION = 0.25`).
- Floor of 10 nodes — prevents splitting small graphs to noise.

Both constants should be `const` at module scope, documented in-line, not yet configurable via `graphify.toml` (YAGNI — wait for a concrete case where defaults fail).

Recompute `cohesion` (if FEAT-035 landed) on resulting split communities.

## Implementation

1. In `graphify-core/src/community.rs`, add a new private function `split_oversized` that runs on the Phase-2 output.
2. Induced subgraph: reuse whatever graph representation Louvain already uses internally (the adjacency map built in `detect_communities`). Extract nodes + intra-community edges into a fresh `CodeGraph` or a lightweight subgraph wrapper.
3. Second Louvain pass: call the existing `detect_communities` recursively with a recursion guard (never split a community that is itself the result of a split — track via a boolean flag or by bounding recursion depth to 1).
4. Re-index community IDs by size descending (preserve existing determinism contract from `community.rs` comment block).
5. Apply symmetrically to `label_propagation` output.
6. Unit tests in `community.rs`:
   - `split_oversized_no_op_below_threshold` — 20-node graph, biggest community 5 nodes → untouched.
   - `split_oversized_splits_hub_community` — graph with clear bimodal structure inside one community.
   - `split_oversized_leaves_edgeless_community_alone` — community with no internal edges stays intact.
   - `split_oversized_respects_size_floor` — 15-node graph with one 6-node community (40% of total, but < 10 floor) → untouched.
   - `split_oversized_preserves_determinism` — running twice yields identical communities.

## Test plan

- `cargo test -p graphify-core community::split_oversized` — unit coverage.
- `cargo test --workspace` green.
- `cargo clippy --workspace -- -D warnings` clean.
- Dogfood: run on graphify's own config; expect zero splits (communities are already sized <25%). Delta-check top hotspots unchanged.
- External benchmark (optional if accessible): re-run on `parisgroup-ai/cursos` pin `8ff36cc1` and compare community size distribution pre/post — expected: the oversized `src`-rooted community splits into 2-3 sub-communities.

## Acceptance criteria

- [ ] `detect_communities` and `label_propagation` both apply the split pass
- [ ] 5 unit tests listed above pass
- [ ] Recursion depth capped at 1 (no runaway splits)
- [ ] `cargo test --workspace` green
- [ ] Graphify dogfood: no community-count regression, no hotspot score change (since no splits expected)
- [ ] Community determinism preserved (`community_ids` stable across runs, reindexed by size)

## Out of scope

- **Configurable threshold** via `[settings].community_split_threshold`. YAGNI until defaults prove wrong.
- **Leiden algorithm** in place of Louvain. Separate question — FEAT-XXX if ever needed. Splitting fixes the main Louvain failure mode without the port cost.
- **Three-level recursion** (split, then split the splits). Cap at 1 to avoid pathological deep recursion; revisit only if single-pass doesn't help.

## Discovered context

Surveyed 2026-04-22 while comparing graphify with the unrelated `safishamsi/graphify` Python project cloned at `/Users/cleitonparis/www/pg/repos_outros/graphify`. Their `cluster.py:107::_split_community` is the direct reference. Their implementation uses Leiden (graspologic) for the second pass; we use Louvain (since that's what we already have). Behavior-equivalent for the common case.

## Related

- `graphify-core/src/community.rs` — Phase 2 singleton-merge (complement to this Phase 3 split).
- FEAT-035 (cohesion score) — split benefits from recomputing cohesion on resulting communities.
- FEAT-029 — benchmark document showing oversized-community pattern on `parisgroup-ai/cursos`.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
