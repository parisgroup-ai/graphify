---
uid: bug-021
status: done
priority: normal
scheduled: 2026-04-26
completed: 2026-04-26
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

# F1: suggest stubs — already_covered_prefixes records too-broad prefix

`score_stubs` records `extract_prefix(target)` whenever `current_stubs.matches(target)`. But `ExternalStubs::matches` is a longest-prefix match with boundary chars, so a stub like `tokio::runtime` covers only `tokio::runtime::*`, not all of `tokio`. The relatório então mostra "tokio é coberto" quando só `tokio::runtime` está. Misleading default markdown output.

## Description

Surfaced by FEAT-043 final review. Either (a) record the matched stub itself (needs `ExternalStubs::matching_prefix(&str) -> Option<&str>`), or (b) rename the field `already_covered_via_prefixes` and document the asymmetry.

Recommended fix: (a) — add `matching_prefix` getter to `ExternalStubs`, plumb through `score_stubs`, update the report struct field semantics.

## Subtasks

- [x] Add `ExternalStubs::matching_prefix(&str) -> Option<&str>` — landed in `crates/graphify-core/src/stubs.rs`. Returns the FIRST matching stub from the longest-first sorted list, so longest match wins automatically (no extra logic needed). Mirrors `matches()` shape; uses the same `prefix_matches` helper
- [x] Update `score_stubs` to use `matching_prefix` for already_covered tracking — replaced the `matches() + extract_prefix()` two-step with a single `matching_prefix()` call. Records the actual stub string instead of a normalized top-segment that could misrepresent which slice of the namespace is guarded
- [x] Update unit test `score_stubs_records_already_covered_and_skips_them` — no change needed; the existing test uses `["tokio"]` stub against `tokio::spawn` target, where the matched stub IS the top segment (`"tokio"`), so behavior is identical for that input. Test still passes
- [x] Verify markdown output reads correctly on dogfood — `graphify suggest stubs --format md` on this repo produces identical output (graphify-self has no sub-namespace stubs, so the change is unobservable in current dogfood; the fix is defensive against a class of future configs)

## Resolution

Added `ExternalStubs::matching_prefix(&self, target: &str) -> Option<&str>` to `crates/graphify-core/src/stubs.rs`. Implementation: `self.prefixes.iter().find(|p| prefix_matches(p, target)).map(String::as_str)`. Because `ExternalStubs::new` sorts `prefixes` by descending length, the first hit IS the longest match — no separate ranking pass needed. Returns `None` when no stub matches (mirrors the existing `matches()` semantics).

Updated `crates/graphify-report/src/suggest.rs::score_stubs`: the per-link "already covered" branch was a `matches() + extract_prefix(target, lang)` two-step. The first call answered "is this target covered?", the second computed the top-level segment to record in `already_covered_prefixes`. Replaced by a single `matching_prefix()` call that returns the actual stub string. Recording the stub itself eliminates the misreporting (a `tokio::runtime` stub no longer falsely surfaces as "tokio is covered").

Tests added:

1. `bug_021_matching_prefix_returns_longest_match` (in `stubs.rs`) — `["tokio", "tokio::runtime"]` registered, target `tokio::runtime::Builder` returns `Some("tokio::runtime")`, target `tokio::spawn` returns `Some("tokio")`. Confirms longest-first sort guarantee.
2. `bug_021_matching_prefix_returns_only_what_actually_matched` (in `stubs.rs`) — direct regression guard: `["tokio::runtime"]` only, target `tokio::runtime::Builder` returns `Some("tokio::runtime")`, target `tokio::spawn` returns `None`. Locks in the asymmetry the bug body called out.
3. `bug_021_matching_prefix_none_when_no_match` (in `stubs.rs`) — sanity: non-matching target returns None, no false positive on substring (`standard` not matched by `std`).
4. `bug_021_already_covered_records_actual_stub_not_top_segment` (in `suggest.rs`) — integration: `score_stubs` with `["tokio::runtime"]` stub + `tokio::runtime::Builder` link produces `already_covered_prefixes == ["tokio::runtime"]`, NOT `["tokio"]`. Pre-fix this would have asserted the buggy `["tokio"]`.

Field name `already_covered_prefixes` left unchanged. With the fix it's now exactly accurate ("list of stub prefixes that already cover the link in question") — the rename to `already_covered_via_prefixes` proposed as option (b) in the task body would have been documenting the bug rather than fixing it.

CI gates: 860 tests pass (was 856 — +3 stubs.rs + +1 suggest.rs), `cargo fmt --check` silent, `cargo clippy --workspace -D warnings` clean, `graphify check` PASS on all 5 crates with 0 cycles. graphify-core ticked +1 node / +1 edge from the new method.

Dogfood: identical markdown output on this repo (no sub-namespace stubs registered — every entry in `[settings].external_stubs` is a top-level prefix like `std`, `serde`, `Vec`). The fix is defensive against a class of configs that could have produced misleading reports; not a behavior change against current configs.

## Related

- Spec: `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
- FEAT-043 task body section "Follow-ups" → F1
- Lands cleanly on top of CHORE-011 (`edda9e6`) — `ExternalStubs` now in `graphify-core`, so the new method is co-located with the existing matcher API

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
