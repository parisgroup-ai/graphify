---
uid: bug-021
status: open
priority: normal
scheduled: 2026-04-26
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- bug
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# F1: suggest stubs — already_covered_prefixes records too-broad prefix

`score_stubs` records `extract_prefix(target)` whenever `current_stubs.matches(target)`. But `ExternalStubs::matches` is a longest-prefix match with boundary chars, so a stub like `tokio::runtime` covers only `tokio::runtime::*`, not all of `tokio`. The relatório então mostra "tokio é coberto" quando só `tokio::runtime` está. Misleading default markdown output.

## Description

Surfaced by FEAT-043 final review. Either (a) record the matched stub itself (needs `ExternalStubs::matching_prefix(&str) -> Option<&str>`), or (b) rename the field `already_covered_via_prefixes` and document the asymmetry.

Recommended fix: (a) — add `matching_prefix` getter to `ExternalStubs`, plumb through `score_stubs`, update the report struct field semantics.

## Subtasks

- [ ] Add `ExternalStubs::matching_prefix(&str) -> Option<&str>`
- [ ] Update `score_stubs` to use `matching_prefix` for already_covered tracking
- [ ] Update unit test `score_stubs_records_already_covered_and_skips_them`
- [ ] Verify markdown output reads correctly on dogfood

## Related

- Spec: `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
- FEAT-043 task body section "Follow-ups" → F1

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
