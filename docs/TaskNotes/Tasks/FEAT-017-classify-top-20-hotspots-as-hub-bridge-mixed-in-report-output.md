---
uid: feat-017
status: done
priority: normal
scheduled: 2026-04-14
completed: 2026-04-14
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- report
- dx
- hotspots
tags:
- task
- feat
---

# Classify top-20 hotspots as hub/bridge/mixed in report output

## Description

The current hotspot score is a single scalar (0.0–1.0) that combines in-degree, out-degree, betweenness, and pagerank. That scalar correctly ranks structural importance, but hides the *kind* of importance — and the refactor strategy differs sharply between the two dominant causes.

## Motivation

Real-world evidence from a ToStudy `pkg-api` analysis:

| Node | Score | in_degree | out_degree | betweenness |
|---|---|---|---|---|
| `src.shared.domain.errors` | 0.595 | **231** | 15 | 11,026 |
| `ProcessManualRefundUseCase` | 0.402 | 2 | 9 | **14,455** |

Both surfaced in the top-5 hotspot list of `architecture_report.md`. A user looking at the list intuits "big complex file" for both — but `ProcessManualRefundUseCase` is an 80-line file with two consumers. Its score comes from bridging four layers (domain/interfaces + domain/entities + domain/errors + application/services) in a single chokepoint. The correct fix is **dependency inversion** (inject the cross-layer call, reduce betweenness). The correct fix for `shared.domain.errors` (a classic hub) is completely different: **extract sub-modules**, invert dependencies of consumers.

Without surfacing the type, users either:
1. Attempt the wrong refactor and don't see the score move.
2. Open `analysis.json` and compute `betweenness / in_degree` by hand to disambiguate (only feasible for power users).

## Proposed Outcome

Classify each top-N hotspot in `check`, `pr-summary`, and `architecture_report.md` as one of:

- **`hub`** — `in_degree > HUB_THRESHOLD` (default: 50). Fix: split module or invert dependencies of consumers.
- **`bridge`** — `betweenness / max(in_degree, 1) > BRIDGE_RATIO` (default: 3000). Fix: inject cross-layer dependencies, reduce chokepoints.
- **`mixed`** — neither threshold dominates; human judgment required.

Thresholds exposed via flags for tuning across very different repo sizes.

## Likely Scope

- Add classifier function to `graphify-core` (pure function over existing node metrics — no extraction change).
- Surface `hotspot_type` field in `analysis.json` node schema (additive, backward compatible).
- Extend `architecture_report.md` hotspot table with a `Type` column.
- Extend `pr-summary` hotspot section to annotate `hub` / `bridge` / `mixed` inline.
- Thresholds configurable via CLI flags (`--hub-threshold`, `--bridge-ratio`) and `graphify.toml` overrides.
- Update README with the classification table and a short "which refactor fits which type" recipe.

## Subtasks

- [ ] Define classifier function signature and default thresholds in `graphify-core`.
- [ ] Validate thresholds against 3+ real-world analysis snapshots (at minimum: one small Python service, one large TS monorepo, one mixed).
- [ ] Add `hotspot_type: "hub" | "bridge" | "mixed"` to the node schema in `analysis.json` (additive).
- [ ] Surface the type in `architecture_report.md` hotspot table.
- [ ] Surface the type in `pr-summary` output (inline annotation near each top hotspot).
- [ ] Add CLI flags `--hub-threshold` and `--bridge-ratio` to `run` and `check`.
- [ ] Wire `[hotspots] hub_threshold = …` / `bridge_ratio = …` keys in `graphify.toml`.
- [ ] Document the classification + recommended-refactor mapping in README.
- [ ] Add fixture + assertion for each class in the existing report snapshot tests.

## Notes

- This is additive — existing consumers reading `analysis.json` keep working.
- The computation is cheap (already have `betweenness` and `in_degree` per node). No new graph traversal.
- Consider whether `pagerank` should influence the classification for edge cases where a node has both high in_degree and high betweenness with modest absolute numbers.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-015-pr-and-editor-integration]] — landing surface for `pr-summary` changes
- [[FEAT-001-interactive-html-visualization]] — the HTML viz can color nodes by classification once the field exists
