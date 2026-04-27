# Session Brief — 2026-04-27 (FEAT-044 wave: Rust intra-crate pub use collapse)

## Last Session Summary

Dispatched the 4-slice plan that decomposed FEAT-044 (Rust re-export collapse) into FEAT-045 → FEAT-046 → FEAT-047 → FEAT-048. The first three landed clean (3 feat commits, 873 workspace tests / +4, 0 cycles, no hotspot regression). FEAT-048 ran Path B by design — its task body had a self-imposed gate (≥5 cross-crate misclassifications) that the workspace fails (1 hit, `src.Community`); the dispatch produced ADR 0002 documenting the deferral + re-open criteria instead of forcing implementation. Net: Rust intra-crate `pub use` is now collapsed canonically (mirrors TS FEAT-021/025/026); cross-crate is gated; type-alias collapse (`pub type`) is filed as FEAT-049 follow-up because the dogfood gap traces there, not to FEAT-047.

## Current State

- Branch: `main`, **4 commits ahead of `origin/main`** (push happens at session-close commit)
- Working tree: clean (after this close commit)
- Latest release: `v0.13.3` (unchanged — this wave is unreleased on `main`, will need a `v0.13.4` cut next session if a release feels warranted)
- TaskNotes: 79 total, 3 open (FEAT-047 still `open` despite all 4/4 subtasks ticked, FEAT-048 `open` as deferral artifact, FEAT-049 newly filed)
- `graphify suggest stubs` candidate count: **7, unchanged** — confirms the FEAT-047 dogfood AC was misattributed. The 2 sticky candidates that motivated FEAT-048/049 (`src.Community` cross-crate, `src.Cycle` type-alias) are exactly what those tasks own

## Commits This Session

`b9d476e..cc529ce` (4 commits, NOT pushed yet — close commit will push):

- `b9d476e` feat(extract): rust pub use → ReExportEntry emission (FEAT-045)
- `02c86f4` feat(extract): rust per-project pub-use canonical collapse (FEAT-046)
- `0c8a1c4` feat(extract): rust consumer-side use_aliases canonicalization (FEAT-047)
- `cc529ce` docs(adr): defer FEAT-048 — cross-crate pub use count below gate threshold

(plus the close commit landing now with task-tick markdown updates + CLAUDE.md learnings + this brief + FEAT-049 task file)

## Decisions Made (don't re-debate)

- **FEAT-046 went rename + widen** (`has_ts_reexport_work` → `has_reexport_work`, gated on `TypeScript || Rust`) over the alternative of duplicating the gate condition. Single source of truth for the build trigger; the language-specific resolver callbacks branch internally
- **FEAT-046 needed BOTH exact-match AND prefix-match edge rewrite** — discovered during implementation. Rust raw_targets aren't module-shaped pre-resolution, so a Calls edge to `Bar::new()` lands as `src.Bar.new` (not `src.Bar`); the canonical map keys on `src.Bar`, so a prefix-match shape (`src.Bar.new` → `src.foo.Bar.new`) is the load-bearing complement to the exact-match path. Helper `rewrite_via_barrel_prefix` (private, in `graphify-cli/src/main.rs`)
- **FEAT-047 returned `outcome: partial` despite all 4 subtasks done** — the work assigned to FEAT-047 IS complete (rewrite, ordering, integration test); the 4th AC ("dogfood `src.Community` drops") was a design-doc misattribution. `src.Community` is a cross-crate `pub use` (graphify-core → graphify-report), so it's genuinely FEAT-048 territory. Honest signal preserved: `partial` outcome with detailed note. The user has a pending decision on whether to close the task as `done` manually (work is complete) or leave it `open` until the AC is corrected
- **FEAT-048 went Path B (deferral) over Path A (full implementation)** — task body's own gate (subtask 1) requires ≥5 cross-crate misclassifications; workspace shows 1. Forcing Path A would have violated the task's own guard. ADR 0002 (`docs/adr/0002-cargo-workspace-reexport-graph-gate.md`) records the gate-check evidence + 3 re-open criteria. CHANGELOG `[Unreleased] ### Deferred` entry landed
- **FEAT-049 filed (low priority) instead of expanding FEAT-047 scope** — type aliases (`pub type Cycle = Vec<String>;`) are a separate detection problem (different tree-sitter node kind) from `pub use`. Body documents the option-1 (full pipeline, mirrors FEAT-045) vs option-2 (suggest-stubs filter) trade-off and leans option 1 with the same justification as FEAT-021/045 (data quality > reporting band-aid)
- **CLAUDE.md updated** with FEAT-045/046/047 facts as a contiguous block right after the TS FEAT-028 bullet, plus FEAT-048 deferral note + FEAT-049 follow-up note. Mirrors the existing TS narrative shape so the Rust path reads as the natural mirror

## Architectural Health

`graphify check --config graphify.toml` — all 5 projects PASS (run during FEAT-046 + FEAT-048 dispatches; baseline holds across the wave):

- 0 cycles introduced, no hotspot >0.85
- Workspace tests: **873 pass, 0 fail** (was 869 → +4 from FEAT-046 integration test + FEAT-047 unit + integration tests; FEAT-045 added 7 in its own commit but those landed before this baseline check)
- `graphify suggest stubs` candidate count: 7 (unchanged from session start — this is *correct* given the cross-crate / type-alias residue, not a regression)

## Open Items (3 follow-ups)

- **FEAT-047** (low, status=open): code complete; pending user decision on close-as-done vs leave-open until AC#4 of design doc gets corrected. Recommended action: `tn done FEAT-047` next session with note `AC#4 was FEAT-048 territory per session 2026-04-27 close`
- **FEAT-048** (low, status=open): deferred via ADR 0002, gate at ≥5 cross-crate misclassifications. Re-open trigger: workspace count crosses threshold OR a single high-edge cross-crate hit (~50+ edges) becomes user-visible. Subtasks 1+2 ticked, 3-7 deliberately open
- **FEAT-049** (low, status=open): type-alias collapse follow-up (`pub type X = Y;`). Surfaced by FEAT-047's dogfood gap analysis. 5 subtasks; recommend option 1 (full pipeline) over option 2 (suggest-stubs filter). Will close `src.Cycle` when it lands

## Suggested Next Steps

1. **Cut a `v0.13.4` release** — three meaningful feat commits since `v0.13.3` (FEAT-045/046/047) plus the FEAT-048 deferral ADR. Per CLAUDE.md release workflow: bump `[workspace.package].version` in root `Cargo.toml`, `cargo build --release -p graphify-cli`, commit, `git tag v0.13.4 <SHA>` (explicit SHA, not HEAD), `git push origin main --tags`, then `cargo install --path crates/graphify-cli --force` to refresh local PATH binary. CHANGELOG already has an `[Unreleased]` block — promote it to `[0.13.4] - 2026-04-27`
2. **Close FEAT-047** — code is complete and committed. Run `tn done FEAT-047` with a note explaining the AC#4 misattribution. Cleans up the sprint count and stops the open-task table from carrying a misleading row
3. **FEAT-049** if scope feels right — additive, ~45m estimate, uses FEAT-046 plumbing if option 1. Closes the last visible dogfood candidate (`src.Cycle`) that traces to a fixable bug rather than gated future work

## Frozen Modules / Hot Spots

- `src.server` (graphify-mcp, 0.60) — duplicated CLI logic (known debt per CLAUDE.md)
- `src.policy` (graphify-core, 0.49) — unchanged this session
- `src.resolver` (graphify-extract, 0.43) — slight delta from FEAT-047's new public method `rewrite_use_alias_targets`; under threshold

## Notes for Future Reader

- The FEAT-044 design doc (at `docs/superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md`) overestimated FEAT-047's reach — its AC#4 attributed a cross-crate dogfood delta to FEAT-047 that was genuinely FEAT-048 territory. If you read the design doc when planning FEAT-049 or any successor, double-check the per-task AC against the FEAT-044 → FEAT-045/046/047/048 split before committing to acceptance criteria
- Cumulative subagent tokens this session: 463k (4 dispatches). Hook fired 2x (cwd-mismatch issue still active for first dispatch — known). The 463k is calibration signal, not capacity — per CHORE-005 the model context window is the actual constraint
