# Session Brief — Next Session (post-2026-04-18d)

**Last session:** 2026-04-18 (tn session `2026-04-18-0837`, 45m / 60m wall, 152k / 400k tokens). Dispatched two tasks via `/tn-plan-session`: **DOC-001** (done, 7m / 48k, commit `15fdccf` — README pre-FEAT-020 migration subsection) and **FEAT-021 Part B slice** (partial, 38m / 104k, commit `0cf10ed` — barrel symbol collapse + JSON writer + TS `is_package` resolver fix). Remainder of FEAT-021 tracked on **FEAT-025**; that task body now has the satisfied subtasks checked off with explicit references to `0cf10ed`.

## Current State

- Branch: `main` @ `0cf10ed feat(extract): collapse TS barrel re-exports to canonical nodes — FEAT-021 Part B (slice)` (will advance by one memory commit at session close)
- CI: green locally at `0cf10ed` (`cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` all passed per dispatcher's self-report)
- tn session `2026-04-18-0837` closed via `/session-close`; calibration picks up two new FEAT/claude-solo samples (DOC=7m vs 6m est, FEAT=38m vs 32m est, both +17-19%)
- Unstaged at session start: two task frontmatter updates (DOC-001 → done, FEAT-021 timeEntry added) from `tn session log` — being committed by `/session-close` along with memory updates
- Graphify itself has no `graphify.toml` at the repo root, so Step 2c (architectural health) is skipped — Graphify analyses other projects, not itself

## Open Items (tn tasks)

- **FEAT-025** (open, normal, ~45m — but scope now **reduced** by Part B slice) — Remaining work: (a) 6 writer fan-outs (CSV, Markdown, HTML, Neo4j, GraphML, Obsidian), (b) **module-level `Imports` edge rewrite** — needs named-import capture in TS extractor, currently only symbol-level edges collapse, (c) confidence downgrade + stderr diagnostic on unresolved fallthrough (policy still open), (d) aliased + cyclic re-export fixtures, (e) hotspot regression on a reference monorepo, (f) perf delta check, (g) Q1 resolution (tsconfig paths through barrels), (h) the closing `CLAUDE.md` bullet. Realistic estimate dropped to ~35m given the slice already landed.
- **FEAT-021** (open, low) — stays open until FEAT-025 lands. Two parts now done; Part B *slice* closed the symbol-level collapse, module-level edges deferred.
- **FEAT-024** (open, low, ~30m) — `pr-summary` renderer gains `[allowlisted]` / `[intentional mirror]` tail annotations. Reads `allowlisted_symbols` from `analysis.json` (FEAT-020) + intentional-mirror matches from `drift` input (FEAT-023). Four unit fixtures. Untouched this session.
- **DOC-001** — closed this session (done, 7m / 48k, `15fdccf`).
- **Four pre-existing frontmatter-invalid tasks** still surface in `tn list --invalid`: BUG-007 (`priority: critical`), FEAT-002 (missing tags), FEAT-011 (`priority: medium`), and `sprint.md` (missing uid). Pre-existing, not this session's scope.

## Decisions Made (don't re-debate)

*(carried from prior sessions — see commit history + CLAUDE.md for full ledger)*

*(added 2026-04-18d)*

- **FEAT-021 Part B was split into a slice + remainder:** the dispatcher deliberately landed the symbol-level collapse + JSON writer + 1 fixture as a partial (38m vs 48m F2 cap), and routed the 6-writer fan-out + module-edge rewrite to FEAT-025. The reasoning is specifically that the module-edge rewrite needs **named-import capture** in the TS extractor which is a separate sub-feature — lumping it in would have blown the cap. Do not try to redo the slice in FEAT-025; the checked subtasks in its body are authoritative.
- **Unresolved-chain confidence policy is still open.** `0cf10ed` emits the stderr warning only for the `Cycle` outcome; the `Unresolved` outcome currently leaves the node untouched with its original confidence. FEAT-025 needs to pick one of: (a) downgrade Unresolved to `Ambiguous` + stderr, (b) leave as-is silently, (c) leave as-is with a stderr diagnostic but no confidence change. The spec in FEAT-021's original body called for downgrade+diagnostic; revisit there when FEAT-025 starts.
- **`resolve_ts_relative` `is_package` fix is BUG-001-pattern.** Don't generalize further in FEAT-025 — the fix already mirrors the Python symmetry. If a third language surfaces the same bug (PHP?), consider extracting a shared helper, but not before.

## Suggested Next Steps

1. **FEAT-025 (~35m reduced)** — closes the FEAT-021 loop. Use `claude-solo` (sample=7 now for FEAT/claude-solo). The 6-writer fan-out is mechanical; the module-edge rewrite is the interesting part (needs named-import capture). Could split again if the named-import work turns out >20m on its own.
2. **FEAT-024 (~30m)** — trivial annotation strip on `pr-summary`. Good candidate to bundle with a short FEAT-025 slice in one session, or run solo.
3. **Sprint/frontmatter cleanup (~10m)** — fix the four invalid-frontmatter tasks so `tn list --invalid` is empty. Optional.

## Out of Scope (for next session unless lifted)

- Patching `is_stub_body_str` in `tasknotes-cli` to count `## Description` content as description. Tracked as an explicit unchecked subtask in CHORE-002's body; lives in the sibling `tasknotes-cli` repo, not this one.
- Named-import capture as a standalone feature. It's a prerequisite for FEAT-025's module-edge rewrite but does not need its own task — let FEAT-025 absorb it.
- Python barrel equivalence (`from .foo import Bar` in `__init__.py`). Explicitly out of scope for FEAT-021/025 v1.

## Re-Entry Hints (survive compaction)

1. Re-read this file + `CLAUDE.md` (the new bullets on `0cf10ed` sit near the end of `## Conventions`, right after the `tn` body-is-stub bullet)
2. `git log origin/main..HEAD --oneline` — see unpushed work (includes `15fdccf`, `0cf10ed`, the closing memory commit, plus any prior unpushed)
3. `git status --short` — only `target/` artifacts expected
4. `tn list --status open` — should show FEAT-025, FEAT-021, FEAT-024 (three rows; DOC-001 now closed)
5. `tn time --roi --week` — FEAT/claude-solo `sample=7`, DOC/claude-solo `sample=1`, CHORE/claude-solo `sample=1`
6. Start-of-session reads for FEAT-025: commit `0cf10ed` diff (ground truth for what's already wired), `crates/graphify-cli/src/main.rs` `run_extract` (the collapse loop), `crates/graphify-report/src/json.rs` (the one writer already fanned out), `crates/graphify-extract/src/typescript.rs` (named-import capture gap lives here)

## Team Dispatch Recommendations

- **FEAT-025**: `claude-solo` — precedent established; 7 samples now. Alternative: `claude-subagents` with `parallelParts: 2` (split between the writer fan-out and the edge rewrite) if you want to test that dispatch path, but the two halves share state (resolver output) so the split is awkward. Recommend solo.
- **FEAT-024**: `claude-solo` — trivial. 30m.

## Context Budget Plan

- **Start of next session**: brief + `CLAUDE.md` + FEAT-025 body + `0cf10ed` diff ≈ 10k tokens
- For FEAT-025 as one shot (~35m): expect 80-150k tokens (6 writers × ~4k Edit each + fixtures + named-import capture in extractor). Factor in the observable-vs-heuristic gap (~25-32% under-count — see Calibration Observations below).
- `/clear` before FEAT-025 is not necessary unless stacking it with FEAT-024 in the same hour.

## Calibration Observations (for CHORE-004 if it exists)

- **DOC/claude-solo with `sample=0` estimate was close.** DOC-001 est 6m → actual 7m (+17%). Single sample, but reasonable.
- **FEAT/claude-solo with `sample=6` estimate also +19%.** FEAT-021 Part B slice est 32m → actual 38m. The estimator is consistently under-predicting on the order of 15-20% across two executors this session. Not yet enough to adjust the ratio; watch the next 3 samples.
- **Subagent heuristic under-counts observable by ~25% this session.** FEAT-021 Part B dispatch reported `tokens: 104000` via the rule-#2 heuristic; the parent's tool-return `<usage>` block showed `total_tokens: 129942`. Matches prior session's ~32% gap; the "heuristic lags observable" finding holds. Logged 104k per protocol.
- **Partial outcomes carry their full time.** FEAT-021 logged 38m even though the outcome was `partial` — the slice genuinely consumed the wall-clock. This is correct for calibration (we want ratio-of-actual-to-estimate), but worth noting that `timeSpent` on the task file (66m cumulative across Parts A + B slice) doesn't equal "work to close the task" — FEAT-025 is where that closes.
