---
uid: chore-011
status: done
priority: normal
scheduled: 2026-04-26
completed: 2026-04-26
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

# F3: move ExternalStubs from graphify-extract to graphify-core

FEAT-043 introduced a `graphify-report → graphify-extract` dependency edge solely to import `ExternalStubs` (a 30-line prefix matcher). `graphify-report` was previously pure-renderer with no extractor coupling; this layer crossing should be cleaned up by moving `ExternalStubs` to `graphify-core`.

## Description

`ExternalStubs` is currently in `crates/graphify-extract/src/stubs.rs`. The struct has no extractor logic — pure prefix matching with `Vec<String>` state. Moving to `graphify-core` restores the dep invariant (report depends only on core; extract depends on core; cli depends on report+extract+core; mcp depends on extract+core).

**Public API change**: import path moves from `graphify_extract::stubs::ExternalStubs` to `graphify_core::stubs::ExternalStubs`. Affects ~6 import sites across the workspace.

## Subtasks

- [x] Create `crates/graphify-core/src/stubs.rs` with `ExternalStubs` (move file + tests) — `cp` from extract; identical content (no changes needed since the matcher had no extractor coupling, just `Vec<String>` and string slicing)
- [x] Add `pub mod stubs;` to `graphify-core/src/lib.rs`
- [x] Re-export from `graphify-core` if convenient — added `pub use stubs::ExternalStubs;` for convenience parity with the old `graphify_extract::ExternalStubs` shape
- [x] Update import sites in `graphify-extract`, `graphify-report`, `graphify-cli`, `graphify-mcp` — 13 sites total: 1 in cli (one inner-path use site at line 5290 + the top-of-file re-import block), 1 in mcp (top-of-file import block), 11 in report/suggest.rs (1 struct field type + 10 test fns)
- [x] Drop `graphify-extract = { workspace = true }` from `crates/graphify-report/Cargo.toml` — `cargo build --workspace` confirms report compiles without the dep
- [x] (Optionally) drop the dep from `graphify-extract/src/stubs.rs` re-export if desired — went all-in: deleted `graphify-extract/src/stubs.rs` entirely + removed `pub mod stubs;` and `pub use stubs::ExternalStubs;` from extract's lib.rs. No backward-compat re-export kept; consumers now import directly from `graphify_core`. Internal-only API, no external SDK contract to preserve
- [x] Run full CI: `cargo fmt --check`, `cargo clippy --workspace -D warnings`, `cargo test --workspace`, `graphify check` — all four green; 856 tests pass (unchanged total — tests moved with the file), zero clippy warnings, all 5 crates PASS the architectural check with 0 cycles

## Resolution

Mechanical move with one consolidation choice. The file `crates/graphify-extract/src/stubs.rs` (220 lines, 14 tests, all pure-string matching) moved verbatim to `crates/graphify-core/src/stubs.rs`. `graphify-core/src/lib.rs` gained `pub mod stubs;` (alphabetically between `query` and `types`) plus a `pub use stubs::ExternalStubs;` convenience re-export at the bottom. Old extract location deleted entirely — no shim, no re-export, no deprecation alias. Internal workspace crate, no published SDK to preserve.

Import-site changes:

- `graphify-cli/src/main.rs`: `ExternalStubs` moved from the `graphify_extract::{...}` import block to the `graphify_core::{...}` block at the top of the file. One inner-path use site (`use graphify_extract::stubs::ExternalStubs;` at line 5290 inside a function-scoped block) rewritten to `use graphify_core::ExternalStubs;` — coincidentally exercises the BUG-025 fix that just landed in the previous commit (function-scoped use_declaration walking).
- `graphify-mcp/src/main.rs`: same shape — `ExternalStubs` migrated from extract import block to core import block.
- `graphify-report/src/suggest.rs`: 11 sites of `graphify_extract::stubs::ExternalStubs` rewritten to `graphify_core::ExternalStubs` via `replace_all` (1 struct field type in `ProjectInput`, 10 `use` statements inside test fns).
- `graphify-report/Cargo.toml`: dropped `graphify-extract = { workspace = true }` from `[dependencies]`. This was the layer-crossing FEAT-043 introduced and that this CHORE was filed to retire.

Architecture impact (`graphify check` PASS on all 5):

- `graphify-core`: 285 → 291 nodes (+6), 439 → 448 edges (+9), max_hotspot 0.486 → 0.472 (slight dilution from added nodes lowering policy's relative score). Communities unchanged at 10.
- `graphify-extract`: 295 → 288 nodes (-7), 601 → 592 edges (-9). max_hotspot identical (0.435 src.resolver). Net counts add up to the 220-line stubs file's contribution.
- `graphify-report`, `graphify-cli`, `graphify-mcp`: essentially unchanged. graphify-cli ticked -1 node / -1 edge from one fewer cross-crate import; report and mcp identical.

Cross-project edges: `graphify-summary.json` still surfaces a `graphify-report → graphify-extract` row with 81 edges. This is an analyzer artifact — graphify's cross-project edge counting is name-based (modules with overlapping ids across projects), not Cargo-dep-direction-based. Confirmed via `grep -rn "graphify_extract\|graphify-extract" crates/graphify-report/` returning zero hits and `cargo build --workspace` succeeding without the dep. The architectural win (broken layer crossing) is real even if the heuristic still surfaces nominal edges. Tracked as a minor analyzer-precision concern, not a blocker.

Self-dogfood: `graphify suggest stubs` candidate count holds at 7 — zero regression. Same per-project breakdown as post-BUG-025.

CI gates: all four green (`cargo fmt --all -- --check` silent, `cargo clippy --workspace -- -D warnings` clean, `cargo test --workspace` 856/0, `graphify check` PASS).

## Related

- Spec: `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
- FEAT-043 task body section "Follow-ups" → F3
- Reviewer flag: code-quality review of Task 2 (commit 350b8bb) + final review of Task 13
- Lands cleanly on top of BUG-025 (`c805b48`) — the function-scoped use_declaration fix that was prerequisite for the cli's `use graphify_core::ExternalStubs;` inside `apply_suggestions` to be visible to the extractor when this dogfood loop is closed

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
