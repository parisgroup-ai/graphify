---
title: "ADR-001: Rewrite Graphify from Python to Rust"
created: 2026-04-12
status: accepted
deciders:
  - Cleiton Paris
requirements: []
tags:
  - type/adr
  - status/accepted
  - architecture
  - foundational
supersedes:
superseded_by:
---

# ADR-001: Rewrite Graphify from Python to Rust

## Status

**Accepted** — 2026-04-12

## Context

The original Graphify was a Python tool. Distribution required Python + dependencies on every install target; runtime was slow on large monorepos; and the dependency surface (NetworkX, pyvis, tree-sitter Python bindings) was large and brittle. We wanted:

- A standalone binary with **zero runtime dependencies**
- Same depth of analysis (Python + TypeScript imports/defs/calls)
- Multi-project analysis driven by a config file
- Output formats consumable by external tools (Obsidian, D3, Gephi, Neo4j)
- macOS + Linux distribution via GitHub Releases

## Decision

**Chosen option:** Rewrite in **Rust** as a Cargo workspace with 4 initial crates (`graphify-core`, `graphify-extract`, `graphify-report`, `graphify-cli`), using `petgraph` for the graph model and `tree-sitter` (with native Rust grammars) for AST extraction.

**Why:** Rust gives us static binaries (MUSL on Linux, universal on macOS), single-digit MB binaries, embarrassingly-parallel extraction via `rayon`, and a strong type system to enforce the layered crate architecture. The tree-sitter ecosystem has mature Rust grammars for Python and TypeScript.

## Consequences

### Positive

- ~3.5MB self-contained binary; no Python/Node required to run
- 5–10× faster extraction on typical monorepos (rayon parallelism + native parsing)
- Clean crate boundaries enforced by the type system (`graphify-core` knows nothing about tree-sitter)
- JSON output schema preserved (NetworkX `node_link_data` compatible) — downstream consumers don't break
- New foundation enables later crates (`graphify-mcp`) without entangling the CLI

### Negative

- Slower iteration vs Python during early development (compile times, fewer scripting affordances)
- Smaller pool of contributors familiar with Rust + tree-sitter
- Tree-sitter `Parser` is `!Send` — forces a fresh parser per file under rayon (acceptable cost; documented in conventions)
- Old Python code lives in `legacy/python` branch; not actively maintained

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Rust rewrite** (chosen) | Static binary, fast, type-safe layering | Compile times, fewer Rust contributors |
| Optimize Python in place | Lowest effort, keep ecosystem | Distribution still painful; ceiling on perf |
| Go rewrite | Static binary, easy distribution | Weaker type system; tree-sitter Go bindings less mature than Rust at the time |
| Node/TypeScript | Same lang as analyzed code | Requires Node runtime; same distribution problem |

## Plan de Rollback

**Triggers:** _Effectively forward-only._ A rollback would require resurrecting the Python codebase from `legacy/python` and re-implementing the v0.x feature set (caching, MCP, drift, etc.) that was added after the rewrite.

**Steps (theoretical):**
1. Check out `legacy/python` branch
2. Restore Python publishing pipeline
3. Backport JSON schema additions from Rust (`confidence`, `confidence_kind`, etc.)

**Validation:** Same `analysis.json` shape produced for the same input. **Effort:** weeks.

## Links

- Spec: `docs/superpowers/specs/2026-04-12-graphify-rust-rewrite-design.md`
- Plan: `docs/superpowers/plans/2026-04-12-graphify-rust-rewrite.md`
- Related ADRs: every later ADR builds on this one
