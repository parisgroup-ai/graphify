---
title: Documentation Map
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/moc
  - type/index
related:
  - "[[🏠 Home]]"
---

# 📚 Documentation Map

A map of every documentation page in this vault, organized by section. Use this when you need to find a specific page or understand what coverage exists.

## 00 — Index

| Page | Purpose |
|---|---|
| [[🏠 Home]] | Main entry point |
| [[📚 Documentation Map]] | This page — the MOC |
| [[🔍 Quick Reference]] | CLI cheat sheet |

## 01 — Getting Started

| Page | Purpose |
|---|---|
| [[Installation]] | Build from source, install binary, prerequisites |
| [[Configuration]] | `graphify.toml` reference and recipes |
| [[First Steps]] | First pipeline run and report walkthrough |
| [[AI Integrations]] | Claude Code + Codex setup for solo and team |
| [[Troubleshooting]] | Common errors and fixes |

## 02 — Architecture

| Page | Purpose |
|---|---|
| [[System Overview]] | High-level crates, pipeline, outputs, why it exists |
| [[Data Flow]] | End-to-end pipeline visualized stage by stage (mermaid) |
| [[Tech Stack]] | Every dependency per crate + cross-cutting choices |
| [[Crate - graphify-core]] | Pure domain model: graph, metrics, snapshots, contracts |
| [[Crate - graphify-extract]] | AST extraction, walker, resolver, SHA256 cache |
| [[Crate - graphify-report]] | 15 output writers — one module per format |
| [[Crate - graphify-cli]] | User-facing binary `graphify` (14 subcommands) |
| [[Crate - graphify-mcp]] | MCP server binary `graphify-mcp` (9 tools over stdio) |

## 03 — API / CLI Reference

| Page | Purpose |
|---|---|
| [[CLI Reference]] | Index of all commands with summary tables |
| [[init]] | `graphify init` — generate starter config |
| [[extract]] | Extraction-only stage |
| [[analyze]] | Extract + metrics |
| [[report]] | Full pipeline + all formats |
| [[run]] | Alias of `report` |
| [[watch]] | Auto-rebuild on file change |
| [[check]] | CI quality gates |
| [[diff]] | Compare two analysis snapshots |
| [[trend]] | Aggregate historical snapshots |
| [[pr-summary]] | Render PR Markdown from JSON artifacts |
| [[query]] | Glob-search nodes |
| [[explain]] | Profile + impact for one node |
| [[path]] | Find dependency paths |
| [[shell]] | Interactive REPL |
| [[MCP Server\|graphify-mcp]] | Companion MCP server binary |

## 04 — Development

> [!todo] Planned
> Local setup, code style (Rust 2021), Git workflow, testing guide, CI/CD.
> Today, see project root `CLAUDE.md` for conventions.

## 05 — Operations

> [!todo] Planned
> Release procedure, version bump runbook, MCP server deployment, cache recovery.
> Today, see `CLAUDE.md → Build & Release`.

## 06 — ADRs (Architecture Decision Records)

| Page | Purpose |
|---|---|
| [[ADR-Index]] | Master index of all ADRs with status |
| [[_ADR-Template]] | Template for new ADRs (governance: IDs, rollback, requirements links) |
| [[ADR-001 Rust Rewrite]] | Foundation: Python → Rust |
| [[ADR-002 Interactive HTML Visualization]] | Self-contained D3 force graph |
| [[ADR-003 SHA256 Extraction Cache]] | Per-file content cache |
| [[ADR-004 Graph Query Interface]] | Reusable QueryEngine + 4 CLI subcommands |
| [[ADR-005 MCP Server]] | Separate `graphify-mcp` binary |
| [[ADR-006 Edge Confidence Scoring]] | First-class confidence on every edge |
| [[ADR-007 Architectural Drift Detection]] | `graphify diff` over snapshots |
| [[ADR-008 CI Quality Gates]] | `graphify check` cycle/hotspot gates |
| [[ADR-009 Watch Mode]] | `graphify watch` with notify v7 |
| [[ADR-010 Auto-Detect Local Prefix]] | Runtime src/app heuristic |
| [[ADR-011 Contract Drift Detection]] | Drizzle-vs-TS gate in `check` |
| [[ADR-012 PR Summary CLI]] | Pure renderer over JSON artifacts |

> [!info] Source preservation
> Original design specs remain in `docs/superpowers/specs/` as the technical deep-dive. ADRs are the governance-grade decision record (concise, with rollback and bidirectional links).

## 07 — Meeting Notes

> [!info] Not used
> Solo project — no meeting notes captured.

## 08 — Glossary

| Page | Purpose |
|---|---|
| [[Terms]] | Concepts: node, edge, hotspot, community, drift, confidence, MCP, etc. |

## TaskNotes (sprint tracking)

| Path | Purpose |
|---|---|
| [[sprint]] | Active sprint board with all bug/feature IDs |
| `docs/TaskNotes/Tasks/BUG-*.md` | Per-bug postmortems |
| `docs/TaskNotes/Tasks/FEAT-*.md` | Per-feature task notes |

## Superpowers (design archive)

| Path | Purpose |
|---|---|
| `docs/superpowers/specs/` | Pre-implementation design docs |
| `docs/superpowers/plans/` | Step-by-step implementation plans |
| `docs/plans/` | Bug-specific design docs |

## Related

- [[🏠 Home]]
- [[🔍 Quick Reference]]
