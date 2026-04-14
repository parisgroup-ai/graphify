---
uid: feat-013
status: done
completed: 2026-04-13
priority: high
timeEstimate: 960
tags:
  - task
  - feature
  - architecture
  - ci
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - architecture
  - policy
  - ci
---

# Policy-driven architecture rules

## Description

Add declarative architecture rules so teams can enforce project-specific boundaries on top of the extracted dependency graph.

## Motivation

`graphify` already explains the current architecture well, but it still stops at observation. The highest-leverage next step is letting teams encode what the architecture is allowed to be, then fail CI when those rules are violated.

## Proposed Outcome

Support rules such as:

1. Namespace A cannot import namespace B
2. Only selected layers can depend on infra or config modules
3. Feature modules cannot depend on each other directly
4. Cross-project dependencies can be forbidden or explicitly allowlisted
5. Fan-in or fan-out thresholds can be enforced per namespace

## Likely Scope

- config format for rules in `graphify.toml`
- rule evaluation engine on top of the existing graph
- CLI reporting through `graphify check`
- machine-readable output for CI
- tests covering positive and negative cases
- documentation with monorepo examples

## Subtasks

- [x] Define the minimum viable rule model and syntax
- [x] Decide which existing CLI surface owns rule evaluation
- [x] Implement rule matching against nodes, namespaces, and projects
- [x] Add violation reporting for human and JSON output
- [x] Cover rules with integration tests and fixtures
- [x] Document recommended rule recipes

## Notes

This should go through the brainstorming/spec flow before implementation. It is the most strategic feature because it turns Graphify from analysis into enforceable architecture governance.

## Verification (2026-04-13)

- Verified `graphify check` evaluates declarative policy rules on top of existing quality gates.
- Verified README now documents `[[policy.group]]`, `[[policy.rule]]`, selectors, partitions, and CI usage examples.
- Fixed the integration-test harness to build the current `graphify` binary before executing CLI assertions, eliminating stale-binary false negatives.
- Verified with `cargo test` from the workspace root.
