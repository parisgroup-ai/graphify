---
uid: feat-016
status: done
completed: 2026-04-13
priority: high
timeEstimate: 960
tags:
  - task
  - feature
  - schema
  - types
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - schema
  - typescript
  - contracts
---

# Contract drift detection between ORM models and TypeScript types

## Description

Detect mismatches between ORM-backed data models and TypeScript contracts that are supposed to represent the same shape across API and application boundaries.

## Motivation

A generic "type comparison" engine would be too broad and brittle. The valuable use case is contract-oriented drift detection: database model vs DTO, ORM model vs Zod schema, or backend entity vs frontend-facing TypeScript interface.

## Proposed Outcome

Start with targeted comparisons such as:

1. Prisma or Drizzle model to DTO or interface
2. ORM model to Zod schema
3. backend entity to frontend contract type

The feature should highlight:

1. missing fields
2. type mismatches
3. nullability mismatches
4. relations present in one side but absent in the other
5. likely stale contracts after model evolution

## Likely Scope

- explicit contract pairing configuration
- structural comparison model
- normalized representation for supported ORM and TS contract sources
- CLI or report surface for drift findings
- fixtures for real-world mismatch cases

## Subtasks

- [x] Define the supported first-class sources and pairings
- [x] Decide how contract mapping is configured explicitly
- [x] Normalize field, nullability, and relation metadata for comparison
- [x] Implement mismatch reporting with useful diagnostics
- [x] Add fixtures for Prisma, Drizzle, and Zod-oriented cases if supported
- [x] Document recommended usage and limitations

## Notes

This should be scoped as contract drift detection, not a universal semantic type engine. The first design decision is whether the primary target is backend-to-API drift or backend-to-frontend drift.

## Verification (2026-04-13)

All 15 tasks in the FEAT-016 plan (`docs/superpowers/plans/2026-04-13-feat-016-contract-drift.md`) are complete.

### Commit history (branch `main`, pre-push)

Task 15 close-out is a single docs + version bump commit on top of the following FEAT-016 commits:

| Commit | Subject |
|---|---|
| `5ba289f` | chore(report): use is_empty() in test asserts to satisfy clippy 1.94 |
| `619c7b9` | feat(core): relation alignment and cardinality comparison (FEAT-016) |
| `955437d` | feat(extract): TS scalar vs relation classification (FEAT-016) |
| `4c55299` | feat(extract): Drizzle relations() block parser (FEAT-016) |
| `bc27889` | test(extract): coverage for inline TS intersections (FEAT-016) |
| `13662d1` | feat(core): deterministic ordering for contract violations (FEAT-016) |
| `c7e190c` | feat(report): contract check JSON schema (FEAT-016) |
| `5c799fb` | feat(report): contract drift Markdown section (FEAT-016) |
| `f2337a9` | feat(cli): wire contract drift gate into graphify check (FEAT-016) |

### Test count

- Prior baseline: 269 workspace tests
- After FEAT-016: 404 workspace tests (+135)
- Verified with `cargo test --workspace`

### CLI surface

- `graphify check` — runs contract drift gate automatically when `[[contract.pair]]` is declared in `graphify.toml`
- `--contracts` — force-enable (redundant with auto-detect)
- `--no-contracts` — opt-out even if pairs are declared
- `--contracts-warnings-as-errors` — promote warning-severity drift to hard failures

### Known limitations (deferred)

- Pair-level declaration `line` is hardcoded to `1` in the JSON output — v2 work (editor integration, FEAT-015)
- `target_contract` on the ORM side is parsed but not compared (advisory)
- Relation nullability comparison is deferred
- Prisma, Zod, and tRPC sources are not supported in v1
- No performance guardrail test (100-pair fixture from spec §9) — add when real-world usage appears

### Verification commands

- `cargo test --workspace` — all 404 tests green
- `cargo clippy --workspace --all-targets -- -D warnings` — per-crate clean on `graphify-core`, `graphify-extract`, `graphify-report`, `graphify-cli`; pre-existing workspace-level warnings in `graphify-mcp/tests/integration.rs` (`manual_flatten`) and `graphify-extract/src/python.rs:547` (`manual_contains`) are explicitly out of FEAT-016 scope and unchanged by this work.
