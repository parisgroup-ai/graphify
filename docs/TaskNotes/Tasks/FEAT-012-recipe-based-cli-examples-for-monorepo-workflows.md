---
uid: feat-012
status: done
completed: 2026-04-13
priority: low
timeEstimate: 120
tags:
  - task
  - feature
  - docs
  - dx
  - cli
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - docs
  - cli
  - onboarding
---

# feat(docs): Add recipe-based CLI examples for common monorepo workflows

## Description

`graphify --help` documents the command surface well, but it does not show practical end-to-end workflows for real monorepos. Users still need to infer how `run`, `query`, `explain`, `path`, and `diff` fit together in day-to-day architecture work.

## Motivation

During real usage on the ToStudy monorepo, the useful command sequence was:

```bash
graphify run --config graphify.toml
graphify query 'src.app.*study-chat*' --project web
graphify explain 'src.shared.domain.errors' --project pkg-api
graphify path 'src.hooks' 'src.trpc.react' --project web
cp report/web/analysis.json /tmp/web-before.json
graphify run --config graphify.toml
graphify diff --baseline /tmp/web-before.json --config graphify.toml --project web
```

Each command is individually discoverable via `--help`, but the workflow is not. This slows down adoption and makes the tool feel more exploratory than operational.

## Proposed Outcome

Document 4-6 concrete recipes such as:

1. Full monorepo refresh
2. Investigate a hotspot before refactoring
3. Trace dependency path between two nodes
4. Compare architectural drift before/after a refactor
5. Query a namespace or route-group quickly

## Affected Docs

- `README.md`
- optional: `graphify --help` / subcommand help epilogues
- optional: `docs/recipes.md` if examples become too large for the README

## Impact

- faster onboarding for real monorepos
- fewer trial-and-error invocations
- easier adoption of `query`, `explain`, `path`, and `diff`
- better bridge between CLI reference and architecture review workflow

## Verification (2026-04-13)

- Added a `Common Monorepo Recipes` section to `README.md`
- Documented 6 practical workflows covering `run`, `query`, `explain`, `path`, and `diff`
- Verified command examples against the current built CLI help for `run`, `query`, `explain`, `path`, and `diff`
