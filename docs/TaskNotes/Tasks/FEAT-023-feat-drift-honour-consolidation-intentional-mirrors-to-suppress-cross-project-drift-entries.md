---
uid: feat-023
status: in-progress
priority: normal
scheduled: 2026-04-18
timeEstimate: 60
pomodoros: 0
contexts:
- consolidation
- drift
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

# feat(drift): honour consolidation.intentional_mirrors to suppress cross-project drift entries

## Description

FEAT-020 landed the `[consolidation]` section in `graphify.toml`, including
`intentional_mirrors` — a user-curated map of leaf symbol name → list of
`"<project>:<node_id>"` endpoints that are expected to co-exist across
multiple projects by design (cross-language contracts, vendor-namespaced
mirrors, DTO families shared between `ana-service` and `pkg-types`, etc.).

The config is already deserialized and exposed via
`ConsolidationConfig::intentional_mirrors()` in `graphify-core`, but **no
consumer reads it yet**. In particular, `graphify diff` continues to flag
hotspot score changes for mirror symbols as if they were independent drift
events, so a single upstream bump to a shared DTO fires one "rising
hotspot" entry per project that mirrors it — exactly the noise
`intentional_mirrors` was introduced to remove.

This task wires the existing data into the drift pipeline.

## Motivation

On the reference monorepo (16 projects), the same canonical symbol family
(`TokenUsage`, `LessonType`, `AnthropicMessage.*`) appears in multiple
project analyses because the projects agree on a shared contract. When a
release touches that contract, every mirroring project's per-project drift
report surfaces the same score change independently:

```
[ana-service] rising: app.models.tokens.TokenUsage  +0.08
[pkg-types]   rising: src.tokens.TokenUsage         +0.07
[tostudy-api] rising: app.contracts.TokenUsage      +0.06
```

Reviewers have to mentally dedupe — and the CI drift gate in
`graphify-ci-guardian` treats each as an independent regression. Declaring
the group once in `graphify.toml`:

```toml
[consolidation.intentional_mirrors]
TokenUsage = [
  "ana-service:app.models.tokens.TokenUsage",
  "pkg-types:src.tokens.TokenUsage",
  "tostudy-api:app.contracts.TokenUsage",
]
```

should mark all three entries as the same intentional cluster and
collapse them into a single annotated signal.

## Proposed Outcome

`graphify-core::diff` gains awareness of `intentional_mirrors` and each
`ScoreChange` in `HotspotDiff` can carry an **annotation** naming the
mirror group it belongs to. The report writers render the annotation;
consumers can filter on it.

Annotate, don't drop — preserves the signal, stays backward compatible,
and lets the CI guardian choose its own policy (demote to warning vs.
gate).

Shape change (additive):

```json
{
  "hotspots": {
    "rising": [
      {
        "id": "app.models.tokens.TokenUsage",
        "before": 0.34,
        "after": 0.42,
        "delta": 0.08,
        "intentional_mirror": "TokenUsage"
      }
    ]
  }
}
```

The existing `--ignore-allowlist` flag on `run`, `check`, `pr-summary`
gains parity semantics on `diff`: when set, mirror annotation is
suppressed (full opt-out for debugging).

Backward compatible: absent `[consolidation.intentional_mirrors]` or
absent `[consolidation]` → current behaviour, `intentional_mirror` field
omitted from JSON.

## Likely Scope

- Extend the drift input surface (`compute_diff` signature or a sibling
  entry point) to accept an optional `&ConsolidationConfig` alongside the
  two `AnalysisSnapshot` references. Prefer a new `compute_diff_with_config`
  wrapper to keep the existing 2-arg signature stable for current callers.
- Add an optional `intentional_mirror: Option<String>` field to
  `ScoreChange` (additive, `#[serde(skip_serializing_if = "Option::is_none")]`
  so legacy JSON shape is preserved when absent).
- Index `intentional_mirrors` for O(1) lookup keyed by node id, so the
  annotation pass is a cheap single walk over `HotspotDiff.rising`,
  `falling`, `new_hotspots`, `removed_hotspots`.
- Thread the config through the CLI pipeline from `load_config` to the
  `cmd_diff` / `cmd_run --baseline` call sites.
- Update `graphify-report::diff_json` writer to emit the new field.
- Update `graphify-report::diff_markdown` writer to group mirror
  annotations into a dedicated subsection (`### Intentional mirrors`) so
  reviewers see the dedup at a glance.
- Honour `--ignore-allowlist` on `diff` — pass an empty
  `ConsolidationConfig` through when set, mirroring the existing CLI
  convention.
- Fixture-level tests in `graphify-core::diff`:
  - Mirror hit: node id matches a declared endpoint → annotation set.
  - Leaf-name collision without mirror declaration → annotation absent.
  - Config absent → `ScoreChange` JSON shape unchanged.
- Integration-level test in `graphify-cli`: run `graphify diff --before
  A.json --after B.json --config graphify.toml` on a fixture pair and
  assert the rendered Markdown contains the annotated subsection.

## Subtasks

- [ ] Extend the drift engine in `graphify-core::diff` to accept an
      optional `ConsolidationConfig` reference; add
      `compute_diff_with_config` (leave `compute_diff` intact as a
      pass-through with an empty config).
- [ ] Build a node-id → mirror-group-name index from
      `ConsolidationConfig::intentional_mirrors()` once per diff call
      (cheap HashMap build, avoid per-ScoreChange scans).
- [ ] Add `intentional_mirror: Option<String>` to `ScoreChange` with
      `#[serde(skip_serializing_if = "Option::is_none")]` so absent
      config preserves the legacy JSON shape byte-for-byte.
- [ ] Populate the annotation on every `ScoreChange` in `rising`,
      `falling`, `new_hotspots`, `removed_hotspots` when its id is
      present in the index.
- [ ] Update the JSON writer in `graphify-report::diff_json` so the new
      field round-trips end-to-end.
- [ ] Update the Markdown writer in `graphify-report::diff_markdown` to
      render an `### Intentional mirrors` subsection under Hotspots,
      listing annotated entries grouped by mirror name and omitting
      them from the main rising/falling lists.
- [ ] Thread `ConsolidationConfig` through the CLI pipeline from
      `load_config` into `cmd_diff` and `cmd_run --baseline`.
- [ ] Make `--ignore-allowlist` on `diff` pass an empty
      `ConsolidationConfig` to the engine, matching the convention
      already used on `run` / `check` / `pr-summary`.
- [ ] Unit fixture: two-snapshot pair where `TokenUsage` rises in a node
      listed under `intentional_mirrors.TokenUsage`; assert
      `intentional_mirror == Some("TokenUsage")`.
- [ ] Unit fixture: same rising symbol when no config is supplied;
      assert `intentional_mirror == None` and JSON shape matches a
      pre-FEAT-023 golden sample.
- [ ] CLI-level fixture: `graphify diff` with a `graphify.toml` that
      declares mirrors produces Markdown containing the new
      subsection.

## Open Questions

1. **Annotate vs. suppress in Markdown.** The proposed outcome is to
   move annotated entries into a dedicated subsection. Alternative: leave
   them inline, tag each with `(intentional mirror: TokenUsage)`. Dedicated
   subsection keeps the main rising list clean at the cost of one extra
   scan for a reviewer looking for a specific symbol. Current proposal:
   dedicated subsection.
2. **Cycles and community moves.** Should the annotation also apply to
   `CycleDiff.introduced` and `CommunityDiff.moved_nodes`? Likely yes for
   symmetry, but the v1 scope above only covers hotspots — the dominant
   noise source. Deferred unless a reviewer surfaces a concrete case.
3. **Partial mirror match.** If `intentional_mirrors.TokenUsage` lists
   three endpoints and only two appear in the current analyses, is that a
   warning? Probably no — the third project may simply not have been
   analysed in this run. Silent match; surface mismatches in a later
   `graphify consolidation --check-mirrors` diagnostic if users ask.

## Notes

- Prereq: FEAT-020 landed in commit `25eabc8` (`ConsolidationConfig` +
  `intentional_mirrors()` accessor already exist). This task only wires
  existing data through the drift pipeline.
- Sibling of FEAT-024 (`pr-summary` consumes the same config for hotspot
  annotations) — both tasks read `ConsolidationConfig`, but against
  different output surfaces. Order-independent.
- CI impact: `graphify-ci-guardian` currently fails on any new rising
  hotspot above threshold. Once this lands, the guardian can opt to
  demote annotated entries to warnings (follow-up task, out of scope
  here).

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]] — landed
  the config surface this task consumes.
- [[FEAT-024-feat-pr-summary-integrate-consolidation-allowlist-into-pr-summary-hotspot-annotations]]
  — sibling consumer of the same config against `pr-summary`.
- [[DOC-001-docs-consolidation-readme-section-migration-note-for-consolidation-ignore-to-graphify-toml]]
  — README update covers both this and FEAT-020.
- GH issue: <https://github.com/parisgroup-ai/graphify/issues/13>
