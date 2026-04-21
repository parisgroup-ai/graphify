---
uid: chore-005
status: done
priority: normal
scheduled: 2026-04-20
completed: 2026-04-20
timeEstimate: 30
pomodoros: 0
timeSpent: 3
timeEntries:
- date: 2026-04-20
  minutes: 3
  note: source=<usage>; cross-repo — /tn-plan-session skill lives at ~/.claude/commands/tn-plan-session.md, outside graphify; dispatch from ~/.claude cwd or via share-skill
  type: manual
  executor: claude-solo
  tokens: 44086
projects:
- '[[sprint.md|Current Sprint]]'
contexts:
- skill
- tn-plan-session
- dx
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  estimateTokens: 30000
  hintsInferred: false
---

# chore(skill): guard /tn-plan-session step 8 against closing on subagent_tokens_sum near budget

The `/tn-plan-session` skill's Step 8 bullet 5 already forbids recommending session close on the token advisory alone, but the guidance is buried and doesn't name the specific misread pattern (`subagent_tokens_sum / budget.tokens` near 100% being treated as context exhaustion). Add an explicit paragraph clarifying that `subagent_tokens_sum` is a soft calibration meter — fresh `Task` subagents each allocate their own 1M-token context from the model window — and reference the FEAT-028 incident so the rule sticks.

## Description

During graphify session `2026-04-20-1437` (FEAT-028 multi-slice dispatch) the orchestrator interpreted `subagent_tokens_sum: 378788 / budget.tokens: 400000` (95%) as a hard ceiling and pressed for session close — at a point where the Claude Code status bar showed ~9% main-context usage (so ~91% headroom on the 1M model window). The user correctly pushed back: three distinct meters were being conflated:

1. **tn `subagent_tokens_sum`** — cumulative across dispatches, advisory, feeds FEAT-019 calibration only.
2. **tn `current_window_tokens`** — snapshot of the latest main-session turn (per BUG-012/DOC-003).
3. **Claude Code `% ctx`** — real-time usage of the main orchestrator's 1M-token model context window.

The skill currently covers #2 and #3 adequately in Step 8 bullet 3 ("trust the Claude Code status bar `% ctx` for real-time headroom") but does not spell out #1's non-gating role. The forbidden move — closing because `subagent_tokens_sum` is near `budget.tokens` — is the exact pattern that fired during the live incident, and "based on the token advisory alone" in the current wording is too abstract to catch that misread in practice.

## Motivation

- Prevent the live incident from recurring. The skill's existing guidance is correct but not sufficiently concrete — it needs to name the specific shape of the error.
- Close the loop with CHORE-004 (tn-side rename). Even after tn renames `main-context budget:` → `main-context snapshot:`, an orchestrator reading `subagent_tokens_sum` as a ceiling would still mis-close. The skill-side fix is orthogonal and both should land.
- Make the orchestrator aware that each `Task` tool invocation allocates a fresh 1M-token model context — so cumulative subagent spend has no direct bearing on the next dispatch's capacity. This is a property of Claude Code / Anthropic API that is not otherwise surfaced in the skill.

## Likely scope

1. Add a new paragraph to `/tn-plan-session` Step 8 (or a new dedicated row in the Exit Conditions table) with this shape:

   > **Do NOT recommend close on `subagent_tokens_sum` approaching `budget.tokens`.** tn's `--tokens N` budget is a **soft calibration signal**, not an enforcement ceiling. Each `Task` subagent dispatch allocates its own 1M-token context from the model window — cumulative subagent spend does not constrain the next dispatch's capacity. For real-time headroom on the main orchestrator session, trust the Claude Code status bar `% ctx` readout (which measures against the 1M model window, not against tn's soft 400k advisory). The only legitimate token-side trigger for close is the explicit main-context-inactive warning block, which signals a hook-tracking problem, not exhaustion.

2. Add a companion sentence to the Exit Conditions table row that currently covers wall-clock 20% remaining: surface `subagent_tokens_sum` approaching `budget.tokens` explicitly as a **NOT a close trigger** entry, so the forbidden move is visible in the table itself.
3. Cross-reference the FEAT-028 incident (graphify session `2026-04-20-1437`) in the skill's change log or top-of-file history so future readers understand why this rule is phrased as precisely as it is.
4. Verify no other skill (e.g. `/tn-plan-today`, `/session-close`) carries a contradictory rule about token budgets gating dispatch. Update them in parallel if found.

## Boundaries / non-goals for v1

- Does NOT change the wall-clock trigger (`wall_used_pct >= 0.8`) — that one is still valid and actionable.
- Does NOT change the main-context-inactive warning block in Step 7 / Step 8 — that one is a real problem signal (hook silent while logging tokens).
- Does NOT modify FEAT-019 calibration behavior. `subagent_tokens_sum` continues feeding calibration; the only change is how the orchestrator is instructed to interpret it.

## Related

- [[sprint]] — Current sprint
- [[activeContext]] — Active context
- CHORE-004 — tn-side rename (`main-context budget:` → `main-context snapshot:`) — companion fix; both should land.
- BUG-012 / DOC-003 — field-semantics fix that inspired this follow-up on the consumer side.
- FEAT-019 — calibration flow whose advisory budget is being misread as enforcement.
- FEAT-028 session `2026-04-20-1437` — the live incident where the misread surfaced.

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
