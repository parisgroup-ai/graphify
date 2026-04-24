---
uid: chore-008
status: done
priority: normal
scheduled: 2026-04-24
completed: 2026-04-24
timeEstimate: 60
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

## Goal

Dogfood, document, and release the new `graphify compare` CLI command.

## Scope

- Run `graphify compare` against real local Graphify outputs.
- Add a concise README recipe for comparing two analysis outputs.
- Update CHANGELOG and workspace version to v0.13.0.
- Keep Cargo.lock aligned.
- Run release gates, commit, tag, push, and verify the GitHub Release workflow.
