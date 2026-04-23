---
uid: feat-038
status: done
priority: low
scheduled: 2026-04-22
completed: 2026-04-22
pomodoros: 0
timeSpent: 28
timeEntries:
- date: 2026-04-22
  minutes: 28
  note: source=<usage>,heuristic=66000,observed=111805,delta_pct=-41.0
  type: manual
  executor: claude-solo
  tokens: 111805
contexts:
- community
- feat-036-followup
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: false
---

# FEAT: split unsplittable oversized communities via Leiden refinement or spectral bisection

FEAT-036 wired Phase-3 split via Louvain greedy local-moves + label-propagation fallback. Both are greedy and can converge to a single community on internally-connected subgraphs without obvious structural cuts. Dogfood evidence (2026-04-22): 3 of 4 oversized communities in graphify's own 5-crate workspace resisted the split. Add a stronger sub-pass for these cases.

## Motivation

Self-dogfood on 2026-04-22 (after FEAT-036 landed) confirmed the known limitation is non-trivial in practice:

| Crate | Community size | Sub-Louvain result | Sub-label-prop result |
|---|---|---|---|
| graphify-report | 75 | split into 2 ✓ | — (not run, Louvain succeeded) |
| graphify-cli | 197 | 1 (no split) | 1 (no split) |
| graphify-mcp | 59 | 1 (no split) | 1 (no split) |
| graphify-mcp | 42 | 1 (no split) | 1 (no split) |

3 of 4 oversized communities stay unsplit. The Python reference uses Leiden (graspologic), which has a refinement step guaranteeing well-connected communities — that's why the Python version has higher split coverage. Rust has no mature Leiden crate; we need an alternative.

## Design options

### Option A — Port Leiden refinement step

Leiden's main differentiator from Louvain: after the aggregation phase, every community is "refined" by running a second local-moves pass *within* the community, starting each node in its own label, but only allowing moves that strictly improve modularity. This breaks communities that are "weakly connected" internally — exactly our failure mode.

Effort: ~200-400 LOC. No new dependency. Cleanly fits our existing `louvain_local_moves` helper.

Risk: algorithm subtleties (connected-subset check, refinement constraints). Medium.

### Option B — Spectral bisection

For each oversized unsplit community, compute the Fiedler vector (eigenvector for second-smallest eigenvalue of the normalized Laplacian) and split by sign. Well-known technique, always produces a 2-way split.

Effort: requires an eigensolver. `nalgebra` or `ndarray-linalg` gets us one. +1 dep.

Risk: arbitrary-looking splits when the community genuinely has no structure. Over-partitions.

### Option C — Kernighan-Lin modularity bisection

Iterative 2-swap heuristic optimizing modularity. No eigensolver. Always produces a split but may be marginal.

Effort: ~100 LOC. No new dependency.

Risk: marginal splits on truly homogeneous subgraphs.

### Recommendation

Start with **Option A** (port Leiden refinement). Closest to the Python reference behavior, no new dependency, cleanest fit with existing code. Reserve B and C as later fallbacks if A still leaves unsplittable cases.

## Implementation (Option A sketch)

1. New private helper in `crates/graphify-core/src/community.rs`:

   ```
   fn leiden_refine(
       community: &mut [usize],
       adj: &[HashMap<usize, f64>],
       degree: &[f64],
       m: f64,
   )
   ```

   Implements the refinement phase: for each current community, extract its members, start each in its own label, run constrained local-moves (only strictly-positive modularity gains), within the community's subgraph.

2. Wire into `split_oversized` as a third attempt after Louvain and label-propagation fallback both return a single sub-label.

3. Recursion depth still capped at 1 — refinement is a single-pass within each oversized community.

## Test plan

- Unit: synthetic graphs that Louvain collapses but Leiden refinement splits (hub-and-spoke, densely-connected cliques with weak inter-clique bridges).
- Integration: confirm graphify-cli 197-community and graphify-mcp 59/42-communities split after the change (verified via dogfood pre/post).
- Regression: all existing FEAT-036 tests stay green.
- `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --all -- --check`.

## Acceptance criteria

- [ ] Unsplittable oversized communities in graphify's self-dogfood (3 of 4) get split post-change
- [ ] Python ref parity: at least match the qualitative split coverage that Leiden+graspologic produces
- [ ] No new dependency introduced (rules out Option B for v1)
- [ ] All existing tests pass
- [ ] Refinement pass is deterministic (snapshot test on a fixed input)

## Out of scope

- **Full Leiden algorithm** (including aggregation optimization). Just the refinement step, which is the differentiating improvement over Louvain.
- **Tuning** Louvain/LP to split these cases. They converge to a single community by design — no knob will fix that without changing the algorithm.
- **Per-language heuristics** to identify unsplittable-but-real-structure subgraphs before running sub-pass. Premature optimization.

## Discovered context

Discovered 2026-04-22 during FEAT-036 implementation (session closed same day, FEAT-036 marked done with this follow-up tracked). The split was designed to be best-effort; dogfood confirmed 3 of 4 oversized communities are resistant. FEAT-036 task body already marked this as out-of-scope with a "separate FEAT if evidence justifies" note — this ticket is the justified follow-up.

## Related

- `crates/graphify-core/src/community.rs` — `split_oversized` is the integration point.
- FEAT-036 — landed Phase-3 split via Louvain + label-propagation.
- `graphify/cluster.py::_split_community` in `/Users/cleitonparis/www/pg/repos_outros/graphify` — Python ref that uses Leiden via graspologic.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
