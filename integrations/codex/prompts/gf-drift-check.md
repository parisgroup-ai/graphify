Run architectural drift gate against a baseline using the `graphify-drift-check` skill.

## Instructions

Invoke the `graphify-drift-check` skill. When the skill delegates to the CI guardian, it resolves to `claude-agent-graphify-ci-guardian` (the Codex bridge wrapper for the `graphify-ci-guardian` agent).

If `$ARGUMENTS` is empty, defaults to `report/baseline/analysis.json`.

Output: PR-comment-ready Markdown + exit code.

## Arguments

`$ARGUMENTS` — Optional: `--baseline <path>`.
