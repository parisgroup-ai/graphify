---
uid: feat-015
status: open
priority: normal
timeEstimate: 720
tags:
  - task
  - feature
  - dx
  - integration
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - dx
  - github
  - editor
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

- [ ] Choose the first integration target and narrow scope
- [ ] Define the output contract for PR/editor consumption
- [ ] Implement summary generation or annotation formatting
- [ ] Test the workflow against real repository fixtures
- [ ] Document setup for GitHub and assistant-driven workflows

## Notes

This should follow rules or drift features rather than precede them. The delivery mechanism matters less than the quality of the signal being delivered.
