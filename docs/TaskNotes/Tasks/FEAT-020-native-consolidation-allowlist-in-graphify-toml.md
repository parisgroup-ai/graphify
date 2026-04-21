---
uid: feat-020
status: done
priority: normal
completed: 2026-04-18
pomodoros: 0
timeSpent: 45
timeEntries:
- date: 2026-04-18
  minutes: 45
  type: manual
  executor: claude-solo
  tokens: 72000
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- consolidation
- config
- dx
tags:
- task
- feat
---

# Native consolidation-allowlist in graphify.toml

> **Closed 2026-04-18.** Core slice landed in `25eabc8`; deferred subtasks shipped
> as [[FEAT-022]] (consolidation subcommand, `1be5225`), [[FEAT-023]]
> (intentional_mirrors drift suppression), [[FEAT-024]] (pr-summary hotspot
> annotations, `d6f916e`), and [[DOC-001]] (README migration note, `15fdccf`).
> The subtask checkboxes below remain unchecked as a historical record of the
> original scope before it was decomposed; the work itself is complete.

## Description

Consumers of Graphify (notably the `code-consolidation` skill running on
multi-project monorepos) routinely curate allow/ignore lists to suppress
known-intentional "duplicate" symbols — cross-language Pydantic↔TS
contracts, vendor-namespaced mirrors, DTO families — before surfacing
consolidation candidates to humans.

Today this list lives outside `graphify.toml` (per-skill ignore files,
bash filters), which means:

1. Every consumer reinvents the format.
2. `architecture_report.md` hotspot scores are not affected by the list.
3. Cross-project analyses keep double-counting the same intentional mirror.

Source: [parisgroup-ai/graphify#13](https://github.com/parisgroup-ai/graphify/issues/13)
(Proposal A — ranked by the reporter as "low effort, high value, ~1-2 days").

## Motivation

A ~24k LOC monorepo (16 projects, 23k nodes, 43k edges) analysis produced
1,912 raw consolidation candidates. A band-aid `--ignore-file` on the
consumer-side skill suppressed 74 of them (~4%). The remaining noise is
dominated by patterns that:

- are intentional (cross-language contracts, vendor namespacing)
- recur across runs
- deserve to live next to the other project config (`graphify.toml`)

A first-class `[consolidation]` section lets Graphify itself apply the
list uniformly to: consolidation reports, hotspot scoring, cross-project
summaries, and drift detection.

## Proposed Outcome

`graphify.toml` gains an optional `[consolidation]` section:

```toml
[consolidation]
# Regex patterns, anchored ^...$, matched against the leaf symbol name.
allowlist = [
  "TokenUsage",
  "LessonType",
  "(Guided|SemiGuided|Challenging)Exercise",
  "Anthropic.*",
  ".*(Response|Output|Json|Dto)",
]

[consolidation.intentional_mirrors]
# Optional: declare pairs to exclude from cross-project drift detection.
TokenUsage  = ["ana-service:app.models.tokens", "pkg-types:src.tokens"]
LessonType  = ["ana-service:app.models.pipeline_v3", "tostudy-core:src.LessonType"]
```

And new surface area:

- `analysis.json` gains `allowlisted_symbols: [...]` so consumers can
  respect the project's list without re-parsing TOML.
- `--ignore-allowlist` flag on existing commands (opt-out for debugging).
- New subcommand `graphify consolidation` that emits
  `consolidation-candidates.json` natively (replacing the bash+grep+python
  skill script currently shipped in `code-consolidation`).

Backward compatible: absent section = current behavior.

## Likely Scope

- Extend `graphify-cli` config parsing with `ConsolidationConfig`
  (optional, serde default).
- Pipe the config through the pipeline orchestrator to the report stage.
- Thread the allowlist into:
  - `crates/graphify-report/src/check_report.rs` (suppress flagged symbols from
    violations where appropriate)
  - `crates/graphify-report/src/pr_summary.rs` (strip from hotspot annotations)
  - A new consolidation module under `crates/graphify-report/src/` that
    renders `consolidation-candidates.json`
- New CLI subcommand `graphify consolidation --config graphify.toml`.
- New `--ignore-allowlist` flag on `run`, `check`, `pr-summary`.
- Documentation: README section + example block in `graphify.toml`
  emitted by `graphify init`.

## Subtasks

- [ ] Design `ConsolidationConfig` struct (allowlist: Vec<String>,
      intentional_mirrors: HashMap<String, Vec<String>>) and validation
      rules (regex compile at config-load time, fail fast).
- [ ] Wire config into pipeline; surface `allowlisted_symbols` in
      `analysis.json` (additive).
- [ ] Implement `graphify consolidation` subcommand + JSON output schema.
- [ ] Add `--ignore-allowlist` flag on `run`, `check`, `pr-summary`.
- [ ] Update `graphify init` template to include a commented
      `[consolidation]` block.
- [ ] Fixture tests: allowlist matches the leaf symbol only (no accidental
      substring hits); `intentional_mirrors` suppresses cross-project
      drift entries.
- [ ] README section + migration note for skill consumers on
      `.consolidation-ignore` → `graphify.toml`.

## Open Questions

1. Should the allowlist affect `architecture_report.md` hotspot scores
   (i.e., recompute centrality after removing allowlisted nodes), or only
   the consolidation-candidates output? The issue reporter implies both;
   the invasive path is "both", the safe path is "consolidation only".
2. `graphify consolidation` vs. config-only contract (skill opts in via
   `analysis.json` field). Reporter asked this explicitly — decision
   needed before starting.
3. Regex vs glob in `allowlist`. Regex is more expressive; glob is easier
   for non-engineers. Default proposal: regex anchored `^...$`.

## Notes

- This task covers **Proposal A only**. The barrel re-export collapse
  (Proposal B in the same issue) is tracked separately as
  [[FEAT-021-collapse-barrel-reexports-in-ts-extractor]].
- Local workaround already shipped in consumers: `.consolidation-ignore`
  file + `--ignore-file` flag in the `code-consolidation` skill script.
  Keep it working until this lands; deprecate once available.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-021-collapse-barrel-reexports-in-ts-extractor]] — Proposal B
  from the same issue, higher effort, lands later.
- [[FEAT-016-contract-drift-detection-between-orm-and-typescript]] —
  adjacent "intentional cross-language mirror" concern.
- GH issue: <https://github.com/parisgroup-ai/graphify/issues/13>
