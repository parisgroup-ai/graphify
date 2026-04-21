---
uid: doc-001
status: done
priority: low
scheduled: 2026-04-18
completed: 2026-04-18
timeEstimate: 20
pomodoros: 0
timeSpent: 7
timeEntries:
- date: 2026-04-18
  minutes: 7
  type: manual
  executor: claude-solo
  tokens: 48000
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- consolidation
- docs
tags:
- task
- doc
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# docs(consolidation): README section + migration note for .consolidation-ignore to graphify.toml

Add a short migration subsection to the README's existing "Consolidation
Candidates" block explaining how to move hand-rolled ignore lists
(previously maintained in shell scripts or ad-hoc files) into the
native `[consolidation].allowlist` / `[consolidation.intentional_mirrors]`
sections of `graphify.toml`.

## Description

FEAT-020 landed the `[consolidation]` section as the first native
config for filtering consolidation noise (commit `25eabc8`);
FEAT-023 then extended `intentional_mirrors` to drive hotspot
annotations in the drift report (commit `700b5ce`). The README's
existing "Consolidation Candidates" section (already shipped in
FEAT-022) mentions the allowlist and `--ignore-allowlist` but does
**not** explain how users who managed exclusions before the native
config (via `graphify-consolidation-scan.sh` in the `code-consolidation`
skill, or via CI grep-excludes, or via any `.consolidation-ignore`-style
convention they rolled themselves) should migrate those patterns.

This task adds a brief "Migrating from pre-FEAT-020 exclusion lists"
subsection under the existing Consolidation Candidates section with
concrete before/after snippets.

## Proposed Content

Three short paragraphs inside the existing section, covering:

1. The three sources exclusions used to live in (the shell script
   from the `code-consolidation` skill, CI grep-excludes, and any
   local convention files) — acknowledge the variety without
   endorsing any particular legacy format.
2. The two canonical destinations in `graphify.toml`:
   - `[consolidation].allowlist` — regex patterns anchored against
     the *leaf* symbol name, for "known-duplicate-by-design" shapes
     like `TokenUsage` or `LessonType`.
   - `[consolidation.intentional_mirrors]` — explicit
     symbol-name → endpoint-list map, for shared-contract DTOs that
     legitimately co-exist across multiple projects.
3. One before/after example showing a grep-exclude pattern becoming
   an allowlist regex, and one showing a hand-written ignore list
   becoming an `intentional_mirrors` entry.

## Subtasks

- [ ] Add a "Migrating from pre-FEAT-020 exclusion lists"
      subsection to the Consolidation Candidates block in the
      README, placed immediately after the `--ignore-allowlist`
      paragraph and before the "supersedes the
      `graphify-consolidation-scan.sh`" note.
- [ ] Include one before/after grep-exclude → `allowlist` snippet.
- [ ] Include one before/after hand-rolled ignore →
      `intentional_mirrors` snippet.
- [ ] Link the new subsection to the `graphify consolidation`
      and `graphify check` command references already in the
      README for context.
- [ ] Cross-check the snippet syntax against the real
      `[consolidation]` shape documented in `CLAUDE.md`
      (regex anchored `^…$`, leaf-name match, fail-fast validation
      at config load).
- [ ] Proofread against the repo's doc voice — terse, example-led,
      no marketing language.

## Acceptance Criteria

- README renders correctly on GitHub (mermaid-free, no broken
  anchors).
- The new subsection fits within the existing Consolidation
  Candidates section without breaking the surrounding structure.
- Snippets are copy-pasteable into a real `graphify.toml` and
  would pass the fail-fast regex validation on config load.

## Notes

- This is a docs-only task. No code changes. No CI gates to
  re-run beyond a local preview of the README.
- Do NOT introduce a new `.consolidation-ignore` file format.
  The native config is the endpoint; the task title's mention of
  that filename is historical framing, not a format being added.
- If the section starts ballooning past ~40 lines, move the
  long-form migration into a dedicated file under
  `docs/migrations/` and leave a two-line pointer in the README.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-020-native-consolidation-allowlist-in-graphify-toml]] —
  the consolidation config this task documents migrating *into*.
- [[FEAT-022-feat-cli-graphify-consolidation-subcommand-emits-consolidation-candidates-json]]
  — introduced the existing Consolidation Candidates README
  section; this task extends it.
- [[FEAT-023-feat-drift-honour-consolidation-intentional-mirrors-to-suppress-cross-project-drift-entries]]
  — motivation for the `intentional_mirrors` migration path.
