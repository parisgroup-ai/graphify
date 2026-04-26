---
uid: feat-042
status: done
priority: normal
scheduled: 2026-04-26
completed: 2026-04-26
pomodoros: 0
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

## Goal

Promote the two project-local bash scripts that consumers maintain to drive `graphify` into Claude Code session context — `gf-context-brief.sh` (~221 LOC) and `gf-context-scope.sh` (~110 LOC) in `apps/cursos/scripts/` — into native `graphify` subcommands, so every consumer gets it for free instead of copy-pasting bash + jq glue.

Reference implementation lives at `~/www/pg/apps/cursos/scripts/gf-context-{brief,scope}.sh` and is documented in `~/www/pg/apps/cursos/memory-bank/topics/gf-context-dispatch.md`.

## What the scripts do today

### `gf-context-brief.sh`

- Reads `report/<proj>/analysis.json` for every `[[project]]` declared in `graphify.toml`.
- Emits a consolidated `.claude/session-context-gf.json` consumed by `/session-start` skill and `tn-session-dispatcher` subagent.
- Output schema (see consumer's `memory-bank/topics/gf-context-dispatch.md`):
  - `schema_version`, `generated_at`, `graphify_version`
  - `baseline_age_days`, `stale` (true when baseline > 7d)
  - `projects[]`
  - `hotspots[]` — top-10 across all projects, ranked by score, with `{project, id, score, in_degree, out_degree, in_cycle, hotspot_type}`
  - `frozen[]` — static list mirroring CLAUDE.md frozen-modules section (consumer-supplied, NOT graphify concern — see open question)
  - `cycles[]` — all cycles found, tagged with project
  - `scope_files[]` / `scope_explains[]` — empty here, filled by `gf-context-scope.sh`
- Cache-aware: skips regeneration when no `analysis.json` is newer than the brief. `--force` bypasses, `--check` exits 2 if stale.

### `gf-context-scope.sh`

- Resolves the active `tn` in-progress task (or accepts `TASK-ID` arg).
- Greps the task body for backtick-quoted code paths under `apps/`, `packages/`, `scripts/`.
- Runs `graphify explain <file>` on each (top 5), captures first 40 lines of output.
- Merges `{scope_task, scope_files[], scope_explains[]}` into `.claude/session-context-gf.json`.

## Why native

- Consumers (cursos, nymos, ordo, …) all want the same brief. Today each project copies bash + jq pipelines that re-implement schema serialization, cache check, baseline staleness, hotspot sort+dedup. Easy place for drift.
- The schema is already a contract (`schema_version: 1`) — graphify owns it semantically. Bash code in consumers serializing that contract is inversion of ownership.
- Scope augmentation ties graphify (`graphify explain`) to a task body. Exposing a stable `graphify scope --files <a,b,c>` command lets consumers compose without grepping their own task notes from a graphify script.

## Proposed shape

```
graphify session brief   # writes <out> with the schema above; cache-aware, --force, --check
  --out <path>           # default: .claude/session-context-gf.json
  --top <N>              # default: 10
  --stale-days <D>       # default: 7

graphify session scope   # appends scope_files/scope_explains to an existing brief
  --files <a,b,c>        # explicit list (no tn coupling)
  --task <ID>            # optional convenience: resolves in-progress via `tn` if installed
  --max <N>              # default: 5
  --in <path>            # default: .claude/session-context-gf.json
```

Boundary call: keep `tn` lookup behind a feature flag or graceful fallback so graphify does NOT hard-depend on tasknotes-cli. Consumers without `tn` use `--files` directly.

## Open questions

1. **`frozen[]` ownership**: The bash version hardcodes a frozen-modules list with paths/reasons/ADRs that mirror the consumer's `CLAUDE.md`. That is consumer-specific, NOT graphify concern. v1 should drop `frozen[]` from native output and let consumers append it post-hoc (or move it to a separate `graphify session frozen --from <md-file>` parser if it ever generalizes).
2. **`tn` dependency in `scope`**: include or exclude? Lean exclude — keep `--files` explicit, document the `tn list --status in-progress | xargs` recipe in README.
3. **Output location**: `.claude/` is Claude-Code-specific. Should the default be `--out <stdout>` and let consumers redirect? Or keep `.claude/session-context-gf.json` as a sane default for the dominant use case?

## Acceptance Criteria

- [ ] `graphify session brief` produces JSON matching today's schema (`schema_version: 1`) modulo the `frozen[]` decision.
- [ ] Cache check honors `--force` / `--check` semantics from bash version (exit 0 fresh, exit 2 stale, exit 1 hard error).
- [ ] `graphify session scope --files a,b,c` appends `scope_files`/`scope_explains` without disturbing other fields.
- [ ] Cursos can delete `gf-context-brief.sh` + `gf-context-scope.sh` and replace `package.json` / hook entries with the new commands.
- [ ] Documented under `~/ai/graphify/docs/` (consumers know what schema they consume).

## Source

- Suggested upstream from `apps/tasknotes-cli` repo session 2026-04-26 — discussion about whether GH/GR/PM glue scripts should go native in `tn`. Conclusion: GH glue belongs in `tn` (FEAT-048/049/050 there); GR glue belongs in `graphify` (this task).
- Reference scripts:
  - `apps/cursos/scripts/gf-context-brief.sh`
  - `apps/cursos/scripts/gf-context-scope.sh`
  - `apps/cursos/memory-bank/topics/gf-context-dispatch.md` (schema doc)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
