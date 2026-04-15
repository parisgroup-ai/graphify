---
title: "graphify pr-summary"
created: 2026-04-14
updated: 2026-04-15
status: published
tags:
  - type/cli
  - command
  - ci
related:
  - "[[CLI Reference]]"
  - "[[check]]"
  - "[[diff]]"
  - "[[ADR-012 PR Summary CLI]]"
---

# `graphify pr-summary`

Render a concise PR-ready Markdown summary of architectural change for a single project, by reading existing JSON artifacts. **Pure renderer** — no re-extraction, no recompute, no gating.

## Synopsis

```bash
graphify pr-summary <PROJECT_OUTPUT_DIR>
```

## Arguments

| Arg | Required | Description |
|---|---|---|
| `<PROJECT_OUTPUT_DIR>` | yes | Path to a single project's Graphify output directory (e.g. `./report/my-app`). Must contain at least `analysis.json`. |

## Flags

None. **Determinism is a feature**: same inputs → same output.

## Inputs read

| File | Required | Source |
|---|---|---|
| `<DIR>/analysis.json` | **yes** | `graphify run` / `graphify analyze` |
| `<DIR>/drift-report.json` | optional | `graphify diff --baseline ... --config ...` |
| `<DIR>/check-report.json` | optional | `graphify check` (always writes this since FEAT-015) |

## Examples

```bash
# Render to stdout
graphify pr-summary ./report/my-app

# GitHub Actions — append to job summary
graphify pr-summary ./report/my-app >> "$GITHUB_STEP_SUMMARY"

# Save as a PR comment body
graphify pr-summary ./report/my-app > pr-summary.md
gh pr comment --body-file pr-summary.md

# Full local pre-push self-check
graphify run --config graphify.toml
graphify diff --baseline ./report-main/my-app/analysis.json \
              --config graphify.toml --project my-app
graphify check --config graphify.toml || true   # gate separately
graphify pr-summary ./report/my-app
```

## Output

Markdown to **stdout**; warnings to **stderr**. Layout:

```markdown
### Graphify — Architecture Delta for `my-app`

142 → 148 nodes (+6) · 287 → 312 edges (+25)

#### Drift in this PR
- **New cycle** — `app.services.llm ↔ app.config`
  `→ graphify path app.services.llm app.config`
- **Escalated hotspots (2)**
  - `app.services.llm` (0.42 → 0.58)  `→ graphify explain app.services.llm`
  - `app.api.routes`   (0.31 → 0.45)  `→ graphify explain app.api.routes`

#### Outstanding issues

**Rules violations (1)** — `graphify check --config graphify.toml`
- `domain-must-not-import-infra` — `app.domain.user → app.infrastructure.db`

**Contract drift (1)** — `graphify check --config graphify.toml`
- `users` (Drizzle) ↔ `UserDto` (TS): FieldMissingOnTs `phone`

<sub>Graphify v0.8.0 · `graphify pr-summary <dir>` to regenerate</sub>
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Successful render (regardless of findings) |
| 1 | Directory missing, `analysis.json` missing/malformed, or multi-project root passed by mistake |

## Graceful degradation

| Input state | Behavior |
|---|---|
| `analysis.json` missing/malformed | exit 1 |
| `<DIR>` is multi-project root (no `analysis.json` of its own) | exit 1 with hint |
| `drift-report.json` missing | hint line: "_No drift baseline — run `graphify diff` to populate._" |
| `drift-report.json` empty | "_No architectural changes vs baseline._" |
| `drift-report.json` malformed | stderr warning; section omitted |
| `check-report.json` missing | "Outstanding issues" section omitted entirely |
| `check-report.json` malformed | stderr warning; section omitted |
| Either subsection of "Outstanding issues" empty | that subsection omitted (other still rendered) |

## Gotchas

- **No flags, no configurability** in v1. Hard-coded section order, hard-coded 5-row cap, hard-coded fixed footer. Opinionated by design ([[ADR-012]]).
- **Doesn't gate** — exits 0 even when violations exist. Combine with `graphify check` if you need PR failure.
- **Pass a single-project directory.** Passing `./report/` (multi-project root) exits 1 with a usage hint pointing at a subdirectory.
- **Output is deterministic** — safe to snapshot-test.

## See also

- [[check]] — produces `check-report.json` (one of the inputs)
- [[diff]] — produces `drift-report.json` (one of the inputs)
- [[ADR-012 PR Summary CLI]] — design rationale and rollback plan
