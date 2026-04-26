# Session Brief — 2026-04-26 (BUG-022 resolver fix)

## Last Session Summary

Investigated BUG-022 (graphify resolver misclassifying first-party symbols as external) end-to-end via systematic-debugging + TDD. Root-cause investigation disconfirmed the original "single bug affecting all ~30 cases" hypothesis: the candidate list traces to **at least 4 distinct bugs at different layers**. Fixed the resolver part (case 8.6) and filed the other three as follow-up tasks. Self-dogfood `graphify suggest stubs` candidate list dropped 35 → 18 (49% reduction), exactly the 17 prefixes predicted as Cat 1+2 (same-file `Type::method` + sibling-mod-from-crate-root).

## Current State

- Branch: `main`, latest commit `e1f83c7 docs(tasks): close BUG-022, file BUG-023/BUG-024/FEAT-044, CHANGELOG`
- Working tree: clean (memory-bank updates pending in this close commit)
- Origin sync: 0 ahead / 0 behind (pushed mid-session at `e1f83c7`)
- Latest release: `v0.13.1` — `Unreleased` CHANGELOG section now has FEAT-043 + BUG-022 entries, ready for `v0.13.2` whenever bumped
- Local installed binary: `graphify 0.13.1` at `~/.cargo/bin/graphify` — refreshed this session via `cargo install --path crates/graphify-cli --force` from the canonical `~/www/pg/apps/graphify` source (was previously pinned to a stale `~/ai/graphify` path)
- TaskNotes: 73 total / 6 open / 0 in-progress / 67 done (after this session: BUG-022 closed, BUG-023/BUG-024/FEAT-044 opened)

## Commits This Session

`fa8bbaf..e1f83c7` (2 commits, both pushed):

- `0f46ec7` fix(resolver): scoped same-module + sibling-mod paths resolve as local (BUG-022)
- `e1f83c7` docs(tasks): close BUG-022, file BUG-023/BUG-024/FEAT-044, CHANGELOG

## Decisions Made (don't re-debate)

- **BUG-022 is multi-layer, not single-bug**: Phase 1 root-cause investigation showed the ~30 misclassifications come from 4 distinct layers — resolver (Cat 1+2), extractor `scoped_use_list` parsing (Cat 3), extractor closure handling (Cat 4), and missing Rust re-export collapse (Cat 5). Trying to fix all of them in one session would have been scope creep; instead followed the "minimum-impactful-fix + file the rest" pattern. Resolver fix lands ~50% reduction; the rest are tracked as BUG-023, BUG-024, FEAT-044
- **Case 8.6 placement before case 9 is load-bearing**: same-module symbols shadow aliased imports. Rust resolution semantics back this up (compile error if both in scope). The shadow-alias test (`bug_022_scoped_local_match_shadows_alias`) is the regression guard — proves case 9 was producing wrong results before the fix
- **Synthesis uses `::` → `.` normalization**: lets the existing BUG-019 negative-guard test (`bug_019_scoped_call_skips_bare_synthesis`) keep passing because its pathological plant has literal `::`, which doesn't match the normalized `.` synthesis — keeping the test was the cheapest way to preserve the original BUG-019 invariant under the new logic
- **Skipped Cat 4 even though task body mentioned `pct` as canary**: `pct` is a closure (not extracted as Defines), so the resolver fix can't help it; closure handling needs scope analysis, which is BUG-024's territory. Picked a more representative canary (`PolicyError::new`, then `walker::DiscoveredFile`) to drive the fix
- **Auto-push fast-forward gate worked smoothly again**: pushed `fa8bbaf..e1f83c7` mid-session, no race with sibling instance because none was active. Session-close push step is a no-op (already in sync)

## Architectural Health

`graphify check --config graphify.toml` — all 5 projects PASS:

- `graphify-core`: PASS, 0 cycles, max_hotspot 0.487 (`src.policy`) — unchanged
- `graphify-extract`: PASS, 0 cycles, max_hotspot 0.435 (`src.resolver`) — slight drop from 0.439 last session as case 8.6 promoted more edges to local
- `graphify-report`: PASS, 0 cycles, max_hotspot 0.454 (`src.pr_summary`) — unchanged
- `graphify-cli`: PASS, 0 cycles, max_hotspot 0.452 (`src.install`) — drop from 0.469 last session, same reason
- `graphify-mcp`: PASS, 0 cycles, max_hotspot 0.600 (`src.server`) — unchanged
- Policy violations: 0
- All hotspots well under the 0.85 CI threshold

Workspace tests: 758+ pass, 0 fail (5 new `bug_022_*` resolver tests added).

## Open Items (3 follow-up tasks filed, all pushed)

- **BUG-023** (normal): rust extractor preserves nested `use a::{b::{c, d}}` as literal text including braces. Fixes `ExtractionCache`, `Item`, `Array`, `Value` candidates. Scope: ~30 LOC + 2 tests. Most impactful next bug
- **BUG-024** (normal): rust extractor emits Calls edges for closures and let-bindings. Fixes `pct`, `sort_key`, `threshold`, `write_grouped`, `find_sccs`, `sha256_hex`, `matches`, `env`. Needs scope analysis — option 1 (pre-scan let-statements) vs option 2 (post-resolution filter); decide before writing tests. Higher uncertainty
- **FEAT-044** (low): Rust re-export canonical-collapse, mirrors TS FEAT-021/025/026/028 architecture. Multi-day. Open question whether the volume justifies the effort

## Suggested Next Steps

1. **BUG-023** is the next clean win — same shape as BUG-022 (well-bounded, low-risk extractor patch, drops 4-5 visible candidates). Likely 30-45 min with TDD
2. **`v0.13.2` release** — both FEAT-043 and BUG-022 entries are queued in `[Unreleased]`. Worth bumping after either BUG-023 lands (groups three resolver/extractor improvements into one release) OR shipping now if it's been a while
3. **BUG-024** (closures) — higher uncertainty. Requires deciding between pre-scan vs post-resolution-filter; would benefit from a brainstorm pass before TDD
4. (long-tail) FEAT-044 — only worth picking up if Rust re-export volume becomes user-visible; currently 8 edges total

## Reminders

- `.claude/session-context-gf.json` is `skip-worktree`'d locally — `git ls-files -v` shows `S`. Reverse with `git update-index --no-skip-worktree`
- 17 local-only skills under `~/.claude/skills/` not in upstream cache (same as last session). Silence per-skill with `.skills-sync-ignore` or publish via `/share-skill <name>`
- Local PATH binary now sourced from `~/www/pg/apps/graphify` (CHORE-009 migration finally took effect this session — previous binary was pinned to `~/ai/graphify`)
- Session-journal not maintained this session. Worth establishing as habit for longer sessions; for ~1h sessions like this one, the conversation context + git log are usually enough
