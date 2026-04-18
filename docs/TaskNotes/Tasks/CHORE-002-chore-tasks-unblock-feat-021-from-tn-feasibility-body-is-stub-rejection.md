---
uid: chore-002
status: open
priority: low
scheduled: 2026-04-18
timeEstimate: 20
pomodoros: 0
contexts:
- tn
- tasknotes
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# chore(tasks): unblock FEAT-021 from tn feasibility 'body is stub' rejection

FEAT-021 kept being rejected by `tn session start --plan-only` as
`feasibility_check_failed: body is stub` despite having a fully authored
Description / Motivation / Likely Scope / Subtasks body. This task
records the root cause and the unblock workaround so the pattern
doesn't re-bite future rich-bodied tasks.

## Description

`tasknotes-core::task::is_stub_body_str`
(defined in the `tasknotes-core` crate of the sibling `tasknotes-cli`
v0.3.0 workspace, alongside the preflight module)
walks the task body and sets `in_description = true` when it hits a
`# ` heading, then flips it to `false` on the first `## …` heading and
**never flips it back**. Only lines captured while `in_description` is
true count toward the "description" buffer; an empty buffer after trim
returns `true` (stub).

FEAT-021 followed the convention of putting all prose under
`## Description` / `## Motivation` / `## Likely Scope`, leaving the
narrow strip between `# Title` and the first `## …` effectively empty.
The heuristic therefore classified it as a stub. FEAT-023 has the
identical structure but slipped past because it was logged manually via
`tn log`, bypassing the planner's feasibility gate.

Diagnosis confirmed by hand-simulating the algorithm on both files:
the captured description buffer contained only a single blank line in
each case.

## Subtasks

- [x] Locate the feasibility check in tasknotes-cli source
      (the `preflight` module in `tasknotes-core`, which delegates to
      `tasknotes-core::task::is_stub_body_str`).
- [x] Reproduce rejection by walking the heuristic against the actual
      FEAT-021 body; confirm "description zone" ends up empty.
- [x] Apply local unblock: add a 2-3 line TL;DR paragraph between
      `# Title` and `## Description` in FEAT-021 so the heuristic sees
      real prose.
- [ ] (Follow-up, tasknotes-cli repo) Patch `is_stub_body_str` to also
      count content under a `## Description` section as description.
      Track as a separate task in that repo — not blocking graphify.
- [x] (Follow-up, graphify) Update the task-authoring note in
      `CLAUDE.md` to mention the TL;DR requirement for tasks that will
      be dispatched through `/tn-plan-session`.

## Notes

- The current `tn new` template still emits the convention-following
  shape (`# Title` → blank → `## Description` → `Description here.`),
  so every task created from scratch is born either stub-shaped or
  structure-trapped once real content moves into `## Description`.
  The durable fix is the tn-side patch listed in the subtasks.
- Cheap authoring rule in the meantime: write a one- or two-sentence
  TL;DR immediately under `# Title`, then the usual `## Description`
  with full context.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
- [[FEAT-021-collapse-barrel-reexports-in-ts-extractor]] — the task
  that was being blocked; now carries a TL;DR paragraph.
- [[FEAT-023-feat-drift-honour-consolidation-intentional-mirrors-to-suppress-cross-project-drift-entries]]
  — same body shape, survived the gate only because it was logged
  manually.
