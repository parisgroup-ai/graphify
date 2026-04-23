---
uid: feat-040
status: done
priority: normal
scheduled: 2026-04-23
completed: 2026-04-23
timeEstimate: 60
pomodoros: 0
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: low
  hintsInferred: true
---

# FEAT-040: Per-project [project.check] threshold overrides (issue #14)

Ship `[project.check]` sub-table under `[[project]]` in `graphify.toml` so a workspace can acknowledge a legitimate high-in-degree facade in one project without relaxing the global hotspot gate for every sibling. Closes GitHub issue #14. Shipped as v0.12.2.

## Motivation

Issue #14 filed by `pageshell` consumer workspace (6 projects): all pass `graphify check --max-cycles 0 --max-hotspot-score 0.70` except `pageshell-native.theme.NativeThemeContext` which scores 0.738 as a legitimate 48-consumer React context facade. Two in-tree workaround attempts (icon-registry extraction, `useNativeTheme` hoist) both failed to move the score meaningfully because the score is on the function node, not the module — relocating a symbol does not change its fan-in. The only honest action is to acknowledge the exception in config.

Pre-v0.12.2 options were all bad:
1. Drop workspace gate 0.70 → 0.75 (loses regression detection on the other 5 projects)
2. Fork CI invocation per-project (wiring drift, doesn't appear in local `graphify check`)
3. Marker comments (no such facility)

## Design

### Schema

```toml
[[project]]
name = "pageshell-native"
repo = "./pageshell-native/src"
lang = ["typescript"]
local_prefix = "@parisgroup-ai/pageshell-native"

[project.check]
max_hotspot_score = 0.75
max_cycles = 0
# Rationale: NativeThemeContext is a legitimate 48-consumer facade.
```

### Precedence (per-field)

`[project.check]` > CLI flag > None (no gate)

Diverges from the issue's literal suggested-behaviour bullet 1 (which claimed CLI > TOML, matching the `tsconfig`/`eslint` "CLI wins" convention for *transient* overrides). The motivating use case only works when the narrower scope wins: workspace CI keeps `--max-hotspot-score 0.70` as a default, individual projects opt out inline. `[project.check]` is a permanent, code-reviewed exception — not a debug toggle — so it shadows the workspace default the same way `tsconfig` per-project config shadows a root extends. Documented in the `effective_limits` doc-comment.

### Implementation

`crates/graphify-cli/src/main.rs`:

- `ProjectConfig` gains `check: Option<ProjectCheck>` field.
- New `ProjectCheck { max_cycles: Option<usize>, max_hotspot_score: Option<f64> }` struct with `#[serde(deny_unknown_fields)]` — typos inside the block (e.g. `max_hoptspot_score`) fail the parse instead of silently disabling the gate. Stronger than the issue's asked-for "warn on unknown keys" because silent gate-disable is worse than a loud parse error.
- New `effective_limits(cli, project) -> CheckLimits` free function applies the precedence rule per field: `project.foo.or(cli.foo)`. Kept as a standalone fn so there is exactly one test target for the precedence rule.
- `cmd_check` builds a per-project `HashMap<String, CheckLimits>` before the evaluation loop so each project is gated by its own effective limits, and `ProjectCheckResult.limits` in `check-report.json` reflects effective values per project (different projects can now legitimately show different limits downstream).

### Schema additive guarantee

Configs without `[project.check]` are untouched: the field is `Option<ProjectCheck>` with default `None`, and `effective_limits(cli, None)` returns just the CLI values. v0.12.1 → v0.12.2 is a pure additive schema bump.

## Subtasks

- [x] Add `ProjectCheck` struct with `deny_unknown_fields` + `check` field on `ProjectConfig`
- [x] Implement `effective_limits(cli, project)` precedence merge
- [x] Wire per-project effective limits through `cmd_check` loop
- [x] 6 unit tests (`issue_14_*`): precedence matrix, TOML parse happy + typo rejection, end-to-end gate trip
- [x] Dogfood on graphify's 5-crate workspace: CLI `--max-hotspot-score 0.30` trips 4, `graphify-mcp` passes via `[project.check] max_hotspot_score = 0.60` at real score 0.559
- [x] Bump workspace version 0.12.1 → 0.12.2 (all crates via `version.workspace = true`)
- [x] Commit `8065045` + tag `v0.12.2` + push `main --tags`
- [x] Refresh `~/.cargo/bin/graphify` via `cargo install --path crates/graphify-cli --force`
- [x] Close issue #14 with PR comment explaining precedence inversion vs. original text
- [x] Follow-up doc pass: README `Per-project threshold overrides` section + `graphify init` template hint + this task file

## Notes

### Lessons learned

1. **When the issue text contradicts the issue's motivating use case, follow the use case.** Filing-time drafts often invoke precedence heuristics by analogy ("CLI usually overrides config") that do not survive the first end-to-end walk-through of the stated scenario. I flagged the contradiction in-conversation and got a one-char confirmation (`1`) before shipping the inverted rule — cheaper than merging against the motivation and reverting later. Pattern for future issue triage: always mentally run the user's stated workflow end-to-end against the proposed semantics before implementing.

2. **`deny_unknown_fields` on a nested struct is surgical.** Putting it on the whole `Config` would break forward compatibility (new top-level keys fail the parse); scoping it to just `ProjectCheck` catches typos where they matter most (a misspelled gate is a silent CI regression) without touching the rest of the schema. General rule: guard the inner structs where silent failure would be expensive; leave outer structs lenient for forward compat.

3. **Per-project `check-report.json` limits is a downstream contract.** Because each project's `ProjectCheckResult.limits` now reflects effective (potentially different) values, downstream consumers (`graphify pr-summary`) automatically show the right gate per project with no changes. Would have been easy to miss if I had kept `limits` as a single workspace-level value.

### Test philosophy

6 `issue_14_*` tests in `graphify-cli/src/main.rs::tests`:

- `issue_14_effective_limits_project_check_takes_precedence_over_cli` — the motivating use case as a unit test
- `issue_14_effective_limits_cli_fills_when_project_check_absent` — fallback path
- `issue_14_effective_limits_none_when_both_absent` — degenerate path (no gate)
- `issue_14_effective_limits_mixed_per_field_precedence` — each dimension independently resolved
- `issue_14_project_check_parses_from_toml_and_rejects_typos` — dual guard: happy-path parse + `deny_unknown_fields` typo rejection
- `issue_14_effective_limits_trips_hotspot_gate_per_project` — end-to-end through `evaluate_quality_gates`

No integration test added because the existing `cmd_check` flow has no test harness and the precedence logic is fully covered by the unit slice.

### Dogfood artifact

Throwaway config at `/tmp/graphify.issue14.toml` (copy of root `graphify.toml` with `[project.check] max_hotspot_score = 0.60` appended under the last `[[project]]` block = `graphify-mcp`). Not checked in.

Command: `./target/release/graphify check --config /tmp/graphify.issue14.toml --max-hotspot-score 0.30`
Result: 4 projects FAIL (gated at 0.30), graphify-mcp PASS (gated at 0.60, actual 0.559).

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- GitHub issue #14 (closed): https://github.com/parisgroup-ai/graphify/issues/14
- Commit: `8065045` (feat(check): per-project [project.check] threshold overrides (#14, v0.12.2))
- Tag: `v0.12.2`
