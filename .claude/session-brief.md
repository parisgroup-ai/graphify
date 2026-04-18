# Session Brief — Next Session (post-2026-04-18)

**Last session:** 2026-04-18 — landed FEAT-020 slice (commit `25eabc8`): first-class `[consolidation]` section in `graphify.toml` with anchored leaf-symbol regex matching, `allowlisted_symbols` in `analysis.json` (opt-in, backward compatible), `--ignore-allowlist` flag on `run`/`report`/`check`, hotspot gate allowlist-aware. CI gates green (fmt, clippy, test). Dispatched via `/tn-plan-session` → tn session `2026-04-18-0001`. FEAT-020 remains **in-progress** (partial); 4 follow-ups spun out.

## Current State

- Branch: `main` @ `25eabc8 feat(consolidation): [consolidation] allowlist in graphify.toml (FEAT-020 slice)`
- CI: green on the 3 gated commands (`cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`)
- Unstaged session artifacts to stage in /session-close commit: `.tasknotes.toml` (schema fix), `docs/TaskNotes/Tasks/FEAT-020*.md` (path-ref fix + tn timeEntries), `docs/TaskNotes/Tasks/FEAT-021*.md` (status backlog→open)
- Pre-existing unstaged (not ours, leave alone): `docs/TaskNotes/Tasks/CHORE-001-*.md`, `target/**`, `tests/fixtures/contract_drift/monorepo/report/`
- tn session `2026-04-18-0001` closed via `tn session close` at end of this session; calibration now has `sample=1` for FEAT-sized work

## Open Items (tn tasks)

- **FEAT-020** (in-progress, normal) — partial; next slices tracked below
- **FEAT-021** (open, low) — blocked on tn feasibility `body is stub`; unblock via CHORE-002
- **FEAT-022** (open, normal, ~1h) — `graphify consolidation` subcommand emits `consolidation-candidates.json`
- **FEAT-023** (open, normal, ~45m) — honour `[consolidation.intentional_mirrors]` to suppress cross-project drift entries
- **FEAT-024** (open, low, ~30m) — integrate allowlist into `pr-summary` hotspot annotations
- **DOC-001** (open, low, ~20m) — README section + migration note for `.consolidation-ignore` → `graphify.toml`
- **CHORE-001** (pre-existing, normal) — apply `cargo fmt --all` to fix lingering rustfmt violations (status may have shifted — verify)
- **CHORE-002** (open, low, ~20m) — rewrite FEAT-021 body to pass tn feasibility check (stub heuristic)

## Suggested Next Steps

1. **FEAT-022** (~1h) — natural continuation of FEAT-020 slice; consolidation subcommand lets skill consumers drop their bash+grep+python pipeline
2. **FEAT-023** (~45m) — `intentional_mirrors` is the bigger user-win from GH#13 (cross-project drift dedup); low risk since structure already established
3. **DOC-001 + README migration** — before FEAT-022 if you want users to adopt the `[consolidation]` section already shipped in 25eabc8
4. **CHORE-002** — 20m unblock of FEAT-021; low priority but keeps the planning flow clean
5. **Meta (not in tn):** upgrade `tn` CLI from 0.2.0 → ≥0.3.0 to match the `<!-- min-tn: v0.3.0 -->` requirement in `/tn-plan-session`

## Decisions Made (don't re-debate)

*(carried from prior sessions — see commit history + CLAUDE.md for full ledger)*

*(added 2026-04-18)*

- **Consolidation matching semantics:** regex `^…$`-anchored against the **leaf symbol name** (last dot-segment), not the full node id. Avoids accidental substring hits on nested modules. Ships in 25eabc8.
- **Backward compat:** `analysis.json` omits `allowlisted_symbols` when no section configured. Do NOT make it a default-empty array — downstream consumers would have to grow a schema check they don't need today.
- **Fail-fast regex validation:** invalid pattern aborts `load_config` with the offending pattern surfaced. Loading a bad config and silently dropping patterns would be worse than erroring at the config boundary.
- **F2 dispatcher fallback is intentional:** `claude-team` items degrade to `claude-solo + self-review` because `TeamCreate` lands in F3. No need to plumb around it — when F3 ships, the fallback disappears automatically.
- **`.tasknotes.toml` canonical shape for graphify:** `tasks_dir = "docs/TaskNotes/Tasks"`, `sprint_file = "docs/TaskNotes/Tasks/sprint.md"`, `archive_dir = "docs/TaskNotes/Tasks/archive"`, `[defaults]`, `[id]`. The bare `[project]` scaffold tn produces on init is **not in the schema** and is silently ignored — always use the full form.
- **tn status vocabulary is `{open, in-progress, done}` only** — any other value (e.g. `backlog`) makes tn silently drop the file from `tn list` (the real error only surfaces via `tn show <id>`). Graphify tasks are `open` when unstarted.
- **tn feasibility check heuristics to avoid:**
  - Any backticked `.rs` path in a task body is stat'd; paths for *new* files must be described without a fully-qualified `.rs` filename (use `a new module under \`crates/<crate>/src/\`` phrasing instead).
  - Paths need the `crates/` prefix — graphify's `.rs` files all live under `crates/<crate>/src/…`.
  - "body is stub" rejection triggers on some heuristic not yet understood (FEAT-021 tripped it despite 151 lines of content) — investigate via CHORE-002.

## Out of Scope (for next session unless lifted)

- Full FEAT-020 breadth beyond the committed slice — each remaining piece has its own FEAT-022/023/024/DOC-001 ticket
- Structural refactor of `graphify_core::consolidation` module — good as-is, regex-anchored approach is deliberately conservative
- Making `allowlisted_symbols` a required field in `analysis.json` — breaks backward compat for questionable gain

## Re-Entry Hints (survive compaction)

1. Re-read `.claude/session-brief.md` (this file) + `CLAUDE.md` (consolidation conventions at end of `## Conventions`)
2. `git log origin/main..HEAD --oneline` — see unpushed work
3. `git status --short` — should be clean after /session-close commit (excluding pre-existing target/ + CHORE-001 noise)
4. `tn list` — should show FEAT-020 (in-progress), FEAT-021 (open), FEAT-022/023/024, DOC-001, CHORE-001/002 as open
5. `tn time --roi --week` — calibration now has 1 sample (FEAT, claude-solo, 45m)
6. `25eabc8` is the canonical reference commit for the `[consolidation]` section — read its body + the regex fixtures it introduces before touching consolidation code

## Team Dispatch Recommendations

- **FEAT-022** (consolidation subcommand): `claude-solo` — tight scope, clear API shape already fixed by 25eabc8. 1h.
- **FEAT-023** (intentional_mirrors drift): `claude-solo + self-review` — drift code is well-understood and `[consolidation]` plumbing already exists; self-review helpful because cross-project edge accounting is fiddly. 45m.
- **FEAT-024** (pr-summary): `claude-solo` — trivial annotation strip. 30m.
- **DOC-001**: `claude-solo` — pure docs. 20m.
- **CHORE-002** (FEAT-021 unblock): `claude-solo` — 20m, quick.

## Context Budget Plan

- **Start of next session**: brief + CLAUDE.md + commit `25eabc8` body + target task's tasknote ≈ 5k tok
- `/clear` not needed for single-task sessions. For multi-task sessions combining FEAT-022 + FEAT-024 + DOC-001 (~1h50m), consider /clear between tasks since their code areas don't overlap.
