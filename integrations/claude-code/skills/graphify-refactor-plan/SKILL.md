---
name: graphify-refactor-plan
description: "Generate a prioritized, multi-phase architectural refactor plan from Graphify analysis. Use when the user says 'plan a refactor', 'where should I start refactoring', 'reduce coupling', or wants to tackle hotspots, cycles, or consolidation systematically."
version: 1.0.0
min_graphify_version: "0.6.0"
---

# Graphify Refactor Plan

## Purpose

Produce a prioritized, phased plan for resolving architectural issues in a codebase. Each item includes: target, minimum-disruption approach, expected score delta, verification command.

## Prerequisites

Same as `graphify-onboarding`. Additionally, if `report/baseline/analysis.json` exists, load it for drift-aware prioritization (items that got worse since baseline rise in priority).

## Flow

1. Verify prerequisites + ensure fresh analysis exists
2. List every cycle from `analysis.json` and the top 5 hotspots by `score`
3. For each item, delegate to the `graphify-analyst` agent with this prompt:

   > Minimum-disruption fix for `<issue description>`? Include:
   > - target edge/node to modify
   > - estimated effort (one of: `file-move`, `api-rename`, `signature-change`, `split`, `consolidate`)
   > - expected score delta (for the affected node)
   > - verification command (a `graphify check` or `graphify diff` invocation)
   >
   > Do not suggest implementation code. Start with the target; no preamble.

4. Consolidate the analyst's responses into a ranked plan:
   - **Phase 1 — Break Cycles** (all cycles; blockers for any other refactor)
   - **Phase 2 — Hotspots** ordered: `hub` kind first (split), then `bridge` (decouple), then `mixed` (investigate)
   - **Phase 3 — Consolidation** — invoke the `code-consolidation` skill if available; otherwise leave a placeholder section with the candidate list
   - **Phase 4 — Verification** — literal `graphify diff` + `graphify check` commands against the pre-refactor snapshot
5. Write the plan to `docs/plans/refactor-plan-$(date +%Y-%m-%d).md`
6. Report a chat summary: total cycles, total hotspots, estimated PR count, cumulative expected score delta (all A1-estimated)

## Output File Structure

```markdown
# Refactor Plan — <project>

## Summary
- N cycles · M hotspots · est. K PRs · expected cumulative score delta: -D (A1-estimated)

## Phase 1 — Break Cycles
### 1.1 Cycle: A → B → A
- Break edge: `B → A` (weight 1, confidence 0.6)
- Effort: `api-rename` via interface in `pkg/contracts`
- Expected: cycle count 3 → 2
- Verification: `graphify check --max-cycles 2`

…

## Phase 2 — Hotspots
### 2.1 `app.services.llm` (hub, score 0.92)
- Split into `.client` + `.cache` + `.retry`
- Effort: `split`
- Expected: 0.92 → ~0.55
- Verification: `graphify diff --before /tmp/pre.json --after report/<project>/analysis.json`

…

## Phase 3 — Consolidation
(delegated to `code-consolidation` skill, or candidate list if unavailable)

## Phase 4 — Verification
\`\`\`bash
graphify diff --before /tmp/pre.json --after report/<project>/analysis.json
graphify check --max-cycles 0 --max-hotspot-score 0.85
\`\`\`
```
