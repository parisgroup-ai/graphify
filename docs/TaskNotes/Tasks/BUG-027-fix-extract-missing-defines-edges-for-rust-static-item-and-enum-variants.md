---
uid: bug-027
status: done
priority: normal
scheduled: 2026-04-27
completed: 2026-04-27
pomodoros: 0
tags:
- task
- bug
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: med
  hintsInferred: false
---

# fix(extract): missing `Defines` edges for Rust `static_item` and enum variants

`graphify suggest stubs` surfaces two distinct local symbols as external candidates because their canonical ids never enter `known_modules`: `src.install.copy_plan.INTEGRATIONS` (a `pub static` from `include_dir!`) and `Selector::Group`/`Selector::Project` (enum variants of `enum Selector` in `graphify-core/src/policy.rs`). Both trace to the Rust extractor emitting `Defines` only for fn/struct/enum/trait/impl/type item kinds, leaving sub-symbols invisible to BUG-018's seeding pass.

## Description

Two concrete misclassifications surfaced by the post-FEAT-049 dogfood run on this repo (2026-04-27):

**Symptom A — `static_item` not registered**
- `crates/graphify-cli/src/install/copy_plan.rs:8` — `pub static INTEGRATIONS: Dir<'_> = include_dir!(...)`
- Consumer: `crates/graphify-cli/src/install/codex_bridge.rs:5` — `use crate::install::copy_plan::INTEGRATIONS;` then `INTEGRATIONS.get_file(...)` at line 31
- Resolver case 9 (`use_aliases`) qualifies the bare `INTEGRATIONS` to `src.install.copy_plan.INTEGRATIONS`, but no `Defines` edge was ever emitted for the static item, so the canonical id is missing from `known_modules` and the edge lands on a non-local placeholder node. `is_local=false` makes it surface in `suggest stubs`.

**Symptom B — enum variants not registered**
- `crates/graphify-core/src/policy.rs:123` — `enum Selector { Project(...), Group(...) }`
- Variant constructors at lines 320, 329, 404, 411 — `Selector::Project(...)`, `Selector::Group(...)`
- Captured edges (verified via `report/graphify-core/graph.json`): target `Selector::Group` / `Selector::Project`, kind `Calls`, `confidence: 0.5`, `confidence_kind: Ambiguous`
- Resolver case 8.6 (BUG-022) synthesizes `src.policy.Selector.Group` from the bare scoped path and looks it up in `known_modules` — fails because the extractor emits `Defines` for the enum `Selector` but NOT for each variant. Falls through to use_aliases (no entry), downgrades to Ambiguous, lands non-local.

Both symptoms are the same shape as FEAT-049 (`pub type` aliases): items participate in scoped/qualified callsites but the extractor doesn't seed them into `known_modules` via `Defines`. FEAT-049 closed `type_item`. This task closes `static_item` and enum-variant emission.

## Reproduction

```bash
graphify run --config graphify.toml --force
graphify suggest stubs --config graphify.toml
```

Expected (post-fix):
- `src.install.copy_plan.INTEGRATIONS` not in candidate list
- `Selector` not in candidate list
- Total candidates: 1 (only `src.Community`, which is the FEAT-048 deferred gate signal)

## Hypothesis & approach

In `crates/graphify-extract/src/rust_lang.rs`:

1. **`static_item`** — add a top-level match arm (mirror FEAT-049's `type_item`) that emits a `Defines` edge `{module} → {module}.{name}` for `static FOO: T = ...`. Reuse `extract_named_type` if its signature accepts the kind, or add a sibling helper. Same shape for `const_item` (likely affected too even if not in the current candidate list — check before scoping out).

2. **Enum variants** — extend `extract_enum_item` to walk the `enum_variant_list` and emit one `Defines` edge per variant: `{module}.{EnumName} → {module}.{EnumName}.{VariantName}` (or the resolver-friendly shape — confirm what case 8.6 expects). Verify against `enum Selector { Project(String), Group(String) }` and the unit-variant case.

3. **NodeKind reuse** — per FEAT-049 precedent, reuse existing `NodeKind::Class` for static and enum variants rather than introducing new variants (same cascade-cost trade-off).

## Acceptance criteria

- `graphify suggest stubs --config graphify.toml` candidate count drops from 4 to 1 (only `src.Community` remains, deferred via ADR-0002)
- Unit tests in `crates/graphify-extract/src/rust_lang.rs`:
  - `pub static FOO: u32 = 1;` emits `Defines` edge `mod -> mod.FOO`
  - `enum E { A, B(u32) }` emits `Defines` for both `mod.E.A` and `mod.E.B`
  - Bare `E::A` callsite from same module resolves to canonical `mod.E.A` (case 8.6 round-trip)
- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass
- `graphify check --config graphify.toml` passes (no new cycles, no hotspot regressions)
- CLAUDE.md updated under the FEAT-049 bullet (or a new sibling bullet) documenting the closure of `static_item` + enum-variant `Defines` gaps

## Out of scope

- `const_item` if no candidate surfaces it (verify during implementation; if it exposes the same gap, fold in — it's the same fix shape)
- Tuple-struct field emission (`struct Foo(u32)` accessing `.0`) — not the same shape, not surfaced
- Cross-crate enum variant resolution — that's FEAT-048 territory (deferred)
- Generalizing `extract_named_type` into a kind-parametrized helper — refactor pass after both arms land, only if the duplication justifies it

## Related

- Surfaced from FEAT-043 self-dogfood + post-FEAT-049 stub-suggestion review
- Sibling: BUG-026 (Rust `std::env::*` prefix stripping) — different bug shape, both surfaced same review
- Precedent: FEAT-049 (Rust `pub type` alias collapse via Defines edge)
- Reference: BUG-018 (Defines-target seeding in `run_extract_with_workspace`), BUG-022 (resolver case 8.6 scoped-path synthesis)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
