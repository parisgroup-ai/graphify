---
title: "ADR-005: MCP Server as a Separate `graphify-mcp` Binary"
created: 2026-04-12
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-007"
tags:
  - type/adr
  - status/accepted
  - integration
  - cli
supersedes:
superseded_by:
---

# ADR-005: MCP Server as a Separate `graphify-mcp` Binary

## Status

**Accepted** — 2026-04-12

## Context

AI assistants (Claude Code, Codex, etc.) speak the **Model Context Protocol** (MCP) over stdio. Exposing Graphify's graph queries as MCP tools would let an assistant ask "what depends on `X`?" or "explain `Y`" directly during a coding session. We needed to decide:

1. Whether to ship MCP as a subcommand of `graphify` or a separate binary
2. Whether to share or duplicate config/extraction code with the CLI
3. Whether extraction is eager (startup) or lazy (per tool call)

## Decision

**Chosen option:**

1. **Separate binary** `graphify-mcp` in a new workspace crate, alongside `graphify`.
2. **Duplicate config structs** (small, stable) in the MCP crate. Extract to a shared crate only if a third consumer appears.
3. **Eager extraction** at startup; all 9 `QueryEngine` methods exposed as MCP tools; `QueryEngine` wrapped in `Arc` (rmcp's `ServerHandler` requires `Clone`).

Built on `rmcp` v0.1 with `#[tool(tool_box)]` macro (note: API differs from current docs which describe `#[tool_router]`). All diagnostics on stderr — stdout is reserved for JSON-RPC.

## Consequences

### Positive

- Lean CLI binary (no `tokio`/`rmcp` dependency for users who don't need MCP)
- Eager extraction → instant tool responses (acceptable 1–3s startup)
- Reuses [[ADR-004 Graph Query Interface]] — zero duplication of query logic
- 9 tools cover every `QueryEngine` method; per-project routing via optional `project` parameter
- Clean stdio contract: stderr for noise, stdout for protocol

### Negative

- **Two binaries** to ship and version (`graphify` + `graphify-mcp`)
- Config structs duplicated — small risk of drift; documented and accepted
- Long-lived process holds full graph in memory per project
- No incremental refresh — code edits during a session don't update the graph (planned future work)
- `rmcp` is pre-1.0; future API breaks possible

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Separate binary, duplicated config** (chosen) | Lean CLI, simple to ship | Two binaries, slight config duplication |
| Subcommand `graphify mcp` | Single binary | Pulls `tokio` + `rmcp` into the lean CLI |
| Shared `graphify-config` crate | Eliminates duplication | Premature abstraction for two consumers |
| Lazy extraction per tool call | Fresh data | Slow tool responses; complex caching |
| HTTP transport | Networked use | More moving parts; not how MCP clients connect today |

## Plan de Rollback

**Triggers:** MCP protocol change breaks the integration; or the duplicated config drifts and causes user-visible bugs.

**Steps:**
1. Stop building/publishing `graphify-mcp` in CI release workflow
2. Add a deprecation note in the README pointing users at `graphify` CLI commands
3. Optionally extract config to a shared crate **before** restoring MCP

**Validation:** `cargo build --release -p graphify-cli` still succeeds without `graphify-mcp` artifacts. CLI users unaffected.

## Links

- Spec: `docs/superpowers/specs/2026-04-12-feat-007-mcp-server-design.md`
- Plan: `docs/superpowers/plans/2026-04-12-feat-007-mcp-server.md`
- Task: `[[FEAT-007-mcp-server]]`
- Related ADRs: [[ADR-001 Rust Rewrite]], [[ADR-004 Graph Query Interface]]
