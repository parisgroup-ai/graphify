---
title: ADR Index
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/index
  - type/adr
related:
  - "[[🏠 Home]]"
  - "[[📚 Documentation Map]]"
---

# ADR Index

Architecture Decision Records — one entry per significant, structural decision made in Graphify. Each ADR is **immutable** once accepted; revisions create a new ADR with `supersedes:`.

## Conventions

| Field | Rule |
|---|---|
| ID | `ADR-NNN` (zero-padded, sequential) |
| Status | `proposed` → `accepted` → optionally `superseded`/`deprecated` |
| Requirements | Linked FEAT/BUG IDs in frontmatter `requirements:` |
| Rollback | Mandatory section — even if "forward-only", state it explicitly |
| Source spec | Linked under `## Links` (preserves the deep technical exploration) |

> [!info] Template
> Use [[_ADR-Template]] when adding new ADRs.

## All ADRs

| # | Date | Title | Status | Requirements |
|---|---|---|---|---|
| [[ADR-001 Rust Rewrite]] | 2026-04-12 | Rewrite Graphify from Python to Rust | accepted | — |
| [[ADR-002 Interactive HTML Visualization]] | 2026-04-12 | Self-contained D3.js force graph as a report format | accepted | FEAT-001 |
| [[ADR-003 SHA256 Extraction Cache]] | 2026-04-12 | Per-file content-addressable extraction cache | accepted | FEAT-005 |
| [[ADR-004 Graph Query Interface]] | 2026-04-12 | Reusable QueryEngine in core + 4 CLI subcommands | accepted | FEAT-006 |
| [[ADR-005 MCP Server]] | 2026-04-12 | Separate `graphify-mcp` binary exposing 9 tools over stdio | accepted | FEAT-007 |
| [[ADR-006 Edge Confidence Scoring]] | 2026-04-12 | First-class confidence + ConfidenceKind on every edge | accepted | FEAT-008 |
| [[ADR-007 Architectural Drift Detection]] | 2026-04-13 | `graphify diff` over `analysis.json` snapshots | accepted | FEAT-002 |
| [[ADR-008 CI Quality Gates]] | 2026-04-13 | `graphify check` with cycle/hotspot gates | accepted | FEAT-004 |
| [[ADR-009 Watch Mode]] | 2026-04-13 | `graphify watch` with notify v7 + 300ms debounce | accepted | FEAT-010 |
| [[ADR-010 Auto-Detect Local Prefix]] | 2026-04-13 | Runtime `local_prefix` heuristic via src/app dominance | accepted | FEAT-011 |
| [[ADR-011 Contract Drift Detection]] | 2026-04-13 | Drizzle-vs-TS contract comparison via `graphify check` | accepted | FEAT-016 |
| [[ADR-012 PR Summary CLI]] | 2026-04-14 | Pure-renderer `graphify pr-summary` over JSON artifacts | accepted | FEAT-015 |

## Status legend

| Status | Meaning |
|---|---|
| `proposed` | Under discussion; not yet implemented |
| `accepted` | Decision made and codified in the codebase |
| `rejected` | Considered and rejected — kept for historical context |
| `superseded` | Replaced by a newer ADR (linked via `superseded_by`) |
| `deprecated` | Decision remains in code but is no longer recommended |

## Numbering policy

ADR numbers are **chronological by decision date**, not by FEAT ID. The FEAT/BUG linkage lives in the `requirements:` frontmatter. Renumbering FEATs does not affect ADRs.

## Migration note

ADRs ADR-001 through ADR-012 were retroactively created on 2026-04-14 from the design specs in `docs/superpowers/specs/`. The original specs are preserved as the **technical deep-dive**; ADRs are the **governance-grade decision record**.

## Related

- [[🏠 Home]]
- [[📚 Documentation Map]]
- `docs/superpowers/specs/` — original design specs
- `docs/superpowers/plans/` — implementation plans
