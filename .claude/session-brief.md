# Session Brief — Next Session (post-2026-04-20)

**Last session:** 2026-04-20 (tn session `2026-04-20-1216`, 46m / 60m wall, 174k / 400k tokens). Dispatched two tasks via `/tn-plan-session`: **CHORE-003** (done, 12m / 62k `<usage>`, benchmark — headline −17.1% nodes, −89% top hotspot on `parisgroup-ai/cursos` @ `8ff36cc1`) and **FEAT-026** (done, 34m / 112k `<usage>`, module-level TS named-import edges now fan out to canonical modules). Also created 3 tasks (FEAT-026, CHORE-003, FEAT-027), fixed sprint.md drift (BUG-014 → done), and closed GitHub issue #13 (the originating consolidation proposal) with a resumé pointing at the 6 shipped FEAT/DOC IDs + 3 follow-ups.

## Current State

- Branch: `main` @ `b39937f` (v0.10.0) at session start → advances by 3 commits at session close (FEAT-026 code, CHORE-003 benchmark, session memory)
- CI locally green: `cargo fmt --all -- --check` + `cargo clippy --workspace -- -D warnings` passed; `cargo test --workspace` passed per dispatcher self-report (integration test count 13 → 14, new `feat_026_named_imports_fan_out_to_canonical_modules`)
- tn session `2026-04-20-1216` closed; calibration now has 19 observations across 3 cells. FEAT/claude-solo sample=9 (FEAT-026 ratio 1.06× — nearly spot-on at 34m vs 32m est); CHORE/claude-solo sample=3 (CHORE-003 ratio 1.71× — low_confidence flag was accurate).
- Working-tree note: `target/` binaries show as modified in `git status` because they are **tracked** in git (legacy) — NEVER stage these; the `.gitignore` `/target` entry only catches new untracked files.
- Benchmark worktree preserved at `/tmp/graphify-benchmark/v0.9.0` (disposable — `git worktree remove /tmp/graphify-benchmark/v0.9.0` when done).
- GitHub issues: 0 open. Issue #13 closed this session with consolidated summary.

## Open Items (tn tasks)

- **FEAT-027** (open, low, ~2h — scope likely reduced now) — Spike on `tsconfig.json` paths that traverse barrels to canonical modules. With FEAT-026 landed, this is probably already covered for the symbol layer via the re-export graph + named-import fan-out; needs a 2-project fixture that imports via `@repo/core` alias through a barrel to verify. If covered, close with a regression test; if not, draft FEAT-028 with the delta.
- **FEAT-021** (open, low) — The umbrella. Part A + Part B slice + FEAT-025 writer fan-out + FEAT-026 module edges all landed; the task body still has unchecked subtasks for "aliased re-export fixtures" and "perf delta" that are now arguably FEAT-027's job. Consider closing FEAT-021 with a pointer to FEAT-027.
- **FEAT-025** (done). The three follow-ups it spawned: FEAT-026 (done), CHORE-003 (done), FEAT-027 (open). Full loop closed except the tsconfig spike.
- **Four pre-existing frontmatter-invalid tasks** still surface in `tn list --invalid`: BUG-007 (`priority: critical`), FEAT-002 (missing tags), FEAT-011 (`priority: medium`), `sprint.md` (missing uid). Pre-existing, cosmetic.

## Decisions Made (don't re-debate)

*(carried from prior sessions — see commit history + CLAUDE.md for full ledger)*

*(added 2026-04-20)*

- **Module-layer edge rewrite uses the same ReExportGraph as the symbol layer.** FEAT-026 deliberately reuses `resolve_canonical` (not a parallel structure) so the two layers stay in lock-step. Don't build a separate resolver for module-layer in a future task.
- **Unresolved/Cycle outcomes fall back to barrel-targeted edges.** Explicit v1 policy to keep "no import is ever silently dropped." If FEAT-025's original "downgrade Unresolved confidence" idea resurfaces, it needs a new task that consciously chooses between (a) silent-fallback-to-barrel (current), (b) downgrade-on-fallback, (c) warn-but-keep — don't default to (b) just because it was in a prior spec.
- **`import * as ns from '...'`** stays single-edge-to-barrel. The specifier set is empty at the syntax level; walking is semantically wrong. If a consumer asks "why is my namespace import still landing on the barrel", point at this note.
- **Type-only imports (`import type { Foo }`) keep parity.** They contribute `is_type_only: true` entries and walk through the fan-out path, weight 1. Changing this to "skip type-only imports entirely" is a behavioral change that would need a migration-note task; don't do it casually.
- **Consolidation allowlist + intentional_mirrors loop is fully closed.** Issue #13's Ask A shipped as FEAT-020/022/023/024 + DOC-001 (all done 2026-04-18). Don't re-open — the allowlist is the contract, the `.consolidation-ignore` workaround is documented as deprecated.

## Suggested Next Steps

1. **FEAT-027 (~1-2h, likely shorter)** — Build the 2-project alias fixture, run current HEAD, verify whether `@repo/core/Foo` with a barrel target already resolves canonically post-FEAT-026. If yes: add a regression test + close. If no: draft FEAT-028 (e.g., apply re-export walker to alias-resolved module ids with a cross-project flag) and close FEAT-027 as investigation-complete. `claude-solo` (sample=9 for FEAT).
2. **Close FEAT-021** — All its children landed. A 5-minute cleanup: verify the body subtasks, check the remaining "aliased re-export fixtures" note maps to FEAT-027, and `tn done FEAT-021`.
3. **Sprint/frontmatter cleanup (~10m)** — fix the four `tn list --invalid` rows. Optional cosmetic.
4. **Version bump + release** — v0.10.0 shipped at `b39937f` *before* FEAT-026/CHORE-003. Next release (v0.11.0?) should capture module-edge fan-out + the benchmark. Per CLAUDE.md "Version bump" recipe.

## Out of Scope (for next session unless lifted)

- Python barrel equivalence (`from .foo import Bar` in `__init__.py`). Still out — not asked.
- `is_stub_body_str` fix in sibling `tasknotes-cli` repo. Still out — that's the `tasknotes-cli` maintainer's job.
- FEAT-028 / cross-project alias-through-barrel handling. Stays latent unless FEAT-027's spike says "not covered."

## Re-Entry Hints (survive compaction)

1. Re-read this file + CLAUDE.md (the FEAT-026 / CHORE-003 paragraph now lives at the end of the existing "TS barrel re-export collapse" bullet in `## Conventions`)
2. `git log origin/main..HEAD --oneline` — 3 unpushed commits expected if the push auto-step ran (FEAT-026 code, CHORE-003 benchmark, session memory)
3. `git status --short` — expect only `target/` binaries as noise
4. `tn list --status open` — should show FEAT-027 + FEAT-021 (FEAT-021 is probably closable on inspection)
5. `tn time --roi --week` — FEAT/claude-solo now `sample=9` (steady), CHORE/claude-solo `sample=3`
6. Start-of-session reads for FEAT-027: `crates/graphify-extract/src/resolver.rs` (alias path), `crates/graphify-extract/src/typescript.rs` (named-import capture, post-FEAT-026), `docs/benchmarks/2026-04-20-feat-021-025-cursos-regression.md` (where to extend the monorepo fixture count)

## Team Dispatch Recommendations

- **FEAT-027**: `claude-solo` — precedent established; 9 samples. Spike tasks with `uncertainty: high` tend to undershoot wall-clock when the answer is "covered" and overshoot when it's "not covered" — let the dispatcher pick outcome `partial` if the spike reveals a net-new task.

## Context Budget Plan

- **Start of next session**: brief + CLAUDE.md + FEAT-027 body + post-FEAT-026 tree diff ≈ 12k tokens
- FEAT-027 spike (~1-2h): expect 50-90k tokens if it ends in a regression fixture; 30-50k if it ends in a no-op close. Observable-vs-heuristic gap this session closed to ~6% (vs ~25-32% prior) — the `<usage>` parsing in `/tn-plan-session` step 7 is working as designed.

## Calibration Observations

- **FEAT-026 ratio 1.06× (sample=9) → calibrator is converging on claude-solo/FEAT.** Estimate of 32m vs actual 34m. The ratio 0.35 held; the sample-weight is now high enough that a single outlier won't shift the prediction materially.
- **CHORE-003 ratio 1.71× (sample=3) with low_confidence flag accurate.** Estimate 7m → actual 12m. This is the third CHORE data point; expect the ratio to shift from the default 0.36 toward ~0.42-0.45 at the next calibration rebuild.
- **Observable vs heuristic: CHORE-003 62k observed vs 62k heuristic (~0%), FEAT-026 112k observed vs 102k heuristic (−9%).** Gap shrank from prior session's 25%. Bigger news: dispatcher self-report is now reliably within 10% — the FEAT-030 / CHORE-004 wiring is landing accurate data.
- **Stop hook still silent this session** (`hook_fired: false`, `main_context_inactive: true` after first log). `--tokens 400k` budget ceiling was not enforced; all calibration came from per-dispatch `--tokens` logs. Known limitation; the FEAT-019 calibration-source contract handles it.
