---
uid: chore-007
status: done
priority: high
scheduled: 2026-04-21
completed: 2026-04-21
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

## Audit result — 2026-04-21 (post-BUG-019 @ v0.11.9)

**Scope:** 10 branches in `ModuleResolver::resolve_with_depth` (cases 1, 2, 3, 4, 5, 6a `crate::`, 6b `super::`/`self::`, 7, 8, 8.5, 9) plus the helper path through `canonicalize_known_module`.

| # | Branch | local_prefix-safe? | bound-safe? | Tests | Findings |
|---|---|---|---|---|---|
| 1 | Python relative (`.`, `..`, …) | ✓ uses `from_module.split('.')` which carries prefix from walker registration | ✓ non-recursive single-pass | ✓ 9 tests: single/double/bare dot, from `__init__.py` × 3, no-false-walk (BUG-001 regression), confidence | — |
| 2 | Per-module TS aliases (`@/lib/api`) | ✓ happy path via `lookup_module_by_path` returns a registered (prefix-ed) id; fallback `path_to_dot_notation` returns a non-prefix id only when the path is inside `self.root` but NOT in `known_modules` — then `is_local=false` anyway so prefix moot | ✓ non-recursive | ✓ covered by FEAT-028 workspace alias test suite + `apply_ts_alias_workspace_*` × 5 | — |
| 3 | Global TS aliases | ✓ same path-lookup pattern as case 2 | ✓ non-recursive | ✓ (path-free legacy mode, exercised indirectly) | Legacy mode — kept for backward compat, low traffic |
| 4 | TS relative (`./foo`, `../bar`) | ✓ uses `from_module.split('.')` prefix-aware | ✓ non-recursive | ✓ 8 tests: same_dir, parent, strips js/tsx/mjs, no_suffix, unknown_extension, confidence | `is_package` honoured symmetric with Python (BUG-001 shape) |
| 5 | Go module path | ✓ `canonicalize_known_module` consults registered prefix-ed ids; non-local fallback preserves the raw relative string (correct external behaviour) | ✓ non-recursive | ✓ 5 tests: local, nested, external, third-party, `load_go_mod` | `canonicalize_known_module` does an O(N) suffix-match across `known_modules` — silently returns `None` on ambiguity to avoid wrong attribution; acceptable trade-off |
| 6a | Rust `crate::` | ✓ explicit `apply_local_prefix` after stripping `crate::` (BUG-016 fix) | ✓ non-recursive | ✓ 4 tests: crate, nested, **smoking-gun BUG-016**, **no_prefix regression guard** | — |
| 6b | Rust `super::` / `self::` | ✓ uses `from_module.split('.')` prefix-aware | ✓ non-recursive | ✓ 3 tests: super, self, super::super | — |
| 7 | PHP `\` namespace targets | ✓ PSR-4 walker registers namespace-prefixed ids; resolver normalizes `\` → `.` and looks up (matches) | ✓ non-recursive | ✓ 3 tests: matches_known, non_local_extracted_confidence, strips_leading_backslash | **LANDMINE** — PHP projects with an explicit `[[project]].local_prefix` in `graphify.toml` would conflict with PSR-4 namespace prefixes. Not a bug (PHP projects shouldn't set `local_prefix` — PSR-4 provides the namespace structure) but worth documenting. Filed as DOC-002. |
| 8 | Direct `known_modules` lookup | N/A (raw passthrough; no rebuild from pieces) | ✓ non-recursive | ✓ exercised throughout the suite | — |
| 8.5 | Bare-identifier same-module (BUG-019) | ✓ synthesizes `{from_module}.{raw}` where `from_module` is prefix-ed | ✓ non-recursive, one HashMap lookup | ✓ 4 tests: happy path, external, scoped-shape guard, empty-from_module guard | — |
| 9 | Rust `use`-alias fallback (FEAT-031) | ✓ recurses into case 6a which applies prefix | ✓ `MAX_ALIAS_REWRITE_DEPTH = 4` + filters for `(X, X)` and `(X, X::Y)` self-references | ✓ 5 tests: scoped, bare-fn, no-match-stays-external, unknown-tail, per-module-scope + **2 BUG-017 OOM regression guards** (pathological alias bounded, bare-name self-reference bounded) | — |

**Conclusion:** all 10 branches satisfy both invariants and have adequate test coverage for the happy path, the external-stays-external path, and at least one language-specific tricky shape. **No follow-up BUGs filed.** One documentation task filed (DOC-002 — PHP projects and `local_prefix`).

## Follow-ups filed

- **DOC-002**: Document that PHP projects should leave `local_prefix` unset; PSR-4 namespace mapping handles module-prefix structure. Low priority.

## Audit-log entry

Added to `crates/graphify-extract/src/resolver.rs` module doc-comment (`//!`) as `## Audit log` table so future contributors can see the baseline without re-running the pass.
