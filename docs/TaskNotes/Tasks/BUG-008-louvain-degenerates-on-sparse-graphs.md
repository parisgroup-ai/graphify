---
status: done
priority: normal
timeEstimate: 120
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - core
  - community
tags:
  - task
  - bug
  - algorithm
  - community-detection
uid: bug-008
---

# fix(core): Louvain community detection degenerates on sparse graphs

## Description

For packages with low edge density, the Louvain algorithm produces near-1:1 node-to-community ratios, making community detection effectively useless.

## Evidence

pkg-types analysis:

| Metric | Value | Expected |
|--------|-------|----------|
| Nodes | 156 | — |
| Communities | 101 | ~10-20 |
| Single-node communities | ~93 | <10 |
| Ratio (nodes:communities) | 1.5:1 | 10:1+ |

Comparison with healthy package:

| Package | Nodes | Communities | Ratio |
|---------|-------|-------------|-------|
| pkg-types | 156 | 101 | **1.5:1** (broken) |
| pkg-jobs | 765 | 14 | 55:1 (healthy) |
| pkg-api | 11,597 | 530 | 22:1 (healthy) |

Source: `report/pkg-types/analysis.json`. 91% of communities are isolated single nodes.

## Root Cause

In `crates/graphify-core/src/community.rs`, the Louvain Phase 1 (modularity optimization) converges too quickly when the graph is sparse:

1. Lines 79-81: Nodes with no edges are pre-assigned to their own community
2. The modularity gain calculation (lines ~85-136) finds no positive ΔQ for merging isolated nodes
3. The algorithm terminates with most nodes still in singleton communities
4. Label Propagation fallback may also fail on disconnected subgraphs

pkg-types has many type-only modules that export types but have few import edges (they're consumed via `@repo/types` barrel export, not via direct imports).

## Fix Approach

1. **Post-processing merge:** After Louvain completes, merge singleton communities into the nearest connected community (by shared edge weight)
2. **Minimum community size:** Add configurable `min_community_size` (default 2-3). Singletons below threshold get absorbed into nearest neighbor's community
3. **Better handling of disconnected components:** Run Louvain per connected component, not on the whole graph. Disconnected nodes get a special "unclustered" label.

## Affected Code

- `crates/graphify-core/src/community.rs` — `louvain()`, possibly `label_propagation()`
- `crates/graphify-core/src/graph.rs` — may need `connected_components()` helper

## Impact

- Community count is inflated for any package with low edge density
- Makes the "community clusters" section of architecture_report.md useless for sparse packages
- Affects: pkg-types (confirmed), likely also pkg-resilience, pkg-llm-costs, and other small packages
- Severity is moderate because the main analysis (hotspots, cycles) is unaffected
