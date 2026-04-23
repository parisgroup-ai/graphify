# Session Brief — Next Session (post-2026-04-22 evening, FEAT-038 + BUG-020 close)

**Last session:** 2026-04-22 evening (tn session `2026-04-22-2340`, 40m / 60m budget). Picked up the two remaining open tasks via `/tn-plan-session` from the prior session brief's recommendation: FEAT-038 (Leiden refinement + spectral bisection) and BUG-020 (external_stubs cache investigation). Both shipped — FEAT-038 with a 4-stage split cascade landing `c32f5a3`, BUG-020 closed as **not reproducible on v0.12.0** with a re-open evidence checklist added to the task body. Backlog is now empty (0 open tasks).

## Current State

- **Graphify**: branch `main`, **2 unpushed commits** (`b4c068b`, `c32f5a3`), 2 unstaged task-body files awaiting the session-close commit. Version **0.12.0** on PATH (from prior session's release).
- **Task state**: `tn sprint summary` = 61 total / 0 open / 0 in-progress / 61 done. Sprint board is empty for the first time since late February.
- **Local binaries**: `graphify 0.12.0`, `tn 0.5.9`.
- **Architectural health (`graphify check`)**: all 5 projects PASS, 0 cycles. Max_hotspot **0.559 (`src.server` in graphify-mcp)** — unchanged from prior session. `src.pr_summary` at 0.433, `src.policy` at 0.486, `src.resolver` at 0.441, `src.install.codex_bridge` at 0.478. Top 5 hotspots identical session-over-session despite the community reshape — confirms FEAT-038's community-split work is orthogonal to betweenness/PageRank scoring.
- **Test tally**: 478 → **487 passed** (+9 net-new tests in `graphify-core::community`). 0 failed. `cargo fmt --all -- --check` clean. `cargo clippy --workspace -- -D warnings` clean.

## What shipped this session

**FEAT-038 — Leiden refinement + greedy bisection fallback (c32f5a3, 437 LOC, 9 new tests)**

Extended `split_oversized()` in `crates/graphify-core/src/community.rs` with a two-stage cascade that fires when both Louvain and label-propagation sub-passes collapse to a single sub-label:

1. **`leiden_refine()`** — Leiden-style constrained refinement. Three structural differences from Louvain: strictly-positive gain gate (`delta > 0.0`, not `>= 0.0` — prevents tied-gain collapse); well-connectedness gate (`k_i_in > k_i * k_c / (2m) + epsilon` — rejects moves below configuration-model expectation); singleton initialization on the induced subgraph.
2. **`greedy_modularity_bisection()`** — deterministic final fallback. Guarantees a 2-way split of any community with ≥1 intra-edge. Seeds group A from lowest-degree node (tie-break: smallest index), grows A by highest-edge-weight frontier until A's degree sum ≥ half the community total.

Cascade order: Louvain → label-prop → leiden_refine → greedy_modularity_bisection. Each step short-circuits when it produces ≥2 distinct sub-labels.

**Self-dogfood results (all 3 previously-unsplittable communities split):**
- `graphify-cli 197 → 195 + 2` (cohesion 0.010 → 0.000 + 1.000)
- `graphify-mcp 59 → 57 + 2` (0.034 → 0.000 + 1.000)
- `graphify-mcp 42 → 40 + 2` (0.048 → 0.000 + 1.000)
- `graphify-report 75 → 38+...` (finer subdivision via leiden_refine — not just bimodal)

**BUG-020 — investigation closed as "not reproducible on v0.12.0"**

Four fixtures failed to reproduce the reported symptom, including a `/tmp/gf-bug020-verify` throwaway with the cache intentionally hot. Architectural root cause of non-reproducibility: `ExternalStubs::matches()` fires in the edge-resolution loop on *every* edge regardless of cache provenance; the cache key (`{version, local_prefix, per-file SHA256}`) never stores classification. Latent hypothesis for the consumer: shadowed pre-0.11.7 binary silently ignoring `[settings].external_stubs` (before FEAT-034's merge layer landed). Task body carries a re-open evidence checklist.

## Decisions Made (don't re-debate)

- **Leiden before greedy-bisection in the cascade, not after.** Leiden can produce >2-way splits when the community actually has richer sub-structure; greedy bisection is strictly 2-way by construction. The ordering matters when a 75-member community has 3 natural clusters — Leiden finds all three, bisection would collapse two of them together.
- **Strictly-positive gain gate is load-bearing.** `>= 0.0` re-introduces the FEAT-036 "everything drifts to label 0" tied-gain collapse. Verified with the `leiden_refine_rejects_negative_expected_gain` regression test.
- **Well-connectedness gate uses configuration-model expectation, not a raw threshold.** Matches the Leiden paper's innovation over Louvain literally. A candidate sub-community must "pull harder than chance" to accept a member.
- **Greedy bisection's degree-sum target is half the total, not exactly half the node count.** Degree-balanced splits respect weighted edges (stronger links weighted more); node-count balance ignores structure. Also guarantees termination — the target is always reachable on any connected component.
- **No feature flag on the cascade.** Happy path (Louvain succeeds on the first try) pays zero cost since each stage short-circuits. Adding a flag would complicate the call site for no runtime benefit.
- **Lopsided splits (195+2, 57+2, 40+2) are not a bug.** The pre-FEAT-038 unsplittable communities were dominated by `merge_singletons` step-(b) isolated-singletons buckets — they have no intra-community edges. Cohesion 0.000 on the 195-member remainder is truthful, not pathological. Documented in the commit body so no future contributor tries to "fix" it.
- **BUG-020 closed without a code change.** Investigation only. Consumer-side evidence was not reproducible on v0.12.0; architectural inspection confirmed the proposed root cause (classification leaking into cache) is architecturally impossible. Re-open checklist added to task body so a future report from any consumer can disambiguate in one round-trip without another open-investigation loop.
- **v0.12.0 release stays.** No version bump for FEAT-038 — the CLAUDE.md entry notes `no version bump`. User decision when to tag v0.12.1.

## Meta Learnings This Session

- **Heuristic tokens under-count observed by ~45% consistently.** Two data points this session (FEAT-038: 66k heuristic vs 111k observed, −41%; BUG-020: 35k heuristic vs 66k observed, −47%). Averaged across three paired samples now (including the one captured in `2026-04-20-1437`), the multiplicative under-count is stable at 0.55×. The dispatcher's rule #2 constants in `~/.claude/agents/tn-session-dispatcher.md` (20k baseline + 2.5k per Read + 4k per Write + 1k per Bash) would converge on observed usage if retuned to ~36k baseline + 4.5k per Read + 7k per Write + 1.8k per Bash. CHORE-011's `--note source=<usage>,heuristic=H,observed=M,delta_pct=D` grammar captured today's pair — the retune task is not ticketed yet but has clean input data whenever it lands.
- **BUG-020's architectural analysis method generalizes.** "Is this cache-invalidation bug architecturally possible?" is answered by tracing which fields the cache key covers vs which fields the feature depends on. If the feature's inputs are NOT in the key, the cache can't be at fault — the feature's apparent non-responsiveness has to come from somewhere else (shadowed binary, wrong file inspected, feature not actually wired). Worth recording as a debugging playbook for future "feature seems to not take effect without --force" reports.
- **The session journal is still absent** (9 consecutive sessions now). Would have been particularly useful this session — two distinct investigations running in parallel with their own findings, and the only structured record is this brief. Pattern is not self-correcting; consider starting the journal in the first dispatch turn of any future multi-task session.
- **tn CLI's `tn done` does not auto-run after `claude-solo` dispatches.** FEAT-038's dispatcher committed code but left the task status `open` — I had to run `tn done FEAT-038` manually mid-session-close. BUG-020's dispatcher DID run `tn done BUG-020` because I explicitly authorized it in the dispatch prompt (investigation close-out). The pattern needs codifying: either the dispatcher's claude-solo path always runs `tn done` on `outcome: done`, or the orchestrator always runs it after successful logging. Current "no session mutation" guardrail forbids the dispatcher; therefore the orchestrator should. Not ticketed yet — file if it repeats.

## Open Debts

- **16 unshared skills** in `~/.claude/skills/` — unchanged from last session. 0 modified, so nothing blocking, but the `.skills-sync-ignore` pass still deferred. (Now 9 sessions carry-over.)
- **`docs/TaskNotes/Tasks/sprint.md` has invalid frontmatter** (missing `uid` field) — `tn` silently skips it, warning on every command. Cosmetic, pre-existing, surfaces in every `/tn-plan-session` run. Worth a 2-minute fix next session.
- **Backlog is zero.** All three open tasks entering this session (FEAT-038, BUG-020, and the prior already-closed ones) are now done. Next session starts with NO automatic work surfaced by the planner — will require either a brainstorm cycle or an explicit decision on next territory.
- **Dispatcher heuristic retune not ticketed.** Three paired samples now exist (`--note source=<usage>,heuristic=...,observed=...,delta_pct=...`) confirming ~45% under-count. Adding a CHORE task would be cheap; leaving it until a 10-sample threshold (CHORE-011's convergence target) is also defensible.
- **Release 0.12.1 deferred.** FEAT-038 shipped on main but binary stays v0.12.0. Version bump + tag push + `cargo install --path …` is a one-session chore; defer until there's a second change that warrants the release cycle.
- **Stop hook still doesn't fire on Task subagent stops.** `hook_fired: false` across both dispatches this session. Known upstream limitation. The workaround (dispatcher estimates `tokens` in DISPATCH_RESULT + orchestrator reads `<usage>` and forwards to `tn session log --tokens`) is working correctly — all three meters (heuristic, observed, calibration) agree within the documented delta.

## Suggested Next Steps

1. **Brainstorm next territory.** Backlog is empty for the first time in months. Candidate directions: (a) new language support (Ruby, Java, C#); (b) integration target (GitHub Action marketplace listing, VS Code extension); (c) performance work (the self-dogfood `cargo run --release analyze` is sub-second but `--force` on a 1000-node mono-repo like `parisgroup-ai/cursos` takes ~15s; room for parallelization); (d) UX polish (`graphify explain` currently prints raw text; an interactive browsing mode could ship on rmcp's tool/prompt surface); (e) `graphify compare <PR1> <PR2>` as a cross-PR drift tool for reviewers auditing multiple PRs at once.
2. **Release v0.12.1** if any FEAT-038 consumer surfaces a need. Otherwise hold until a second change accumulates.
3. **Fix `sprint.md` frontmatter** — one `uid:` line added, silences the recurring warning. Smallest possible session.
4. **File the dispatcher heuristic retune CHORE** if you want to lock in the observed under-count before losing the paired-sample memory.

## Quick-start commands for the next session

```bash
# Orient
/session-start

# Option A — brainstorm new direction
# describe the candidate territory, then /tn-plan-session

# Option B — quick polish session
# fix sprint.md uid, tag v0.12.1 if motivated

# Option C — release cycle (only if a second change warrants it)
# edit Cargo.toml → 0.12.1, then:
cargo build --release -p graphify-cli
git commit -am "fix: bump version to 0.12.1"
git tag v0.12.1
git push origin main --tags
cargo install --path crates/graphify-cli --force
```
