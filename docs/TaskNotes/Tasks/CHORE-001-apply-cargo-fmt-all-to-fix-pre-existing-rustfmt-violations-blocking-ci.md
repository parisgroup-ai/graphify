---
uid: chore-001
status: open
priority: normal
scheduled: 2026-04-15
pomodoros: 0
contexts:
- ci
- fmt
- feat-018
tags:
- task
- chore
---

# Apply cargo fmt --all to fix pre-existing rustfmt violations blocking CI

## Description

`cargo fmt --all -- --check` has been failing on `main` since FEAT-018
(commit `06660f9` et al) introduced new files in `crates/graphify-cli/src/install/`
without running `cargo fmt`. The v0.7.0 prep CI run (`24447255168`) and every
subsequent run have failed on the `fmt` step. My session on 2026-04-15
(commits `79c24d3` + `b1ce4d9`) cleared the `clippy` gate but the `fmt` gate
remains red.

## Scope

Run `cargo fmt --all` at the repo root. Commit as
`chore(fmt): apply cargo fmt --all (post-FEAT-018 cleanup)`. No behavior
changes — purely mechanical reformatting.

## Affected files (as of 2026-04-15)

- `crates/graphify-cli/src/install/copy_plan.rs` (5 spots — struct-field line breaks)
- `crates/graphify-cli/src/install/frontmatter.rs`
- `crates/graphify-cli/src/install/mcp_merge.rs` (~9 spots)
- `crates/graphify-cli/src/install/mod.rs` (~6 spots)
- `crates/graphify-cli/src/main.rs` (5 spots — some from session 2026-04-15
  BUG-014 eprintln! block where my lines exceed 100 chars; rustfmt will
  refactor them)
- `crates/graphify-cli/tests/install_integrations.rs` (1 spot)
- `crates/graphify-core/src/metrics.rs` (2 spots — `pub fn classify`
  signature + test formatting)
- `crates/graphify-report/src/json.rs` (1 spot — import reorganization)

## Subtasks

- [ ] `cargo fmt --all` at repo root
- [ ] `cargo fmt --all -- --check` → should be clean
- [ ] `cargo test --workspace` → all 493 tests still pass
- [ ] `cargo clippy --workspace -- -D warnings` → still clean
- [ ] Commit as `chore(fmt): apply cargo fmt --all`
- [ ] Push → CI should flip green
- [ ] Consider adding a pre-commit hook or CI-auto-fmt check to prevent recurrence

## Notes

- Pre-existing failures were introduced before this session — FEAT-017/018 work
  did not run `cargo fmt` before committing.
- My BUG-014 commit introduced 3 over-width `eprintln!` lines in `main.rs`
  that rustfmt will also fix in the same sweep; fine to bundle.
- If you want to split: (a) fix my new lines only, then (b) a separate commit
  for pre-existing. But since `cargo fmt --all` is atomic and pure-mechanical,
  one commit is cleanest.

## Related

- [[BUG-014-graphify-diff-before-history-json-fails-with-cryptic-schema-error]] — introduced the 3 over-width eprintln! lines in main.rs
- [[sprint]] - Current sprint
- [[activeContext]] - Active context
