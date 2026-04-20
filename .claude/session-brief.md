# Session Brief — Next Session (post-2026-04-20, session `2026-04-20-1437`)

**Last session:** 2026-04-20 (tn session `2026-04-20-1437`, 2h 31m wall / 60m budget = 251%, subagent-tokens 521k / 400k advisory = 130%, 5 dispatches all `source=<usage>`). Dispatched FEAT-028 across 5 slices (scaffold → walker → alias resolver → inner-glob matcher → P2a+P2b+P3 in one sweep) — feature functionally shipped, tripwire inverted. Also created CHORE-004 + CHORE-005 as meta follow-ups on the tn/skill side from a live misread incident.

## Current State

- Branch: `main` — session advances main by **7 FEAT-028 commits** (`0fe862b` scaffold → `cd760a1` inverted tripwire) + this close commit
- CI locally green across all 7 commits (`cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`). Workspace test count advanced (now includes `feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope` replacing the old FEAT-027 tripwire, plus ~23 new unit tests across `workspace_reexport.rs` and `resolver.rs`).
- tn session `2026-04-20-1437` — to be closed at end of this session-close. Calibration now has **24 observations** (5 FEAT-028 partials + prior 19) across 3 cells; FEAT/claude-solo sample grew from 9 → 14. All 5 FEAT-028 dispatches logged with `source=<usage>` tokens (observed, not heuristic) per CHORE-004/DOC-003 dual-schema handling.
- `target/` binaries still show as modified in `git status` — legacy-tracked, NEVER stage (`.gitignore` `/target` only catches untracked).
- No GitHub issues opened or closed this session.
- **Known CI caveat**: the dispatcher self-reported tests as green on the 5th dispatch; next session should run `cargo test --workspace` once cold to confirm nothing raced with the phase-split refactor.

## Open Items (tn tasks)

- **FEAT-028** (open, normal, ~1h remaining across 2 steps) — **functionally shipped this session**, body updated with completion table. Remaining: step 6 (cursos `cross_project_edges` regression benchmark mirroring CHORE-003 shape — quantify the "2,165 → per-canonical fan-in" motivation claim) + step 8 (feature-gate decision, currently always-on; opt-out flag direction recommended). Both small enough to slot into a future sprint.
- **CHORE-004** (open, low, ~45m) — Rename `main-context budget:` → `main-context snapshot:` in tn's `session log` success line to match BUG-012/DOC-003 snapshot semantics. Lives in the tn repo (external), tracked here for follow-through.
- **CHORE-005** (open, low, ~30m) — Add explicit guard in `/tn-plan-session` Step 8 against recommending close on `subagent_tokens_sum` approaching `budget.tokens`. Lives in the skill, tracked here.
- **FEAT-021** (open, low) — umbrella. Part A + Part B + FEAT-025 writer fan-out + FEAT-026 module edges + FEAT-027 split-answer + FEAT-028 cross-project all landed. Body still has unchecked subtasks that are now arguably done elsewhere. Consider closing FEAT-021 with a pointer to the FEAT-028 completion.
- **FEAT-027** (open) — was the spike that produced the FEAT-028 tripwire. Since FEAT-028 inverted that tripwire and shipped the fix, FEAT-027 can probably close as `done` with a one-line pointer to FEAT-028's commits.
- **Four pre-existing frontmatter-invalid tasks** still surface in `tn list --invalid`: BUG-007 (`priority: critical`), FEAT-002 (missing tags), FEAT-011 (`priority: medium`), `sprint.md` (missing uid). Pre-existing, cosmetic.

## Decisions Made (don't re-debate)

*(carried from prior sessions — see commit history + CLAUDE.md for full ledger)*

*(added 2026-04-20, session `2026-04-20-1437`)*

- **Workspace graph is topology-triggered, not flag-triggered.** Built only when ≥2 projects AND ≥1 TS project — single-project and non-TS configs stay on the legacy fast path with zero overhead. If step 8 (feature-gate decision) adds an opt-out flag, the default stays `true` (always build when topology matches); the flag would be purely escape-hatch for edge cases.
- **Option 2 namespacing locked.** Public node ids stay per-project (e.g. `src.foo` in both consumer and core projects). Workspace lookup is an internal `modules_to_project` first-wins index + collision log. Cross-project edges reference the target's `module_id` verbatim. Decision rationale in `workspace_reexport.rs` module-level doc-comment. Do NOT switch to full-prefix (`core.src.foo`) without a migration task — it breaks every downstream consumer of `graph.json` / `analysis.json`.
- **`match_alias_target` supports inner-glob tsconfig paths.** `"@repo/*": ["../../packages/*/src"]` now resolves. Previously trailing-`*` or exact-match only (pre-existing limitation unearthed during FEAT-028 slice 4). Tests in `resolver.rs` pin the contract.
- **`run_extract` is two-phase.** Phase 1 = `build_project_reexport_context` (collect, no edges emitted). Phase 2 = `run_extract_with_workspace` (fan-out against the merged workspace graph). Any future multi-project feature (FEAT-029+) should plug into this structure, not re-split the function.
- **tn `subagent_tokens_sum / budget.tokens` is NOT a dispatch-capacity ceiling.** It's an FEAT-019 calibration advisory meter. Each `Task` subagent allocates a fresh 1M model-context window. CHORE-005 adds an explicit guard in `/tn-plan-session` Step 8 against the misread. Until that ships, orchestrator discipline is: trust Claude Code status bar `% ctx` for real-time headroom, use tn's meter only to feed calibration.
- **tn session token source preference.** Prefer `<usage>total_tokens: N` from the top-level Task return (flat plaintext, primary in practice — observed in all 5 FEAT-028 dispatches) over the dispatcher's heuristic self-report. Dual-schema parser per DOC-003; fall back to heuristic only if `<usage>` absent. `--note "source=<usage>"` tags the log entry so calibration can distinguish.

## Suggested Next Steps

1. **Close FEAT-028 step 6** — run the cursos `cross_project_edges` regression benchmark (mirror CHORE-003's shape at `docs/benchmarks/2026-04-20-feat-021-025-cursos-regression.md`). Run Graphify on cursos @ fixed commit before (v0.10.0, pre-FEAT-028) vs after this session's main HEAD. Primary metric: `cross_project_edges` from `graphify-summary.json` — the motivation claim was "2,165 barrel-inflated edges should redistribute". Secondary: top-N canonical destinations, hotspot score movement on shared-package symbols.
2. **Close FEAT-028 step 8** — feature-gate decision. Recommendation in the task body: opt-out flag default `true` + stderr notice on first workspace run. Small code change (add `workspace_graph` to `[settings]` in `config.rs`, gate the aggregation path). Slot in the same sprint as step 6.
3. **Close stale FEAT-027** — it can be `tn done FEAT-027` with a one-line pointer; the v1 tripwire it landed has been intentionally inverted by FEAT-028.
4. **Consider closing FEAT-021** — umbrella task, all child slices now shipped. Close with pointer-to-completion commits.
5. **CHORE-004 + CHORE-005 when convenient** — both are small quality-of-life fixes, not blocking anything. Do them opportunistically when in the tn repo or editing skills.

## Meta Learnings This Session

- `/tn-plan-session` skill has correct guidance but burying "do NOT close on token advisory" in Step 8 bullet 5 was insufficient — the misread happened anyway. CHORE-005 addresses.
- tn output wording `main-context budget: X / Y` actively misleads — "budget" implies enforcement, but the field is a snapshot. CHORE-004 addresses.
- When `tn` plans a task with ratio 0.36 against an author's `timeEstimate: 300`, the resulting 32m estimate is useful context (calibration says "historically 0.36× of author estimates"), NOT a ceiling. FEAT-028 actually took ~151m across 5 slices — ratio 0.5× against author estimate, calibration should shift after this session's logs.
- Incremental commit-per-slice pattern from the dispatcher spec paid off: 7 atomic commits, each CI-green, tripwire held until the intentional flip. If FEAT-028 had been paused mid-flight, any slice would have been safe to leave on main.
