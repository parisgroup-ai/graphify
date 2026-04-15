# gf-drift-check

Run architectural drift gate against a baseline using the `graphify-drift-check` skill.

## Instructions

Invoke the `graphify-drift-check` skill with optional `--baseline <path>`.

If `$ARGUMENTS` is empty, defaults to `report/baseline/analysis.json`.

Output: PR-comment-ready Markdown + exit code.

## Arguments

`$ARGUMENTS` — Optional: `--baseline <path>` to override default baseline location.
