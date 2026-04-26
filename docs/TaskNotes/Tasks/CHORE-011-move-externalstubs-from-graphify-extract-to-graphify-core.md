---
uid: chore-011
status: open
priority: normal
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

# F3: move ExternalStubs from graphify-extract to graphify-core

FEAT-043 introduced a `graphify-report → graphify-extract` dependency edge solely to import `ExternalStubs` (a 30-line prefix matcher). `graphify-report` was previously pure-renderer with no extractor coupling; this layer crossing should be cleaned up by moving `ExternalStubs` to `graphify-core`.

## Description

`ExternalStubs` is currently in `crates/graphify-extract/src/stubs.rs`. The struct has no extractor logic — pure prefix matching with `Vec<String>` state. Moving to `graphify-core` restores the dep invariant (report depends only on core; extract depends on core; cli depends on report+extract+core; mcp depends on extract+core).

**Public API change**: import path moves from `graphify_extract::stubs::ExternalStubs` to `graphify_core::stubs::ExternalStubs`. Affects ~6 import sites across the workspace.

## Subtasks

- [ ] Create `crates/graphify-core/src/stubs.rs` with `ExternalStubs` (move file + tests)
- [ ] Add `pub mod stubs;` to `graphify-core/src/lib.rs`
- [ ] Re-export from `graphify-core` if convenient (e.g. `pub use stubs::ExternalStubs;`)
- [ ] Update import sites in `graphify-extract`, `graphify-report`, `graphify-cli`, `graphify-mcp`
- [ ] Drop `graphify-extract = { workspace = true }` from `crates/graphify-report/Cargo.toml` (no longer needed)
- [ ] (Optionally) drop the dep from `graphify-extract/src/stubs.rs` re-export if desired
- [ ] Run full CI: `cargo fmt --check`, `cargo clippy --workspace -D warnings`, `cargo test --workspace`, `graphify check`

## Related

- Spec: `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
- FEAT-043 task body section "Follow-ups" → F3
- Reviewer flag: code-quality review of Task 2 (commit 350b8bb) + final review of Task 13

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
