---
uid: feat-037
status: done
priority: normal
scheduled: 2026-04-22
completed: 2026-04-22
pomodoros: 0
contexts:
- pr-summary
- metrics
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: false
---

# FEAT: architectural smell edges in pr-summary

Add a top-N (default 5) "architectural smell edges" section to `graphify pr-summary` output, ranking edges by a composite score combining confidence, cross-community bridging, peripheral-to-hub coupling, and hotspot adjacency. Surfaces the handful of edges most worth reviewing, without requiring the reviewer to read the full hotspot / cycle sections.

Adapted from the `surprising_connections` / `_surprise_score` pattern in the unrelated `safishamsi/graphify` Python project (surveyed 2026-04-22). Their version optimizes for AI-explainability ("tell me interesting stuff"); ours reframes for code review ("what coupling should I stop in PR review").

## Motivation

`pr-summary` today reports hotspot changes and drift deltas — useful, but the reviewer still has to decide which new edges in a PR are the problematic ones. An unfamiliar PR can add dozens of `Imports` edges with no obvious way to triage.

A per-edge composite score can promote the 3-5 edges that are most likely to need discussion:

1. **Low-confidence** edges (`Ambiguous`, `Inferred` below 0.7) — imports the resolver couldn't pin down, a.k.a. hidden coupling.
2. **Cross-community** edges — coupling between modules Louvain said should be separate concerns.
3. **Peripheral→hotspot** edges — low-indegree node suddenly depending on a hotspot, a classic "why is this new leaf touching the god module" smell.
4. **In-cycle** edges — cycles participation, already reported elsewhere but worth reinforcing.

This is *not* noise-ranking by volume (that's what `hotspot_score` already does). It's signal-ranking by structural unusualness.

## Design

### Score formula

```
score(edge) =
    + confidence_bonus(edge)           # 3 if Ambiguous, 2 if Inferred, 1 if Extracted, 0 if ExpectedExternal
    + 2  if source_community != target_community
    + 2  if edge participates in any simple cycle
    + 1  if min(degree(src), degree(tgt)) <= 2 AND max >= 5     # peripheral→hub
    + 1  if source OR target is in top-10 hotspots
```

Integer score, range roughly 0-9. Ties broken by (a) presence in drift-report as "new edge" if drift data is available, (b) lexicographic `src → tgt`.

Output: top 5 (configurable via `--top N` or `[settings].smell_top_n`). Each result includes a `why` list — the specific bonuses that contributed, so the reviewer sees *why* it was flagged.

### Pure renderer

`pr-summary` is already a pure renderer over `analysis.json` + optional drift/check reports (per CLAUDE.md). Keep it pure: compute the score from `analysis.json` alone (it has communities, cycles, confidence, degrees) — no fresh graph extraction.

### Output section (Markdown)

```markdown
## Architectural smells

Edges flagged by the smell detector (top 5 of N considered):

| Source → Target | Kind | Confidence | Score | Why |
|---|---|---|---|---|
| `src.resolver` → `src.cache` | Imports | Ambiguous (0.45) | 8 | cross-community; low-confidence; touches hotspot `src.resolver` |
| ... |
```

Emit zero rows (no section) if nothing scores above a minimum (floor = 3, to avoid flagging single-attribute edges).

## Implementation

1. Extend `analysis.json` (if not already) to expose per-edge: source, target, edge-kind, confidence_kind, confidence-float, source_community, target_community, in-cycle flag. Most are already there — audit `graphify-core/src/analysis.rs` (or the equivalent) to confirm.
2. New pure function in `graphify-report/src/pr_summary.rs` or a sibling module: `score_smells(analysis: &AnalysisSnapshot, top_n: usize) -> Vec<SmellEdge>`.
3. `SmellEdge` struct: `source`, `target`, `edge_kind`, `confidence`, `score: u32`, `reasons: Vec<String>`.
4. Integrate into the existing `render(project_name, analysis, drift, check)` entry point — append a `## Architectural smells` section when `score_smells` returns non-empty.
5. CLI surface: `--top N` flag on `graphify pr-summary`, default 5. Zero means "suppress section".
6. Unit tests in the smell module:
   - `score_includes_confidence_bonus`
   - `score_flags_cross_community_edge`
   - `score_flags_in_cycle_edge`
   - `score_flags_peripheral_to_hub`
   - `score_flags_hotspot_adjacent`
   - `floor_filters_low_score_edges`
   - `top_n_zero_returns_empty`
   - `tie_break_prefers_drift_new_edges`
7. Integration test: end-to-end `graphify pr-summary` against a fixture `analysis.json` with known smells, assert Markdown output contains the expected edge rows.

## Test plan

- `cargo test -p graphify-report smells::` for scoring unit coverage.
- `cargo test --workspace` green overall.
- Dogfood: run `graphify pr-summary report/graphify-core` post-BUG-019 — expect the hand-validated post-BUG-019 edges to surface (the 13 Calls that got promoted Ambiguous → Inferred ought to score higher than background).
- Reviewer UX test: create a synthetic PR that adds a cross-community Imports from a peripheral leaf to a hotspot → confirm it bubbles to position #1 in the smells section.
- `cargo clippy --workspace -- -D warnings` clean.

## Acceptance criteria

- [x] `graphify pr-summary` emits an `## Architectural smells` section with up to N rows
- [x] Score formula produces ties-broken, deterministic output for identical inputs
- [x] All 8 unit tests pass (shipped 11)
- [x] Integration test with fixture analysis.json green
- [x] `--top 0` suppresses the section entirely
- [x] Floor = 3 filters out trivial single-attribute edges
- [x] Reviewer-facing `why` column explains each score contribution

## Out of scope

- **Cross-file-type / cross-repo bonuses** (the Python reference scores `code↔paper`, `code↔image`). Not relevant — we only analyze code.
- **Semantic similarity bonus** (Python reference). Requires embeddings, wrong scope.
- **HTML report integration**. This ticket is `pr-summary` only; adding to HTML is a follow-up if signal proves useful.
- **Fail-CI-on-smell gating**. Current `graphify check` gates on hotspot scores and contract violations. Adding smell as a gate is a separate policy call — don't bundle.

## Discovered context

Surveyed 2026-04-22 while comparing graphify with `safishamsi/graphify` (Python, unrelated project) cloned at `/Users/cleitonparis/www/pg/repos_outros/graphify`. Their `analyze.py:131::_surprise_score` formula is the direct reference; see session notes. Key divergence: their version is tuned for GraphRAG "tell me interesting things about this knowledge graph"; ours reframes for PR review ("tell me which edges I should read first"). Overlapping signals — confidence, cross-community, peripheral→hub — ported. Divergent signals — cross-file-type, cross-repo, semantic similarity — dropped.

## Related

- `graphify-report/src/pr_summary.rs` — pure renderer to extend.
- FEAT-035 (cohesion score) — if per-community cohesion is exposed on `analysis.json`, smell scoring could add a "low-cohesion-community edge" bonus.
- FEAT-017 (classify hotspots as hub/bridge/mixed) — smell scoring can leverage the classification if it lands.
- `graphify check` — smell section is informational, not a gate. Check stays the gate.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
