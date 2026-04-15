Generate a prioritized refactor plan from Graphify analysis using the `graphify-refactor-plan` skill.

## Instructions

Invoke the `graphify-refactor-plan` skill. It will:
1. Verify prerequisites + ensure fresh analysis
2. Iterate over cycles and top hotspots, delegating each to `claude-agent-graphify-analyst` (the Codex bridge wrapper for the `graphify-analyst` agent)
3. Consolidate into a phased plan
4. Write to `docs/plans/refactor-plan-<date>.md`

## Arguments

`$ARGUMENTS` — Optional: constrain scope (e.g., "only cycles").
