---
uid: feat-032
status: done
priority: normal
scheduled: 2026-04-21
completed: 2026-04-21
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
- rust
- resolver
- external-stubs
- bug-fix
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# FEAT: `external_stubs` matcher supports Rust `::` separator

`[[project]].external_stubs` was introduced in issue #12 to let consumers classify known-external targets (npm packages, Python modules) as `ConfidenceKind::ExpectedExternal` so the ambiguity metric reflects only extractor gaps. The matcher at `crates/graphify-extract/src/stubs.rs:48-56` uses `/` and `.` as boundary characters — correct for npm (`drizzle-orm/pg-core`) and Python-ish (`drizzle-orm.eq`), but **wrong for Rust** where path segments join with `::`.

Post-FEAT-031 the Rust extractor now emits scoped-call targets like `std::collections::HashMap::new`, `serde::Serialize`, `tree_sitter::Parser`. A `std` stub never matched these because `::` wasn't a recognised boundary. `external_stubs` was effectively inoperative for every Rust project using the feature.

This task adds `::` as a third boundary character and populates the dogfood `graphify.toml` with per-crate stub lists to exercise the new matcher end-to-end.

## Design

One-line fix in `prefix_matches` — add `rest.starts_with("::")` to the boundary-recognition expression. Scoped-npm, dot-suffix, and exact-match behaviour preserved; no stub-format change; no public API change.

Coupled with the per-crate stub lists in `graphify.toml`, covering:

- `std` — Rust standard library namespace
- Rust prelude: `Vec`, `String`, `Box`, `Option`, `Result`, `Some`, `None`, `Ok`, `Err`, `Self`
- Prelude macros (captured as bare names after FEAT-031 strips `!`): `format`, `writeln`, `println`, `eprintln`, `print`, `eprint`, `vec`, `write`, `assert`/`assert_eq`/`assert_ne`, `debug_assert*`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg`
- Per-crate dependencies from each crate's `Cargo.toml` (petgraph, serde*, tree_sitter*, clap, toml, rayon, rmcp, tokio, …)

Cross-crate workspace references (`graphify_core::*`, `graphify_extract::*`) are **intentionally not stubbed** — they represent real inter-crate dependencies and should remain visible in the architecture view.

## Test plan

- `stubs::tests::feat_032_rust_std_prefix_matches_scoped_target` — `std` stub matches `std`, `std::collections::HashMap`, `std::fs::write`, etc.
- `stubs::tests::feat_032_rust_bare_prelude_exact_match_still_works` — `Vec` matches both bare `Vec` and scoped `Vec::new`.
- `stubs::tests::feat_032_rust_crate_prefix_does_not_leak_into_similar_names` — `std` must NOT match `standard`, `stdx::foo`.
- `stubs::tests::feat_032_rust_stub_coexists_with_legacy_slash_dot_boundaries` — existing npm/Python shapes still match.
- Dogfood: full self-analysis reclassifies 472 edges from `Ambiguous` to `ExpectedExternal` across the 5 graphify crates.

## Acceptance criteria

- `cargo test --workspace` green
- Dogfood `graphify run --config graphify.toml` shows `ExpectedExternal` > 0 for all 5 crates (was 0 for graphify-cli and graphify-mcp pre-fix because those never completed, plus 0 for every Rust project on any machine because of the missing `::` boundary)
- v0.11.5 patch release tagged + pushed

## Out of scope (follow-ups filed)

- **FEAT-033**: deprioritize `ExpectedExternal` edges in hotspot scoring so local symbols actually rise to the top of `graphify query`/`explain` lists. `external_stubs` was never about scoring — only classification — but the practical "readable hotspots" outcome needs the scoring change too.
- **FEAT-034**: `[settings].external_stubs` merge layer so Rust workspaces don't have to duplicate the same ~30 prelude stubs per `[[project]]`. Today the dogfood `graphify.toml` repeats the prelude list 5×.

## Resolution

Implemented in commit `607aee5` alongside BUG-017 (co-shipped because the BUG-017 OOM fix was blocking any Rust dogfood that would exercise this matcher). Released as v0.11.5.
