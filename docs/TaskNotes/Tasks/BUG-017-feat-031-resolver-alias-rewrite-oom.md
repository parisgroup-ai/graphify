---
uid: bug-017
status: done
priority: high
scheduled: 2026-04-21
completed: 2026-04-21
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- bug
- rust
- extractor
- resolver
- feat-031-regression
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# BUG: FEAT-031 resolver alias-rewrite recursion OOMs on real-world Rust code

FEAT-031 v0.11.4 shipped a case-9 resolver fallback that recurses via `self.resolve(&rewritten, ...)` on every alias hit, without any bound on rewrite depth or cycle detection. On files with enough `use` declarations registered, certain alias shapes trigger unbounded string growth inside repeated `format!()` calls — the rewritten string gets longer on every recursion, allocating ~17 GB RSS in the first 10 s of extraction before the kernel SIGKILLs the process.

Discovered during FEAT-032 dogfood on 2026-04-21: graphify-cli and graphify-mcp silently failed to complete in every v0.11.4 pipeline run (exit 137 / SIGKILL). The session-close reports that referenced them were reading yesterday's stale `report/` files, not the current run.

## Repro

Against v0.11.4 on graphify's own `crates/graphify-cli/src/`:

```bash
./target/release/graphify extract --config graphify.toml --force
# [graphify-cli] Auto-detected local_prefix: src
# → RSS climbs to 17 GB within 10 s, then SIGKILL at ~60 s
```

The same extraction on the smaller crates (graphify-core, graphify-extract, graphify-report) completes normally — the bug needs enough use declarations to surface.

## Root cause

`crates/graphify-extract/src/resolver.rs:337-351` (added by FEAT-031):

```rust
if let Some(aliases) = self.use_aliases_by_module.get(from_module) {
    if let Some(full) = aliases.get(raw) {
        let full = full.clone();
        return self.resolve(&full, from_module, is_package);  // unbounded
    }
    if let Some((root, tail)) = raw.split_once("::") {
        if let Some(full) = aliases.get(root) {
            let rewritten = format!("{}::{}", full, tail);   // grows each iter
            return self.resolve(&rewritten, from_module, is_package);
        }
    }
}
```

Hazard shapes in `use_aliases`:

1. **Self-referential bare**: `("X", "X")` from `use X;` (single-segment) — recursing with identical arguments.
2. **Self-amplifying scoped**: `("X", "X::Y")` — each recursion re-prepends `X::Y::` and re-enters case 9's scoped branch, making the string longer forever.

Either shape in a file with a scoped call whose root matches the alias key triggers the runaway.

## Fix

Two changes in `crates/graphify-extract/src/resolver.rs`:

1. Add `const MAX_ALIAS_REWRITE_DEPTH: u8 = 4` and a private `resolve_with_depth` that carries a budget; public `resolve` delegates at full depth. Case 9 decrements on rewrite; at 0 the alias lookup is skipped so `resolve` falls through to the external-reference return.
2. Add a `full_starts_with_root(full, root)` guard that skips scoped aliases whose `full` already starts with `root` — blocks the amplifying-rewrite shape before it spends any depth budget. Also skip the `raw == full` bare-alias case.

Legitimate rewrites need exactly 1 hop (`Node::module` → `crate::types::Node::module` → case 6 `crate::` → canonical id). Depth 4 leaves headroom for any indirection we haven't seen yet without allowing the runaway.

## Test plan

- Unit regression guards in `resolver::tests::feat_031_use_alias_rewrite_is_bounded_against_self_referential_alias` + `feat_031_use_alias_bare_name_self_reference_is_bounded`. Both tests stack-overflowed against the pre-fix resolver (signal 6 / SIGABRT); post-fix they return finite results with `resolved.len() < 256` and `is_local=false`.
- Integration: re-run `graphify extract` against the full graphify-cli src — completes in <10 s with 354 nodes / 426 edges.
- Full self-dogfood completes end-to-end across all 5 crates.

## Acceptance criteria

- `cargo test --workspace` green (717 passing)
- `cargo clippy --workspace -- -D warnings` clean
- `cargo fmt --all -- --check` clean
- Self-dogfood `graphify run --config graphify.toml` completes for every project (was: 2/5 SIGKILL)
- v0.11.5 patch release tagged + pushed

## Resolution

Fixed in commit `607aee5` alongside FEAT-032 (they're same-commit because both the OOM fix and the `::` matcher extension land as bug fixes for FEAT-031's Rust scope). Released as v0.11.5.

## Discovered context

Discovered 2026-04-21 during the FEAT-032 dogfood rollout when isolated-extraction probing on graphify-cli showed 17 GB RSS at 10 s with exit 137. The T7 FEAT-031 dogfood comparison from earlier in the same session was stale-report-vs-stale-report for graphify-cli and graphify-mcp — we didn't catch the OOM until FEAT-032 forced a re-run. The session brief's `in_degree=1, betweenness=0` diagnostic signature from BUG-016 remains useful here: when a dogfood run "completes" but 2/5 crates' hotspot metrics haven't changed in 12+ hours, suspect a silent failure before declaring victory.
