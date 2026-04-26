# Session Brief — 2026-04-26 (FEAT-043 ship)

## Last Session Summary

Shipped **FEAT-043 `graphify suggest stubs`** end-to-end via subagent-driven-development workflow: 13 plan tasks, 18 commits to `main`, all CI gates green (fmt, clippy, test workspace 754 pass, graphify check 5/5 PASS). Dogfood applied 5 legitimate stubs, surfaced ~30 first-party misclassifications as a separate resolver bug (BUG-022). Filed 4 follow-up tasks (BUG-021, CHORE-010, CHORE-011, BUG-022) and pushed to `origin/main`.

## Current State

- Branch: `main`, latest commit `a5007d0 chore(tasks): file FEAT-043 follow-ups (F1-F4)`
- Working tree: clean
- Origin sync: pushed at close (auto-push fast-forward gate; 18 commits up)
- Latest release: `v0.13.1` — `Unreleased` CHANGELOG section now has the FEAT-043 entry, ready for `v0.13.2` whenever bumped
- Local installed binary: `graphify 0.13.1` at `~/.cargo/bin/graphify` (NOT yet updated — `cargo install --path crates/graphify-cli --force` needed before next dogfood run uses the new feature)
- TaskNotes: 70 total / 4 open / 0 in-progress / 66 done

## Commits This Session

`8fa2cca..a5007d0` (18 commits). Highlights:

- `82cdfa9` docs(spec): FEAT-043 design
- `b530db4` docs(spec): correct data source (graph.json, not analysis.json) — surfaced during plan-writing self-review
- `22ed2ca` docs(plan): 13-task TDD implementation plan
- `c57fea5` build: add toml_edit 0.22 dep
- `350b8bb` feat(report): suggest module skeleton + extract_prefix
- `0edfa84` feat(report): score_stubs with threshold + auto-classify + shadowing
- `184fc99` / `8bb5eae` / `607c448` feat(report): markdown / toml / json renderers
- `69ff242` feat(cli): suggest stubs read-only paths
- `cca4bd8` fix(cli): drop dead any_skipped flag (post-review)
- `c504cc2` feat(cli): --apply via toml_edit (atomic, idempotent)
- `3213379` test(cli): suggest fixture (2-project graph.json)
- `2c2fc92` test(cli): 4 e2e tests (md/json/apply/clap-conflict)
- `b1e8a0b` chore(stubs): 5 dogfood-discovered stubs applied
- `884cb7f` docs: README + CHANGELOG
- `41fd4f9` chore(tasks): FEAT-043 done-record
- `a5007d0` chore(tasks): file F1-F4 follow-ups

## Decisions Made (don't re-debate)

- **Subagent-driven-development scaled well at 13 tasks**: ~14 implementer dispatches + ~8 reviewer dispatches across the run. Combined spec+quality reviews were used for trivial mechanical tasks (renderers, fixture+tests) — the rigid "always two stages" rule from the skill was relaxed in favor of one-stage when the diff was config-only or pure data; flagged each deviation in the controller's text for the operator
- **`graph.json` (not `analysis.json`) as data source**: discovered during plan-writing self-review that `AnalysisSnapshot` lacks `is_local` per node and `weight` per link. Spec was amended (commit `b530db4`) before any code landed. Cheap correction
- **Markdown is the default `--format`; TOML output is commented-out by default** — safe paste; `--apply` (mutex with `--format`) is the in-place mutation path via `toml_edit`
- **5 dogfood stubs applied (`include_str` + `graphify_core/extract/report` + `anstyle`); 30 candidate noise items NOT applied** — they reveal a graphify-resolver bug (BUG-022, high-priority follow-up) that should be fixed at the root, not papered over
- **Auto-push fast-forward gate worked smoothly** in solo-dev mode: 2 batches (mid-session at `cca4bd8` cleanup not pushed; final 18-commit push at close went through cleanly). No race with sibling instance because none was active

## Architectural Health

`graphify check --config graphify.toml` — all 5 projects PASS:

- `graphify-core`: PASS, 0 cycles, max_hotspot 0.487 (`src.policy`)
- `graphify-extract`: PASS, 0 cycles, max_hotspot 0.439 (`src.resolver`)
- `graphify-report`: PASS, 0 cycles, max_hotspot 0.454 (`src.pr_summary`) — slight bump from 0.432 last session, expected after dogfood added stubs that reclassified some Ambiguous edges to ExpectedExternal (FEAT-033 deprioritization shifts relative weights)
- `graphify-cli`: PASS, 0 cycles, max_hotspot 0.469 (`src.install`)
- `graphify-mcp`: PASS, 0 cycles, max_hotspot 0.600 (`src.server`) — also bumped (was 0.559) for the same reason
- Policy violations: 0

All hotspots well under the 0.85 CI threshold.

## Open Items (4 follow-up tasks filed, all pushed)

- **BUG-021** (normal): `already_covered_prefixes` records `extract_prefix(target)` which can be broader than the actually-matched stub — relatório engana usuário. Recommended fix: add `ExternalStubs::matching_prefix(&str) -> Option<&str>`
- **CHORE-010** (low): cross-language same-prefix collision test (30 lines) for `score_stubs`
- **CHORE-011** (normal): move `ExternalStubs` from `graphify-extract` to `graphify-core` to remove the `report → extract` layer crossing FEAT-043 introduced. **Public API change** — affects ~6 import sites
- **BUG-022** (HIGH): graphify resolver classifies ~30 first-party symbols as `is_local=false`. Surfaced by FEAT-043 dogfood. Most actionable starting point: pick `pct` or `manifest::sha256_of_bytes` as canary, trace `rust_lang.rs` + `resolver.rs`. Likely related to FEAT-031 / BUG-019 territory

## Suggested Next Steps

1. **Investigate BUG-022** (high priority) — root-cause the resolver misclassification. If it's one bug affecting all ~30, fix + regression test ships in v0.13.2 alongside the FEAT-043 entry already in CHANGELOG
2. **CHORE-011** — move `ExternalStubs` to `graphify-core` while the FEAT-043 dep edge is fresh (smaller refactor than later). Public API change worth a brief ack from operator before refactoring
3. **`cargo install --path crates/graphify-cli --force`** — local PATH binary is still `0.13.1`; running `graphify suggest stubs` from outside the repo will hit the old binary that doesn't have the subcommand

## Reminders

- `.claude/session-context-gf.json` is `skip-worktree`'d locally — `git ls-files -v` shows `S`. Reverse with `git update-index --no-skip-worktree`
- 17 local-only skills under `~/.claude/skills/` not in upstream cache. Same as last session — silence with `.skills-sync-ignore` per skill or `/share-skill <name>` to publish
