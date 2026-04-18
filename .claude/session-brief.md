# Session Brief — Next Session (post-2026-04-18c)

**Last session:** 2026-04-18 (tn session `2026-04-18-0811`) — started in a blocked state (all four open candidates rejected by `tn session start --plan-only` as `body is stub`). Diagnosed the `tasknotes-core::task::is_stub_body_str` heuristic (only counts prose between `# Title` and the first `## …` — everything under `## Description` gets ignored), applied the TL;DR unblock to FEAT-021 + CHORE-002, re-planned. Dispatched CHORE-002 (done, 6m / 42k, commit `7780268`) and FEAT-021 Part A (partial, 28m / 100k, commit `e082c6a` — `Node.alternative_paths` scaffold, `ExtractionResult.reexports`, TS extractor captures named + star re-exports, new `reexport_graph` module with cycle-safe `Canonical | Unresolved | Cycle` walker, 21 unit tests, workspace fmt/clippy/test clean). After dispatch: authored FEAT-025 (Part B follow-up), DOC-001 (README migration subsection), FEAT-024 (pr-summary annotations) — all three now pass feasibility. Session-log + follow-up authoring committed in `dfff6d3`.

## Current State

- Branch: `main` @ `dfff6d3 chore(tn): log session 2026-04-18-0811 + author FEAT-025/DOC-001/FEAT-024 bodies`
- CI: green as of `e082c6a` (Part A); `dfff6d3` is docs-only so CI gates not re-run but nothing touched Rust code
- tn session `2026-04-18-0811` closed via `/session-close`; calibration updated (8 observations across 2 cells: CHORE/claude-solo now `sample=1`, FEAT/claude-solo now `sample=6`)
- Unstaged at close: only `target/` build artifacts (pre-existing, gitignored-equivalent)
- Four open tn tasks (see below); three pre-existing frontmatter-invalid files surfaced by `tn list` (BUG-007 priority `critical`, FEAT-002 missing tags, FEAT-011 priority `medium`, sprint.md missing uid) — pre-existing issues, not this session's scope

## Open Items (tn tasks)

- **FEAT-025** (open, normal, ~45m) — **recommended first** — Part B of FEAT-021: wire `reexport_graph` resolver into edge emission, fan `alternative_paths` through 7 writers (JSON/CSV/MD/HTML/Neo4j/GraphML/Obsidian), run hotspot regression on a reference TS monorepo. Acceptance criteria explicit in body. All CI gates covered.
- **FEAT-021** (open, low) — stays open until FEAT-025 lands. Part A's `reexport_graph` module is standalone-green; Part B wires it into the pipeline and fans the field through writers. Do NOT close FEAT-021 until both parts ship.
- **DOC-001** (open, low, ~20m) — README migration subsection under existing "Consolidation Candidates" block. Pure docs. Scoped tight in the task body (three short paragraphs + two snippets).
- **FEAT-024** (open, low, ~30m) — `pr-summary` renderer gains `[allowlisted]` / `[intentional mirror]` tail annotations. Reads existing `allowlisted_symbols` field from `analysis.json` (FEAT-020) + intentional-mirror matches from `drift` input (FEAT-023). Four unit fixtures.
- **CHORE-002** — closed this session (done, 6m / 42k, `7780268`). Body now documents the tn heuristic gotcha for future reference. tn-side patch of `is_stub_body_str` explicitly deferred as a follow-up in the tasknotes-cli repo (not graphify scope).

## Decisions Made (don't re-debate)

*(carried from prior sessions — see commit history + CLAUDE.md for full ledger)*

*(added 2026-04-18c)*

- **FEAT-021 split into Part A scaffold + Part B wiring:** the dispatcher took the split deliberately within the 36m/54m guardrail. Part A is standalone-green — the tree compiles and tests pass without any downstream consumer reading `ExtractionResult.reexports` or a populated `Node.alternative_paths`. Part B in FEAT-025 closes the loop. Do not backtrack and try to merge them.
- **FEAT-025 scope includes the perf budget check:** the two-pass extraction on a large monorepo is called out in FEAT-021's Open Questions; FEAT-025 subtask list includes a back-of-envelope perf delta. If the delta exceeds 20% on the reference monorepo, STOP and plan a caching layer before shipping — don't merge a slowdown.
- **Consolidation migration framing (DOC-001):** there was never a real `.consolidation-ignore` file format in this repo — that's historical framing from the task title. The migration subsection should explain that pre-FEAT-020 exclusions lived in shell scripts / CI grep-excludes / local conventions, and map those into `[consolidation].allowlist` + `[consolidation.intentional_mirrors]`. Do not introduce a `.consolidation-ignore` format.
- **pr-summary annotation source (FEAT-024):** read `allowlisted_symbols` from `analysis.json` directly (already populated by FEAT-020 when `[consolidation]` is present). Do not thread `ConsolidationConfig` through as a new argument — keeps the renderer pure over JSON inputs.

## Suggested Next Steps

1. **FEAT-025 (~45m)** — closes the FEAT-021 loop. Highest pending-value work. Use `claude-solo` (Part A was `claude-solo` with `sample=6` → high confidence now). Likely a full session on its own given the 7-writer fan-out.
2. **DOC-001 (~20m)** — cheap docs win. Can be bundled with FEAT-024 in one session (both <30m, unrelated code areas, `/clear` between them if token pressure).
3. **FEAT-024 (~30m)** — trivial annotation strip. Fixture-heavy.
4. **Sprint/frontmatter cleanup (~10m)** — fix the three invalid-frontmatter tasks (BUG-007 priority, FEAT-002 missing tags, FEAT-011 priority, sprint.md missing uid) so `tn list` stops complaining. Optional; pre-existing.

## Out of Scope (for next session unless lifted)

- Patching `is_stub_body_str` in tasknotes-cli to count `## Description` content as description. Tracked as an explicit unchecked subtask in CHORE-002's body; lives in the sibling `tasknotes-cli` repo, not this one.
- Updating the `tn new` template to ship with a TL;DR paragraph. Same repo as above.
- Making `Node.alternative_paths` populate for Python barrel-equivalent (`from .foo import Bar` in `__init__.py`). Explicitly out of scope for FEAT-021/025 v1.

## Re-Entry Hints (survive compaction)

1. Re-read this file + `CLAUDE.md` (the new body-is-stub bullet sits near the end of `## Conventions`, right after the other `tn` feasibility-check heuristics bullet added in `7780268`)
2. `git log origin/main..HEAD --oneline` — see unpushed work (includes `7780268`, `e082c6a`, `dfff6d3`, and any prior unpushed)
3. `git status --short` — only `target/` artifacts expected
4. `tn list --status open` — should show FEAT-025, FEAT-021, DOC-001, FEAT-024 (four rows)
5. `tn time --roi --week` — CHORE/claude-solo `sample=1`, FEAT/claude-solo `sample=6`
6. Start-of-session reads for FEAT-025: commit `e082c6a` diff (Part A ground truth), `crates/graphify-extract/src/reexport_graph.rs` (the walker to wire), `crates/graphify-extract/src/lib.rs` (where edge emission happens)

## Team Dispatch Recommendations

- **FEAT-025**: `claude-solo` — Part A precedent established; no new architectural risk. Alternative: `claude-team` with self-review if the 7-writer fan-out feels large, but note that F2 degrades team → solo+self-review anyway. 45m.
- **DOC-001**: `claude-solo` — pure docs. 20m.
- **FEAT-024**: `claude-solo` — trivial. 30m.

## Context Budget Plan

- **Start of next session**: brief + CLAUDE.md + FEAT-025 body + commit `e082c6a` body ≈ 8k tok
- For the combined DOC-001 + FEAT-024 run (~50m): `/clear` between them is overkill — different writers/renderers but both small. Single-session fine.
- For FEAT-025 on its own: expect 150–250k tokens (7 writers × ~4k Edit each + fixtures + resolver integration + regression script). Factor in the observable-vs-heuristic gap (~32% under-count — see below).

## Calibration Observations (for CHORE-004 if it exists)

- **Subagent heuristic under-counts observable by ~32%.** FEAT-021 Part A dispatch reported `tokens: 100000` via the rule-#2 heuristic (~20k baseline + 2.5k per Read + 4k per Edit + 1k per small Bash); the parent's tool-return `<usage>` block showed `total_tokens: 146956`. Matches the dispatcher spec's own caveat that the heuristic lags observable. Logged 100k per protocol (consistency with documented rule), but future calibration regressions should use the observable values when available. This is one of the samples CHORE-004 calibration convergence should account for.
- **Planning-phase work can exhaust a meta-task's content.** CHORE-002's core value was the heuristic diagnosis — produced during `/tn-plan-session` planning, not during dispatch. The dispatched subagent only did the `CLAUDE.md` note addition (~5m of real work) even though the estimate was 8m. For future meta-tasks where the "meta" work IS the diagnosis, consider whether dispatch even makes sense or just manually logging the planning-phase time is cleaner.
