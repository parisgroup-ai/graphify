---
uid: chore-007
status: open
priority: high
scheduled: 2026-04-21
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- chore
- resolver
- audit
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# audit resolver branches for local_prefix and bound consistency

Stop the whack-a-mole pattern: BUG-001 (Python relative), BUG-007 and BUG-011 (TS workspace alias mangling), BUG-016 (Rust `crate::` drops `local_prefix`), BUG-017 (FEAT-031 alias rewrite unbounded → OOM) are all the same family. Every language-specific branch in `ModuleResolver::resolve` has two cross-cutting requirements that each new branch has re-discovered the hard way.

## Audit checklist (one pass, fail-fast)

For every branch in `ModuleResolver::resolve_with_depth` (cases 1–9 today — Python relative, per-module TS aliases, global TS aliases, TS relative, Go module path, Rust `crate::`/`super::`/`self::`, PHP `use`, direct name, Rust use-alias fallback) and any new branch added later:

1. **`local_prefix` application.** If the branch strips a language-specific root (Python dots, Rust `crate::`, TS aliases), does the final returned id re-prepend `self.local_prefix` via `apply_local_prefix`? Branches that resolve against `known_modules` directly don't need to — those ids already carry the prefix from walker registration. Branches that rebuild an id string from raw input absolutely do.
2. **Termination bound.** If the branch can recurse (only case 9 today, but TS alias-chasing could be next) does it carry a depth counter that decrements on every recursion and returns a finite non-local result when exhausted? Guard against the BUG-017 shape where a self-referential alias entry grows the rewritten string unbounded.
3. **Test covering the pathological shape.** For each branch, at least one resolver unit test exercising:
    - The legitimate happy path (`is_local=true` and canonical id).
    - The "stays external" path (no match → raw string passed through, `is_local=false`, bounded time).
    - At least one "tricky" shape specific to that language: Python `from .. import X` from a package `__init__.py` (BUG-001 regression guard), Rust `crate::X` with non-empty `local_prefix` (BUG-016 regression guard), TS workspace alias that traverses outside the project root (FEAT-027 v1 contract), etc.

## Deliverables

- One markdown table in this task body: branch | local_prefix-safe? | bound-safe? | tests covering (1)(2)(3) | findings.
- If the audit finds a branch failing any check, file a follow-up BUG with the failing shape and a one-test fix plan. Do NOT fix during the audit pass — the audit is diagnostic, the fixes ship individually with their own regression tests.
- If every branch passes, record the audit date in a top-level `## Audit log` section in `crates/graphify-extract/src/resolver.rs`'s module doc comment so future additions can reference the baseline without re-running the full pass.

## Out of scope

- New language support (add branches following the checklist; don't audit hypothetical ones).
- Algorithmic refactors of resolution strategy itself (e.g. moving from branch dispatch to a strategy pattern). That's FEAT territory.

## Estimated effort

~1h single strategic pass once all 9 branches are reviewed in sequence.

## Discovered context

Filed 2026-04-21 post-BUG-018 ship (v0.11.8). Motivation: the BUG-001 / BUG-007 / BUG-011 / BUG-016 / BUG-017 pattern is too clearly a family to keep surfacing one bug per session. Dedicated upfront pass over all branches catches the next BUG-020 before a user hits it.
