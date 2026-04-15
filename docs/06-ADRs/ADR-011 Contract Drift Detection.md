---
title: "ADR-011: Contract Drift Detection Between ORM and TypeScript Types"
created: 2026-04-13
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-016"
tags:
  - type/adr
  - status/accepted
  - check
  - extract
  - core
supersedes:
superseded_by:
---

# ADR-011: Contract Drift Detection

## Status

**Accepted** — 2026-04-13

## Context

In monorepos with a Drizzle ORM schema and TypeScript DTOs/interfaces describing the same entity, the two sides drift silently: ORM adds a column, TS forgets to mirror it. Runtime errors only surface when the missing field is actually used. We wanted Graphify to **detect this drift at build time** through `graphify check`, alongside the cycle/hotspot/policy gates.

## Decision

**Chosen option:** Add **first-class contract comparison** to `graphify check`. Three new modules:

- `graphify-core/src/contract.rs` — pure `compare_contracts()` returning typed violations
- `graphify-extract/src/drizzle.rs` — Drizzle parser walking the existing TS AST
- `graphify-extract/src/ts_contract.rs` — TS interface/type extractor walking the same AST

Pairs declared explicitly under `[contract]` in `graphify.toml`. Pair file paths resolve **workspace-root relative** (because pairs routinely cross `[[project]]` boundaries). Contract findings extend the existing `CheckReport` with a workspace-level `contracts` block — additive, non-breaking.

Built-in type map (Drizzle column → primitive) with per-project overrides. Automatic `snake_case ↔ camelCase` normalization plus explicit aliases. 8 violation classes (FieldMissingOnTs, TypeMismatch, NullabilityMismatch, RelationMissingOnTs, CardinalityMismatch, etc.).

## Consequences

### Positive

- Catches a real bug class at build time — no runtime errors from missing/mistyped fields
- Reuses existing TS tree-sitter parser — no new grammar
- Pure comparison in core mirrors `diff.rs` design — easily testable
- Contract data piggybacks on [[ADR-003 SHA256 Extraction Cache]] — incremental builds work for free
- Additive `CheckReport` shape — no breaking change for existing consumers
- `unmapped_type_severity` configurable (warning vs error) — non-blocking adoption path

### Negative

- v1 supports only **Drizzle** — Prisma/TypeORM/Sequelize users get nothing yet
- TS-side limited to `interface`/`type` aliases — Zod/tRPC/OpenAPI not covered
- No convention-based auto-pairing — user must declare every `[[contract.pair]]` explicitly
- Built-in type map is opinionated (e.g., `numeric` → `Number`); needs project overrides for some Drizzle setups
- Workspace-root path resolution **differs** from per-project gates — documented but a footgun for new users
- Mapped/conditional/utility types in TS emit `Unmapped` warnings — false positives at the edges

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Contract gate inside `check`, Drizzle-only v1** (chosen) | Fast to ship; reuses everything | Single ORM in v1 |
| External script | Zero coupling | Every team builds it |
| Generate TS types from Drizzle automatically | Eliminates drift entirely | Forces a code-gen workflow on consumers |
| Parse via dedicated SQL/Drizzle tooling | More accurate | Adds runtime deps; loses single-binary promise |
| `graphify contracts` standalone subcommand | Cleaner namespace | Splits the CI surface |

## Plan de Rollback

**Triggers:** Contract violations produce too many false positives (e.g., common Drizzle patterns the parser doesn't yet understand); or maintenance of the parser proves disproportionate to the bug class it catches.

**Steps:**
1. `graphify check --no-contracts` already disables the gate per-run — recommend this first
2. Make `--no-contracts` the default; require `--contracts` to opt in
3. If structural: remove the `contracts` field from `CheckReport`, delete the new modules

**Validation:** `graphify check` returns identical exit codes for non-contract gates. `CheckReport.contracts` either absent or empty.

## Links

- Spec: `docs/superpowers/specs/2026-04-13-feat-016-contract-drift-design.md`
- Plan: `docs/superpowers/plans/2026-04-13-feat-016-contract-drift.md`
- Task: `[[FEAT-016-contract-drift-detection-between-orm-and-typescript]]`
- Related ADRs: [[ADR-003 SHA256 Extraction Cache]], [[ADR-006 Edge Confidence Scoring]] (uncertainty pattern), [[ADR-008 CI Quality Gates]] (host gate), [[ADR-012 PR Summary CLI]] (downstream consumer)
