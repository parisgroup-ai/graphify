---
uid: bug-022
status: done
priority: high
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

# F4: graphify resolver misclassifies first-party symbols as external

Running `graphify suggest stubs` on the graphify repo itself surfaced ~30 candidates that are NOT external dependencies — they are first-party symbols (modules, types, functions) inside the workspace that the resolver is marking `is_local=false`. The hypothesis "single category of bug affecting all ~30" was disconfirmed by investigation: the symptoms come from at least 4 distinct bugs at different layers. The resolver part is fixed in this task; the others are filed as BUG-023, BUG-024, FEAT-044.

## Resolution summary (2026-04-26)

**Root cause (resolver):** Case 8.5 (BUG-019) only synthesized `{from_module}.{raw}` for *bare* identifiers. Two intra-crate scoped patterns fell through to "no match → external":

1. **Same-file `Type::method`** — `PolicyError::new(...)` from inside `policy.rs`. No `use` clause for a same-file type, so `use_aliases` has no entry; case 9 (FEAT-031) misses.
2. **Sibling-mod from crate root** — `pub use walker::{DiscoveredFile, ...};` in `lib.rs`. Tree-sitter emits scoped Imports targets without a `crate::` prefix; case 6 doesn't fire and no alias is registered for `walker` itself.

**Fix:** Added case 8.6 — when raw is Rust-shaped scoped (`Foo::bar`, `Foo::Bar::baz`, …) and `{from_module}.{raw with :: → .}` is a registered local module, promote to that qualified id at confidence 1.0. Ordered before case 9 so a same-module symbol shadows any aliased import (Rust resolution semantics; safer hotspot-scoring default). 5 new resolver tests; full workspace test suite green.

**Dogfood result:** `graphify suggest stubs` candidate list dropped 35 → 18 (49% reduction). 17 prefixes that disappeared are exactly the resolver-misclassified first-party symbols: `PolicyError`, `GlobMatcher`, `ExplainPalette`, `walker::*`, `lang::*`, `reexport_graph::*`, `ts_contract::*`, `workspace_reexport::*`, `drizzle::*`, `check_report::*`, `contract_json::*`, `json::*`, `diff_markdown::*`, `install::*`, `manifest::*`, `session::*`, `codex_bridge::*`.

**Out of scope (filed as separate tasks):**

- BUG-023 (Cat 3): nested `scoped_use_list` in extractor preserves `{a, b}` literal text — `ExtractionCache`, `Item`, `Array`, `Value` candidates remain
- BUG-024 (Cat 4): closures and let-bindings emitted as Calls — `pct`, `sort_key`, `threshold`, `write_grouped`, `find_sccs`, `sha256_hex`, `matches`, `env` candidates remain
- FEAT-044 (Cat 5): Rust re-export canonical-collapse missing — `src.Community`, `src.Cycle` candidates remain

## Subtasks

- [x] Pick `pct` as the canary case; reproduce by running `graphify run` and inspecting `graph.json`
- [x] Trace through resolver to find where each candidate lands
- [x] Identify root cause — concluded it's at least 4 distinct bugs, not one
- [x] Fix resolver case 8.6 + add regression tests (5 tests)
- [x] Re-run `graphify suggest stubs` on this repo, observe 35 → 18 candidate reduction
- [x] File follow-up tasks for the 3 non-resolver bugs (BUG-023, BUG-024, FEAT-044)

## Related

- FEAT-043 task body section "Follow-ups" → F4
- Related background: FEAT-031, BUG-019 (CLAUDE.md gotcha section)
- Follow-ups: BUG-023, BUG-024, FEAT-044

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
