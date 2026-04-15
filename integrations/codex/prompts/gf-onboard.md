Produce an architecture tour for the current codebase using the `graphify-onboarding` skill.

## Instructions

Invoke the `graphify-onboarding` skill. It will:
1. Verify `graphify.toml` and run analysis if needed
2. Delegate to `claude-agent-graphify-analyst` (the Codex bridge wrapper for the `graphify-analyst` agent)
3. Write the tour to `docs/architecture/graphify-tour-<date>.md`
4. Report a 1-paragraph summary

## Arguments

`$ARGUMENTS` — Optional: specific area to emphasize in the tour.
