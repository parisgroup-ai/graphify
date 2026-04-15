---
uid: feat-015
status: done
priority: normal
completed: 2026-04-15
timeEstimate: 720
pomodoros: 0
projects:
- '[[sprint.md|Backlog]]'
contexts:
- dx
- github
- editor
tags:
- task
- feature
- dx
- integration
---

# PR and editor integration for architecture feedback

## Description

Surface Graphify findings closer to the developer workflow, especially in pull requests and editor-assisted review loops.

## Motivation

Even good architecture insights get ignored if they only live in generated reports. The product becomes more useful when the right finding appears in the place where a developer is already making decisions.

## Proposed Outcome

Potential integration targets:

1. pull request summaries with hotspot and drift highlights
2. inline CI annotations for rule or path violations
3. editor or assistant workflows that query the graph while refactoring
4. links from findings back to `explain`, `path`, or report artifacts

## Likely Scope

- choose the first integration surface with the best leverage
- map Graphify output into PR-appropriate summaries
- define how findings are linked back to local CLI usage
- keep the implementation lightweight and automation-friendly

## Subtasks

- [x] Choose the first integration target and narrow scope — CLI-only `graphify pr-summary <DIR>` (2026-04-14)
- [x] Define the output contract for PR/editor consumption — spec + plan at `docs/superpowers/specs/2026-04-14-feat-015-pr-summary-cli-design.md`
- [x] Implement summary generation or annotation formatting — shipped in 17 commits (`be449dc..b5ebed5`)
- [x] Test the workflow against real repository fixtures — integration fixture at `crates/graphify-cli/tests/fixtures/pr_summary/`, 27-assertion e2e test
- [x] Document setup for GitHub and assistant-driven workflows — README recipe + command/artifact table rows

## Notes

This should follow rules or drift features rather than precede them. The delivery mechanism matters less than the quality of the signal being delivered.

## Verification (Session 8, 2026-04-14)

**Implementation feature-complete on `main`.** All 17 planned tasks landed via subagent-driven TDD, plus 3 re-review fixes (Tasks 9, 11, 16).

Key commits:
- `be449dc` — design spec + implementation plan
- `1288603..ab33851` — Deserialize derives + CheckReport type-move (Tasks 1–4)
- `28cc3a3` — `graphify check` writes `check-report.json` (Task 5)
- `4828eea..92fede2` — pure renderer in `graphify-report/src/pr_summary.rs` (Tasks 6–12)
- `13022bf..e6f1714` — CLI `Commands::PrSummary` + error paths + e2e fixtures (Tasks 13–16)
- `cd2421e` — README recipe + artifact/command rows (Task 17)
- `b5ebed5` — spec alignment: exit 1 convention (documentation)

Test state: 442 workspace tests passing, clippy clean on `graphify-report` and `graphify-cli`.

Remaining for close-out session:
- Move status to `done` + set `completed: 2026-04-??`
- Update `docs/TaskNotes/Tasks/sprint.md` (move FEAT-015 to Done)
- Bump `[workspace.package].version` in `Cargo.toml` to `0.6.0`
- Tag `v0.6.0` + push tag (triggers CI release for 4 targets)
