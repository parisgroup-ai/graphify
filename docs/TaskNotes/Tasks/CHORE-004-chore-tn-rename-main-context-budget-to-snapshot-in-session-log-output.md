---
uid: chore-004
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 45
pomodoros: 0
timeSpent: 3
timeEntries:
- date: 2026-04-20
  minutes: 3
  note: source=<usage>; cross-repo — lives in parisgroup-ai/tasknotes-cli rust/crates/tasknotes-cli/src/commands/session.rs:887, dispatch from that repo instead
  type: manual
  executor: claude-solo
  tokens: 46474
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- tn
- dx
- logging
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  estimateTokens: 40000
  hintsInferred: false
---

# chore(tn): rename main-context budget to snapshot in session log output

The `tn session log` success line currently prints `main-context budget: X / Y` where X is `current_window_tokens` (a per-turn snapshot, per BUG-012/DOC-003), not a cumulative running budget. The word "budget" induces "exhaustion imminent" reading when X approaches Y — even though Y is a soft advisory ceiling, not enforcement. This chore renames the field to match its actual semantics and separates it from the subagent-sum line so the two meters stop sharing one denominator.

## Description

Observed during graphify session `2026-04-20-1437` (FEAT-028 multi-slice dispatch). The `tn session log` success line emits two meters in a single phrase:

```
✓ Subagent tokens: +64977 (total: 378788, main-context budget: 92349 / 400000).
```

Two distinct numbers share one denominator (`400000`):
- `total: 378788` — `subagent_tokens_sum` (cumulative across every dispatch, advisory for FEAT-019 calibration)
- `main-context budget: 92349` — `current_window_tokens` (snapshot of latest main-session turn, per BUG-012/DOC-003)

Reading them together invites two misreads:
1. "Budget X/Y at 95%" implies enforcement, but tn's `--tokens` is documented as soft/advisory (the main-context warning block in `/tn-plan-session` Step 7 explicitly says "the `--tokens N` budget ceiling will NOT be enforced").
2. Both numbers share the `/ 400000` tail visually, making them look like one accumulating meter.

## Motivation

- Live incident during FEAT-028 dispatch (2026-04-20): orchestrator misread `subagent_tokens_sum: 378788 / 400000` as "tokens nearly exhausted, must close session now" and pressed session close at ~91% model-context headroom still available. Skill `/tn-plan-session` Step 8 bullet 5 already forbids this ("Do NOT recommend close based on the token advisory alone"), but the wording of tn's output line actively pushes against that rule.
- BUG-012 + DOC-003 already corrected the field semantics in `tn session status --json` (snapshot vs accumulator). The human-readable log line wasn't updated in the same pass.
- Downstream consumers (skills, future dashboards) parse or at least reason about this line. A precise name makes misuse harder.

## Likely scope

1. Rename `main-context budget:` → `main-context snapshot:` in the `tn session log` success line. File: the session-log renderer in the tn CLI source (find by grepping for the literal string `main-context budget`).
2. Consider (optional) splitting the line into two statements so the two meters don't share a denominator visually:
   ```
   ✓ Subagent tokens: +64977 (cumulative: 378788, advisory: 400000)
   ✓ Main-context snapshot: 92349 tokens (last turn; trust Claude Code % ctx for real-time headroom)
   ```
3. Update the DOC-003 doc (or its equivalent in the tn repo) to reflect the new wording and cite the FEAT-028 incident as motivation.
4. Verify the JSON schema for `tn session status --json` is unaffected — the rename is cosmetic to the CLI output only; `tokens.current_window_tokens` in JSON stays exactly as today.

## Boundaries / non-goals for v1

- Does NOT change `tokens.*` field names in JSON — those are consumed by skills (see `/tn-plan-session` Step 7 and Step 8) and must stay stable.
- Does NOT introduce hard enforcement of `budget.tokens` against `subagent_tokens_sum`. That budget stays advisory (feeds FEAT-019 calibration only).
- Does NOT touch the main-context-inactive warning block — that one already uses correct wording ("main-context budget inactive" refers to the enforcement feature, not the meter).

## Related

- [[sprint]] — Current sprint
- [[activeContext]] — Active context
- BUG-012 / DOC-003 — original field-semantics fix in the JSON shape
- FEAT-019 — calibration flow that consumes `actual_tokens` in the session log
- FEAT-028 session `2026-04-20-1437` — live incident that motivated this chore
- CHORE-005 — parallel skill-side fix to prevent the same misread on the consumer side

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
