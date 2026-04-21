---
uid: chore-006
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 20
pomodoros: 0
timeSpent: 0
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- repo
- gitignore
- rust
- dx
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  estimateTokens: 25000
  hintsInferred: false
---

# chore(repo): untrack target/ so rust build artifacts stop showing as dirty

`/target` is in `.gitignore` (line 2) but 12,614 files under `target/` are tracked because a pre-existing `chore: checkpoint current workspace state` commit (`7fb3040`) accepted them before the ignore rule matured. Git ignore does not retroactively untrack, so every local build flips the same files to modified, polluting `git status`, forcing explicit staging on every commit, and inflating `.git/` (currently 412M vs the repo's real text weight). This chore removes the tracked copies.

## Description

Observed during session `2026-04-20-1811` (post-v0.11.0 tidy-up). After `cargo build --release -p graphify-cli`, `git status --short` reports:

```
 M target/.rustc_info.json
 M target/debug/graphify
 M target/debug/graphify.d
 M target/debug/libgraphify_report.d
 M target/debug/libgraphify_report.rlib
 M target/release/graphify
 M target/release/graphify.d
```

None of those are source. Each commit in this repo since the checkpoint has had to use explicit path staging (`git add <file>` instead of `git add -A`) to avoid accidentally re-checking-in a specific build output. The commit-push skill's "multi-session guard" step has also had to flag these as "unknown" on every invocation (observed today in the `chore(tasks): draft FEAT-029/FEAT-030` commit).

`git ls-files target/ | wc -l` → **12,614 files** tracked. `du -sh target/` → 24G on disk (current build). `du -sh .git/` → 412M, inflated by the two historical commits that touched target/ (`7fb3040` and `4a7f594`). Only two commits in the entire history mention target/, so the blast radius in history is small and a clean untrack is safe.

## Motivation

- Every solo-dev commit currently requires the multi-session guard to sort "mine" from "unknown" because the target/ noise floor is non-zero. A clean status means `git add -A` becomes safe again (the skill can relax that specific rule for this repo).
- The commit-push skill's "Files still modified that you did NOT include" footer has to flag the same 7 target/ files on every single session — that signal becomes noise and real "another session touched this" warnings get ignored by habituation.
- `.git/` at 412M is an order of magnitude over what a Rust CLI with this much source should carry. Fresh clones pay the cost.
- A `.gitignore` rule that doesn't actually take effect is a working-as-intended trap for contributors (and for future Claude sessions). Better to have the ignore rule match reality.

## Likely scope

1. Confirm no tagged release references target/ binaries in its commit. (Spot-checked — `v0.11.0` @ `d0f1a3f` touched source/Cargo only.) Releases build fresh from `cargo` in CI (`.github/workflows/release.yml`), so removing tracked binaries does not break release artifacts.
2. Run `git rm -r --cached target/` — removes from the index but leaves working-tree files intact. Verify with `git status --short` that untracked target/ files do NOT appear (they should stay hidden because `/target` matches in .gitignore).
3. Commit with a conventional message like `chore(repo): untrack target/ now that .gitignore matches` and push. One commit, nothing sneaky.
4. Communicate the change if there are concurrent clones of this repo (there shouldn't be any on another contributor's machine since this is solo-dev — but if the user has another worktree / machine checkout, warn them they'll need to `git pull` and tolerate a big "deleted" set). No action required from users who `cargo build` after the pull; their working-tree target/ stays put and stays untracked.
5. Optional post-cleanup: run `git gc --aggressive --prune=now` to reclaim the 400M-ish that the packfile is holding for the historical blobs. Skip if it means rewriting history that remotes might disagree with — this is pure local compaction against the existing commit graph, so safe.

## Boundaries / non-goals for v1

- Does NOT rewrite history with `git filter-repo` / BFG to purge target/ from past commits. The two touching commits stay. Reason: remote history is already published (v0.11.0 tagged), rewriting would invalidate tag SHAs and break anyone who has pulled. Historical blob weight becomes unreachable after the untrack commit ages out — `gc` picks it up eventually.
- Does NOT change `.gitignore`. The existing `/target` rule is already correct; the problem is historical commits, not the ignore rule.
- Does NOT touch CI. The release workflow's `cargo build --release` path doesn't depend on tracked binaries.

## Acceptance criteria

- After the commit pushes, `git ls-files target/` returns an empty list.
- After `cargo build --release -p graphify-cli`, `git status --short` shows an empty target/ modification list.
- The commit-push skill's "Files still modified that you did NOT include" footer reports zero entries on a clean post-build repo.
- `.git/` size is reduced after `git gc` (if step 5 runs) — target a reduction of at least 200M.
- No regressions in `cargo test --workspace` (target/ is an output directory, not an input — no test should depend on its pre-existing contents).

## Related

- [[sprint]] — Current sprint
- [[activeContext]] — Active context
- `7fb3040` — the original `chore: checkpoint current workspace state` commit that introduced tracked target/ files
- `4a7f594` — second commit touching target/ (pre-dating `.gitignore` maturation)
- FEAT-019 / commit-push skill — the downstream consumer whose "multi-session guard" has to special-case target/ noise
