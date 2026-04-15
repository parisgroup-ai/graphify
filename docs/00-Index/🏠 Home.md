---
title: Graphify Documentation
created: 2026-04-14
updated: 2026-04-14
status: published
tags:
  - type/index
  - graphify
related:
  - "[[📚 Documentation Map]]"
  - "[[🔍 Quick Reference]]"
  - "[[System Overview]]"
---

# 🏠 Graphify Documentation

> [!info] What is Graphify
> A Rust CLI for **architectural analysis of codebases**. Extracts dependencies from Python, TypeScript, Go and Rust source via tree-sitter, builds a knowledge graph, computes architecture metrics, and generates reports that surface hotspots, cycles, communities and structural drift.

## 🚀 Quick Start

1. [[Installation]] — build from source or grab a release binary
2. [[Configuration]] — write your `graphify.toml`
3. [[First Steps]] — run the pipeline and read the report
4. [[AI Integrations]] — install Claude Code / Codex skills + MCP server
5. [[Troubleshooting]] — when something goes wrong

## 📖 Documentation Sections

| Section | What's inside |
|---|---|
| [[🔍 Quick Reference]] | One-page CLI cheat sheet |
| [[CLI Reference]] | Per-command reference (one page each) |
| [[📚 Documentation Map]] | Full map of every page |
| [[System Overview]] | Architecture, pipeline, crates (high level) |
| [[Data Flow]] | Pipeline stage by stage |
| [[Tech Stack]] | Dependencies per crate |
| [[ADR-Index\|ADRs]] | 12 architecture decision records (foundation → contracts → PR summary) |
| [[Terms\|Glossary]] | Concepts: hotspot, drift, confidence, community |

## 🧭 By Task

| I want to... | Go to |
|---|---|
| Install Graphify | [[Installation]] |
| Configure a multi-project monorepo | [[Configuration]] |
| Run my first analysis | [[First Steps]] |
| Understand the report outputs | [[System Overview#Key Outputs]] |
| Detect architectural drift between versions | [[🔍 Quick Reference#Drift detection]] |
| Enforce architecture rules in CI | [[🔍 Quick Reference#CI gates]] |
| Query the graph interactively | [[🔍 Quick Reference#Graph queries]] |
| Run an MCP server for AI assistants | [[🔍 Quick Reference#MCP server]] |
| Install Graphify skills + MCP into Claude Code or Codex | [[AI Integrations]] |
| Look up an unfamiliar term | [[Terms\|Glossary]] |

## 📊 Project Status

> [!info] Current Version
> **v0.6.0** — see root `Cargo.toml` (`[workspace.package].version`)

> [!tip] Sprint Board
> Active and completed work tracked in `[[sprint|TaskNotes Sprint Board]]`.

## 🧱 Workspace Crates

| Crate | Responsibility |
|---|---|
| `graphify-core` | Graph model, metrics, cycles, communities, diff, policy, history |
| `graphify-extract` | File walking, AST extraction, module resolution, SHA256 cache |
| `graphify-report` | JSON, CSV, Markdown, HTML, GraphML, Neo4j, Obsidian outputs |
| `graphify-cli` | CLI commands and pipeline orchestration |
| `graphify-mcp` | MCP server exposing graph queries to AI assistants |

## 🔗 External

- [GitHub Repository](https://github.com/parisgroup-ai/graphify)
- Design specs: `docs/superpowers/specs/`
- Implementation plans: `docs/superpowers/plans/`

## 🆘 Need Help?

- Common issues → [[Troubleshooting]]
- Concepts and jargon → [[Terms|Glossary]]
- Recent changes → check `git log` and the [[sprint|sprint board]]
