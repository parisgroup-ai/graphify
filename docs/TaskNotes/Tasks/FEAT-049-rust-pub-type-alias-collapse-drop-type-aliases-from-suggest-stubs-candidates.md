---
uid: feat-049
status: open
priority: low
scheduled: 2026-04-27
timeEstimate: 45
pomodoros: 0
designDoc: '[[docs/superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: false
---

# Rust pub type alias collapse — drop type aliases from suggest stubs candidates

Recognise `pub type X = Y;` declarations and either route them through the same canonical-collapse pipeline as `pub use`, or filter them out of `graphify suggest stubs` candidates entirely. Surfaced by FEAT-047's dogfood gap.

## Description

After FEAT-045/046/047 landed (Rust intra-crate `pub use` collapse), the dogfood baseline still surfaces `src.Cycle` as a `graphify-report` candidate with 4 edges. Investigation traces it to `pub type Cycle = Vec<String>;` in `graphify-core`, re-exported in `graphify-report/src/lib.rs`. The Rust extractor today does not handle `type_item` declarations as a re-export channel.

Two viable shapes for v1:

1. **Treat `pub type X = Y;` as a `ReExportEntry`** — emit `{from_module, raw_target: full_path_of_Y, line, specs: [{exported_name: "X", local_name: "X"}], is_star: false}`. Same pipeline as `pub use`, no new code paths. Caveat: tree-sitter `type_item` carries the alias target as a `type_identifier` or `scoped_type_identifier` — needs the same path-parsing as `process_scoped_use_list`.
2. **Filter `pub type` from `suggest stubs` only** — narrower scope; teach `score_stubs` to consult an "internally-aliased" set built from `pub type` declarations and skip those prefixes. Doesn't fix `graphify run` edge counts but closes the dogfood UX gap.

Pick option 1 if the underlying graph quality matters (consumers reading `analysis.json` see canonical paths); pick option 2 if `suggest stubs` UX is the only visible symptom. Default leaning: option 1 — same justification as FEAT-021/045 (data-quality fix beats reporting band-aid).

## Subtasks

- [ ] Spike: tree-sitter playground check — does `type_item` carry the target as scoped path? confirm node shape
- [ ] Decide between option 1 (full pipeline) and option 2 (suggest-stubs filter); document rationale in commit
- [ ] Implementation per chosen option
- [ ] Tests: 2-3 cases (simple `pub type X = Y;`, scoped `pub type X = mod::Y;`, generic `pub type X<T> = Y<T>;`)
- [ ] Dogfood verification: `src.Cycle` no longer surfaces in `graphify-report` candidates of `graphify suggest stubs`

## Out of scope

- Function pointer aliases (`pub type Handler = fn(&str);`) — different shape
- Trait aliases (`pub trait X = ...`) — unstable Rust, defer
- Cross-crate type aliases — same boundary as FEAT-048 (gated)

## Related

- Parent / context: FEAT-044 (Rust re-export collapse spike + design)
- Surfaced by: FEAT-047 (dogfood gap analysis at session close)
- Depends on: nothing (additive); benefits from FEAT-046 plumbing if option 1
- Reference: TS FEAT-021 / Rust FEAT-045

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
