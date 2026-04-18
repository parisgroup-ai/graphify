---
uid: feat-024
status: open
priority: low
scheduled: 2026-04-18
timeEstimate: 30
pomodoros: 0
contexts:
- consolidation
- pr-summary
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

# feat(pr-summary): integrate consolidation allowlist into pr-summary hotspot annotations

Extend the `graphify pr-summary` renderer so hotspot entries matching
either `[consolidation].allowlist` or `[consolidation.intentional_mirrors]`
are annotated as such in the Markdown output. Mirrors the treatment
FEAT-023 introduced for the drift report, giving PR reviewers the
same at-a-glance "this is intentional" signal.

## Description

FEAT-023 (commit `700b5ce`) taught the drift pipeline to honour
`[consolidation.intentional_mirrors]` when annotating cross-project
hotspot entries — a mirror symbol no longer fires one "rising
hotspot" per project that carries it. That same annotation pass has
not yet been wired into `graphify pr-summary`, so the PR-ready
Markdown report still treats mirror / allowlisted symbols as if
they were ordinary hotspots worth a reviewer's time.

This task propagates the annotation logic from the drift renderer
into the `pr-summary` renderer, preserving the pure-function shape
(inputs: `analysis.json`, `drift-report.json`, `check-report.json`;
output: Markdown to stdout).

## Motivation

Reviewers skimming a PR summary should be able to ignore "hotspots"
that are known-intentional mirrors at a glance. Without this, every
upstream bump to a shared DTO produces N hotspot lines in the PR
summary (one per project that mirrors it), each of which a reviewer
has to triage manually. The allowlist data already exists; the
renderer just doesn't read it.

## Likely Scope

**1. Renderer input.**
The `pr-summary` renderer (`render(project_name, analysis, drift,
check)` — CLAUDE.md calls this out as the public signature) needs
access to the project's `ConsolidationConfig` to know which symbols
are allowlisted. Two reasonable paths:
   - Thread the `ConsolidationConfig` through as a new optional
     argument alongside `drift` and `check`.
   - OR fold the allowlist membership into a precomputed field on
     the `analysis.json` / `check-report.json` so the renderer
     stays pure over what it already reads.

   The CLAUDE.md note on FEAT-020 says `analysis.json` already
   gains `allowlisted_symbols: [...]` when a `[consolidation]`
   section is present. Prefer reading from there — zero new
   plumbing, and the renderer stays decoupled from TOML parsing.

**2. Annotation rule.**
For every hotspot entry the renderer emits (top-N list, drift
"rising hotspots" section if present in the same output):
   - If the node's leaf symbol name matches a pattern in
     `analysis.allowlisted_symbols`, append ` [allowlisted]` to
     the entry.
   - If the node appears as an endpoint in
     `drift.intentional_mirror_matches` (the structure FEAT-023
     writes to the drift report), append ` [intentional mirror]`
     instead (more specific annotation wins).
   - Both annotations are tail-appended so the entry's primary
     content (symbol name, score, delta) stays leftmost and
     scannable.

**3. Scope discipline.**
No changes to what `graphify check` does (already handled by
FEAT-020). No changes to the drift report (already handled by
FEAT-023). This task touches only the `pr-summary` renderer and
its fixture coverage.

## Subtasks

- [ ] Locate the `pr-summary` renderer module in the
      `graphify-report` crate and confirm it currently has no
      awareness of allowlisted_symbols.
- [ ] Read `allowlisted_symbols` from the `analysis` input; handle
      its absence gracefully (legacy JSON shape, no `[consolidation]`
      section — just skip the annotation pass).
- [ ] Read intentional-mirror matches from the `drift` input when
      `drift` is `Some`.
- [ ] Apply the append-annotation rule to every hotspot entry the
      renderer emits.
- [ ] Unit fixture: hotspot with allowlist match gets the
      `[allowlisted]` tail annotation.
- [ ] Unit fixture: hotspot that's an intentional-mirror endpoint
      gets the `[intentional mirror]` tail annotation (wins over
      `[allowlisted]` when both apply).
- [ ] Unit fixture: hotspot with neither match renders unchanged
      (regression guard).
- [ ] Unit fixture: legacy `analysis.json` without
      `allowlisted_symbols` renders unchanged (backward-compat
      guard).
- [ ] Update the `pr-summary` section of the README (or the
      command's help text) to document the new tail annotations so
      reviewers know what `[allowlisted]` means.

## Acceptance Criteria

- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace -- -D warnings` passes.
- `cargo test --workspace` passes, including the four new fixtures.
- A hand-run of `graphify pr-summary` against a project with an
  `[consolidation]` section emits at least one `[allowlisted]`
  annotation on a known-allowlisted hotspot.
- A hand-run against a project *without* an `[consolidation]`
  section produces byte-identical output to pre-change (legacy
  compatibility).

## Notes

- Prereq: FEAT-020 (commit `25eabc8`) — `allowlisted_symbols` in
  `analysis.json` — and FEAT-023 (commit `700b5ce`) — the
  intentional-mirror matches structure in the drift report.
- The CLAUDE.md note is explicit that `pr-summary`'s exit codes
  are non-gating (`0` on success even with warnings). This task
  must preserve that — annotations never change the exit code.
- Keep the renderer pure. No new I/O beyond what `render` already
  reads from its typed inputs.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]] —
  source of the `allowlisted_symbols` field this task consumes.
- [[FEAT-023-feat-drift-honour-consolidation-intentional-mirrors-to-suppress-cross-project-drift-entries]]
  — the drift-side counterpart whose annotation rule this task
  mirrors.
- [[FEAT-015-pr-summary-cli]] — original pr-summary CLI design.
