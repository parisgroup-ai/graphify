---
uid: feat-022
status: done
priority: normal
scheduled: 2026-04-18
completed: 2026-04-18
timeEstimate: 60
pomodoros: 0
timeSpent: 64
timeEntries:
- date: 2026-04-18
  minutes: 32
  type: manual
  executor: claude-solo
  tokens: 55000
- date: 2026-04-18
  minutes: 32
  type: manual
  executor: claude-solo
  tokens: 55000
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- consolidation
- cli
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

# feat(cli): `graphify consolidation` subcommand emits consolidation-candidates.json

## Description

Ship the native consolidation subcommand deferred from FEAT-020. The
allowlist core (config parsing, `analysis.json#allowlisted_symbols`,
`--ignore-allowlist`) already landed in `25eabc8`; what remains is the
CLI surface that replaces the bash+grep+python script currently shipped
in the `code-consolidation` skill
(`graphify-consolidation-scan.sh`).

The subcommand walks the per-project `analysis.json` + `graph.json`
outputs, groups candidate symbols by leaf-name equivalence, applies the
`[consolidation]` allowlist to filter intentional mirrors, and emits a
single `consolidation-candidates.json` per project plus an aggregate
across projects.

Source: [parisgroup-ai/graphify#13](https://github.com/parisgroup-ai/graphify/issues/13)
(Proposal A, deferred subtask). Partner task: [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]].

## Motivation

- Today the `code-consolidation` skill reinvents the grouping logic in
  shell + Python — slow, brittle, and untested.
- Consumers already have to install `graphify`; making the subcommand
  native removes one dependency hop and unifies error surface.
- Emitting JSON (not just markdown) lets downstream automations consume
  candidates without regex-parsing human-oriented reports.

## Proposed Outcome

New subcommand:

```bash
graphify consolidation --config graphify.toml
# writes ./report/<project>/consolidation-candidates.json per project
# writes ./report/consolidation-candidates.json (aggregate) when 2+ projects
```

Flags:

- `--config <PATH>` (required, consistent with other subcommands)
- `--ignore-allowlist` (debug bypass — same semantics as `run`/`check`)
- `--min-group-size <N>` (default 2; drops singletons from output)
- `--format json|md` (default `json`; `md` emits human-readable grouping)

JSON schema (per-project). Fields shown with illustrative placeholders
(`<consumer>/...`) — real output uses the actual paths recorded on each
node during extraction:

```json
{
  "schema_version": 1,
  "project": "<project-name>",
  "generated_at": "2026-04-18T10:00:00Z",
  "allowlist_applied": 74,
  "candidates": [
    {
      "leaf_name": "TokenUsage",
      "group_size": 3,
      "members": [
        {"id": "MODULE_A.TokenUsage", "kind": "class", "fan_in": 12, "file": "PATH_TO_MODULE_A", "line": 18},
        {"id": "MODULE_B.TokenUsage", "kind": "class", "fan_in":  3, "file": "PATH_TO_MODULE_B", "line": 44}
      ],
      "allowlisted": false
    }
  ]
}
```

Aggregate file adds `project` field per candidate and groups across
projects for cross-project mirrors.

## Likely Scope

- New module `consolidation` under the `graphify-report` crate
  (`crates/graphify-report/src/`) — pure renderer:
  `render(analysis, allowlist, opts) -> ConsolidationReport`.
- New `cmd_consolidation` inside the existing CLI entrypoint
  (`crates/graphify-cli/src/main.rs`) mirroring `cmd_report` plumbing
  (config load → per-project loop → write).
- Reuse `AnalysisSnapshot` from `graphify-core::diff` (already public)
  rather than re-parsing `analysis.json` ad hoc.
- Reuse the compiled allowlist regex set from FEAT-020 (exposed via
  `graphify-core::consolidation::AllowlistMatcher` or similar — add a
  `pub fn matches(leaf: &str) -> bool` helper if not already there).
- Exit codes follow CLI convention: `0` success (even when candidates
  non-empty — gating is `graphify check`'s job), `1` on config or I/O
  error.
- Tests: fixture under `tests/fixtures/consolidation/` with 2 toy
  projects sharing a `TokenUsage` class; snapshot-test the JSON output;
  regression for `--ignore-allowlist` bypass.

## Subtasks

- [ ] Design `ConsolidationReport` / `Candidate` / `Member` structs in
      `graphify-report::consolidation` with serde derives; decide JSON
      schema versioning (start at `schema_version: 1`).
- [ ] Implement grouping: iterate `analysis.nodes`, bucket by leaf
      symbol name (last `.`-segment), keep buckets of size ≥ `min_group_size`.
- [ ] Apply allowlist: mark buckets whose leaf matches; drop when
      `--ignore-allowlist` is absent, include with `allowlisted: true`
      otherwise. Confirm exact policy matches FEAT-020's behavior for
      `analysis.json#allowlisted_symbols`.
- [ ] Add `cmd_consolidation` in CLI; wire `--format md` to a small
      markdown renderer (mirror the existing `architecture_report.md`
      styling — headers, tables).
- [ ] Aggregate mode: when `config.projects.len() >= 2`, emit
      `./<out>/consolidation-candidates.json` with cross-project groups.
- [ ] Fixture tests (integration under `tests/`) covering: single project,
      multi-project aggregate, allowlist hit, `--ignore-allowlist` bypass,
      `--min-group-size=3` filtering.
- [ ] README recipe + migration note (tie-in with DOC-001) showing the
      skill's old bash invocation → new `graphify consolidation` call.
- [ ] Deprecation note in the `code-consolidation` skill's shell script
      pointing to the subcommand (kept working for one release cycle).

## Open Questions

1. Should the md `--format` variant live here or be deferred to a
   follow-up? Leaning "here, minimal" — the grouping logic is the same;
   only the renderer differs.
2. Aggregate file location — top-level `report/` or a dedicated
   `report/consolidation/`? Current convention (`graphify-summary.json`
   at top level) argues for top-level.
3. Do we expose `alternative_paths` (FEAT-021 concept) in the schema now
   as `null`/`[]` so FEAT-021 can fill it later without a schema bump?
   Recommend yes — additive and future-proof.

## Notes

- Depends on FEAT-020's `AllowlistMatcher` being reachable from
  `graphify-report`. If it lives in `graphify-cli` today, extract to
  `graphify-core` first (trivial move).
- Cross-project grouping should use leaf name **only** — trying to
  identify "same logical contract" across languages is out of scope and
  belongs to FEAT-023 (intentional_mirrors).

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]] — core
  allowlist infrastructure this task consumes.
- [[FEAT-023-feat-drift-honour-consolidation-intentional-mirrors-to-suppress-cross-project-drift-entries]] —
  complementary cross-project drift suppression.
- [[FEAT-024-feat-pr-summary-integrate-consolidation-allowlist-into-pr-summary-hotspot-annotations]] —
  consumer of the same allowlist in the PR summary surface.
- [[DOC-001-docs-consolidation-readme-section-migration-note-for-consolidation-ignore-to-graphify-toml]] —
  documentation handoff.
- GH issue: <https://github.com/parisgroup-ai/graphify/issues/13>
