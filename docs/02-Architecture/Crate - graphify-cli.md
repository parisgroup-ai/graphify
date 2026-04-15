---
title: "Crate: graphify-cli"
created: 2026-04-14
updated: 2026-04-14
status: published
owner: Cleiton Paris
component_status: active
tags:
  - type/component
  - crate
related:
  - "[[Tech Stack]]"
  - "[[Data Flow]]"
  - "[[🔍 Quick Reference]]"
---

# `graphify-cli`

The user-facing binary. Owns CLI argument parsing, config loading, and pipeline orchestration. The "thin" orchestration layer in spirit; the **fattest** crate file in practice (`main.rs` is ~3.2k LOC).

## Overview

| Property | Value |
|---|---|
| Path | `crates/graphify-cli/` |
| Binary? | Yes — `graphify` |
| Lines of code | ~3360 (main.rs: 3178, watch.rs: 180) |
| Files | 2 (`main.rs`, `watch.rs`) |
| Depends on | All workspace crates + `clap`, `toml`, `notify`, `notify-debouncer-mini`, `rayon` |
| Depended by | — (binary) |

## Purpose

Be the front door. Every user-facing command (`run`, `extract`, `analyze`, `report`, `check`, `diff`, `trend`, `query`, `path`, `explain`, `pr-summary`, `watch`, `shell`, `init`) lives here. Each command parses its own flags, loads `graphify.toml`, drives the pipeline, and dispatches to a writer.

## Subcommand inventory

| Command | What it does |
|---|---|
| `init` | Generate a starter `graphify.toml` |
| `extract` | Discovery + AST → `graph.json` per project |
| `analyze` | Extract + metrics + communities + cycles → `analysis.json` |
| `report` | Full pipeline + all configured formats |
| `run` | Alias of `report` (back-compat) |
| `check` | Re-run + evaluate gates ([[ADR-008 CI Quality Gates]]) |
| `diff` | Compare two `analysis.json` snapshots ([[ADR-007 Architectural Drift Detection]]) |
| `trend` | Aggregate historical snapshots → `trend-report.{json,md}` |
| `query <pat>` | Glob match nodes ([[ADR-004 Graph Query Interface]]) |
| `path <a> <b>` | Find dependency paths |
| `explain <node>` | Profile + impact for one node |
| `shell` | Interactive REPL |
| `pr-summary <DIR>` | Render PR Markdown ([[ADR-012 PR Summary CLI]]) |
| `watch` | Re-run on file change ([[ADR-009 Watch Mode]]) |

→ Cheat sheet: [[🔍 Quick Reference]].

## File map

| File | LOC | Role |
|---|---|---|
| `main.rs` | 3178 | All subcommand handlers + config types + pipeline orchestration |
| `watch.rs` | 180 | `WatchFilter`, debounce, affected-project detection |

> [!warning] Big main.rs
> `main.rs` is ~3.2k LOC. It works, it's tested, but it's dense. The natural next refactor would split it into per-command modules (`commands/run.rs`, `commands/check.rs`, …). Not yet a priority — the structure is repetitive and easy to navigate via `grep "fn cmd_"`.

## Public surface

CLI is a binary; nothing exported. Subcommands are dispatched via `clap::Subcommand` derive. The shape (loosely):

```rust
#[derive(Parser)]
struct Cli { #[command(subcommand)] cmd: Commands }

#[derive(Subcommand)]
enum Commands {
    Init { /* ... */ },
    Run { config: PathBuf, force: bool, /* ... */ },
    Extract { /* ... */ },
    Analyze { /* ... */ },
    Report { /* ... */ },
    Check { config: PathBuf, max_cycles: Option<usize>, max_hotspot_score: Option<f64>, json: bool, /* ... */ },
    Diff { before: Option<PathBuf>, after: Option<PathBuf>, baseline: Option<PathBuf>, config: Option<PathBuf>, /* ... */ },
    Trend { /* ... */ },
    Query { pattern: String, /* ... */ },
    Path { source: String, target: String, /* ... */ },
    Explain { node_id: String, /* ... */ },
    Shell { /* ... */ },
    PrSummary { dir: PathBuf },
    Watch { /* ... */ },
}
```

Each `Commands::X` has a corresponding `fn cmd_x(...)` handler.

## Design properties

### Project pipeline runs in parallel via `rayon`

```rust
config.projects.par_iter().for_each(|project| {
    run_pipeline_for_project(project, settings);
});
```

CPU-bound work (extraction, metrics, communities, cycles) is the dominant cost. `rayon` saturates cores cleanly. Per-project state is independent; no shared mutable state.

### Config types duplicated in MCP

The config structs (`Config`, `Settings`, `ProjectConfig`) are also duplicated in `graphify-mcp/src/main.rs`. By design ([[ADR-005 MCP Server]]) — small, stable types; extracting to a shared crate would be premature.

### Watch is its own module

`watch.rs` is the only non-trivial extraction. It owns:
- `WatchFilter` — extension + exclude matching
- Debounce loop wiring
- Affected-project detection (per-project path prefix matching)

Everything else stays in `main.rs`.

### Exit-code convention

All Graphify CLI errors use **exit 1** (uniform across the workspace). `graphify check` exits 1 on architecture violations; `graphify pr-summary` exits 1 on missing required input. POSIX would suggest exit 2 for usage errors; we deliberately deviate to keep the convention uniform — see [[ADR-012 PR Summary CLI]].

## Configuration

| Source | Effect |
|---|---|
| `--config` flag | Path to `graphify.toml` (default: `./graphify.toml`) |
| `graphify.toml` | All persistent config — see [[Configuration]] |
| `--force` | Bypass extraction cache (initial build only in watch mode) |
| `--json` | Machine-readable output for `check` (and others) |
| `--project <name>` | Filter to a single project (where applicable) |

## Testing

```bash
cargo test -p graphify-cli                    # unit + integration
cargo test -p graphify-cli --test pr_summary  # specific integration test
```

Integration tests use `OnceLock` to build the binary once, then spawn it as a subprocess against fixture directories under `tests/fixtures/`. This pattern (introduced in FEAT-013) keeps wall-clock bounded for the whole CLI test suite.

## Common gotchas

- **`graphify check` writes `<project_out>/check-report.json` unconditionally** since FEAT-015. This is additive but new — tooling expecting clean dirs needs to know.
- **Watch `--force` only applies to the initial build.** Subsequent rebuilds always use the cache. Restart watch to force again.
- **Default `--config` is `./graphify.toml`.** Run from the wrong directory and you get "config not found". Use `--config` explicitly in CI.
- **`graphify init` overwrites** any existing `graphify.toml` without prompting. Add `git stash` if you've customized one.
- **Query commands always re-extract** — they bypass the cache by design ([[ADR-003 SHA256 Extraction Cache]]).

## Related

- [[Data Flow]] — pipeline orchestration
- [[🔍 Quick Reference]] — every command, one page
- [[Configuration]] — `graphify.toml` reference
- [[Crate - graphify-mcp]] — sister binary (config duplicated)
- [[ADR-008 CI Quality Gates]] · [[ADR-009 Watch Mode]] · [[ADR-012 PR Summary CLI]]
