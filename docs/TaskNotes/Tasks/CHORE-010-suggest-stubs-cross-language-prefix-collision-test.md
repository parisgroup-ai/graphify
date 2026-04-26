---
uid: chore-010
status: open
priority: low
scheduled: 2026-04-26
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# F2: suggest stubs — cross-language same-prefix collision test gap

`score_stubs` aggregates by `(project, prefix)` — different languages emitting the same prefix string (e.g. Go module id `serde` and Rust crate id `serde::Deserialize`) collapse into one candidate with the language of whichever hit landed first via HashMap iteration.

## Description

Surfaced by FEAT-043 final review. Realistically rare but currently undefined. 30-line unit test pinning the behavior would be enough; if a real workload hits it, decide later whether to group by `(prefix, language)` instead.

## Subtasks

- [ ] Add `score_stubs_handles_mixed_language_same_prefix` unit test in `crates/graphify-report/src/suggest.rs`
- [ ] Document the current behavior (first-hit wins) in the doc comment of `StubCandidate.language`
- [ ] (optional) decide whether to switch grouping key to `(prefix, language)` if the test surprises

## Related

- Spec: `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
- FEAT-043 task body section "Follow-ups" → F2

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
