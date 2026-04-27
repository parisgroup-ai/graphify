---
uid: feat-047
status: open
priority: low
scheduled: 2026-04-26
timeEstimate: 30
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

# Rust consumer-side use_aliases canonicalization (named-import fan-out)

Rewrite consumer-side `use_aliases` entries so `use crate::Bar;` resolves to the canonical declaration module after FEAT-046 builds the `barrel_to_canonical` map. Closes the consumer-side fan-out gap.

## Description

Surfaced by FEAT-044 spike. After FEAT-046 lands, call-site rewrites work but the consumer's own `use crate::Bar;` Imports edge still targets the barrel (e.g. `src.Bar` rather than `src.foo.Bar`). The TS analogue is FEAT-026 (named-import canonicalization). For Rust the mechanism is subtler: the consumer's `use crate::Bar;` already populates `use_aliases` (FEAT-031), so we need an additional pass that rewrites alias TARGETS in `use_aliases_by_module` using FEAT-046's `barrel_to_canonical` map.

This means a consumer file with `use crate::Bar;` followed by `Bar::new()` will:
- Have its Imports edge target `src.foo.Bar` (this task)
- Have its Calls edge target `src.foo.Bar` (already covered by FEAT-046's edge rewrite)

## Subtasks

- [ ] After FEAT-046's `barrel_to_canonical` is built, iterate `use_aliases_by_module` and rewrite each entry whose target matches a `barrel_to_canonical` key
- [ ] Verify the rewrite happens BEFORE the resolver pass that consumes `use_aliases` (case 9 fallback in `resolve_with_depth`)
- [ ] Integration test: extends FEAT-046's fixture; asserts the Imports edge from `consumer.rs` (the `use crate::Bar;` itself, not the call) targets `src.foo.Bar`
- [ ] Verify `graphify suggest stubs` no longer surfaces `src.Community` for graphify-report (the dogfood acceptance criterion from the design doc, conditional on FEAT-045+046+047 all landing)

## Open risks

- `use_aliases` is per-file, not per-scope (BUG-025 trade-off). A file with both `use crate::Bar;` (re-export consumer) and `use other::Bar;` (shadowing import) will misroute one of them under last-write-wins. Defer until dogfood surfaces it; document in code comment.

## Out of scope

- Per-scope alias map refactor → v2; no current consumer
- Trait re-exports with method resolution → different problem, different feature

## Related

- Parent: FEAT-044 (spike + design)
- Depends on: FEAT-046
- Reference: TS FEAT-026 (named-import fan-out)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
