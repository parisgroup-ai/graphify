---
title: "ADR-010: Auto-Detect `local_prefix` at Runtime"
created: 2026-04-13
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-011"
tags:
  - type/adr
  - status/accepted
  - config
  - extract
supersedes:
superseded_by:
---

# ADR-010: Auto-Detect `local_prefix` at Runtime

## Status

**Accepted** — 2026-04-13

## Context

`local_prefix` in `graphify.toml` controls which modules are marked "local" (project code) vs external. It also affects file discovery and module-ID stability. Many users hit empty-graph issues because they forgot or mis-set this field. We wanted to make the common cases ("source lives under `src/`" or "source lives under `app/`") work without configuration.

## Decision

**Chosen option:** When `local_prefix` is omitted, detect at runtime via a **conservative dominance heuristic**:

- Use `src` if `src/` concentrates **>60%** of eligible source files
- Else use `app` if `app/` concentrates **>60%** of eligible source files
- Otherwise use empty prefix `""` (root-level files dominate)

Explicit `local_prefix` in config remains sovereign. Detection happens in the walker, **before** `discover_files()`. The detected value is used uniformly in discovery, warnings, and the extraction cache. CLI logs "auto-detected local_prefix: <X>" only when detection was used.

Detection is **not persisted** back to `graphify.toml`.

## Consequences

### Positive

- "Just works" on standard `src/`-based and `app/`-based layouts (covers ~90% of monorepo apps)
- Existing configs with explicit `local_prefix` continue to behave identically
- Clear log line tells the user what was detected — no silent guessing
- Detection lives in the walker — same source of truth as discovery
- 60% threshold is conservative — ambiguous repos fall back to empty prefix instead of guessing wrong
- Cache key includes `local_prefix`, so detection results are correctly invalidated

### Negative

- Two heuristic candidates (`src`, `app`) — repos with non-standard top-level names still need explicit config
- Detection runs on every invocation (cheap but non-zero cost)
- Doesn't read `tsconfig.json` or framework conventions — leaves signal on the table
- The 60% threshold is a magic number — chosen by intuition, not data

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Runtime heuristic, src/app dominance** (chosen) | Covers common cases; conservative | Magic threshold; only 2 candidates |
| Persist suggestion to `graphify.toml` | Self-documenting | Mutates user files; surprising |
| Read `tsconfig.json` `baseUrl`/`paths` | Honors framework | Per-language; doesn't help Python/Go/Rust |
| Always require explicit `local_prefix` | No surprises | Worse onboarding; stays as fallback |
| Try every plausible candidate and pick best | Most thorough | Costly; ambiguous for diverse repos |

## Plan de Rollback

**Triggers:** Auto-detection picks the wrong prefix in non-edge-case repos and users complain.

**Steps:**
1. Make `local_prefix` required again in the config validator
2. Emit a clear error when omitted (point at the docs)
3. Keep the detector function for future opt-in use (e.g., `--detect-prefix` flag)

**Validation:** `graphify init` still writes a starter with explicit `local_prefix`. Existing configs unaffected.

## Links

- Spec: `docs/superpowers/specs/2026-04-13-feat-011-auto-detect-local-prefix-design.md`
- Plan: `docs/superpowers/plans/2026-04-13-feat-011-auto-detect-local-prefix.md`
- Task: `[[FEAT-011-auto-detect-local-prefix]]`
- Related ADRs: [[ADR-001 Rust Rewrite]], [[ADR-003 SHA256 Extraction Cache]] (cache key)
