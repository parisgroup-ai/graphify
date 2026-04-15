---
title: "Crate: graphify-report"
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
  - "[[Crate - graphify-core]]"
---

# `graphify-report`

The output layer. Takes analyzed data and writes it to disk in 7+ formats. **No domain logic** — pure translators. Most expansive and most expansion-friendly crate in the workspace.

## Overview

| Property | Value |
|---|---|
| Path | `crates/graphify-report/` |
| Binary? | No (library only) |
| Lines of code | ~4500 |
| Modules | 16 |
| Depends on | `graphify-core`, `serde`, `serde_json`, `csv` |
| Depended by | `graphify-cli` (and `graphify-mcp` indirectly via `pr_summary`) |

## Purpose

Translate `CodeGraph` + `Vec<NodeMetrics>` + `Vec<Community>` + `Vec<Cycle>` (plus check/diff/contract/trend results) into files on disk. Each format gets its own module so adding a new one is an isolated change.

## Module map

| Module | LOC | Producer of |
|---|---|---|
| `json.rs` | 387 | `graph.json` (NetworkX node_link), `analysis.json` (metrics + communities + cycles + confidence summary) |
| `csv.rs` | 222 | `graph_nodes.csv`, `graph_edges.csv` |
| `markdown.rs` | 266 | `architecture_report.md` |
| `html.rs` | 409 | `architecture_graph.html` (D3.js, self-contained) |
| `neo4j.rs` | 155 | `graph.cypher` (Neo4j import script) |
| `graphml.rs` | 264 | `graph.graphml` (yEd / Gephi compatible) |
| `obsidian.rs` | 374 | `obsidian_vault/` — one `.md` per node with wikilinks |
| `diff_json.rs` | 94 | `drift-report.json` |
| `diff_markdown.rs` | 299 | `drift-report.md` |
| `trend_json.rs` | 96 | `trend-report.json` |
| `trend_markdown.rs` | 271 | `trend-report.md` |
| `check_report.rs` | 177 | `CheckReport` types (moved from CLI in FEAT-015 — public so `pr_summary` can deserialize) |
| `contract_json.rs` | 159 | Contract findings JSON (slots into `check-report.json`) |
| `contract_markdown.rs` | 166 | Contract findings Markdown section |
| `pr_summary.rs` | 945 | Pure renderer for `graphify pr-summary` ([[ADR-012 PR Summary CLI]]) |

## Public surface (highlights)

```rust
// json.rs
pub fn write_graph_json(graph: &CodeGraph, path: &Path);
pub fn write_analysis_json(graph: &CodeGraph, metrics: &[NodeMetrics], communities: &[Community], cycles: &[Cycle], path: &Path);

// csv.rs
pub fn write_nodes_csv(metrics: &[NodeMetrics], path: &Path);
pub fn write_edges_csv(graph: &CodeGraph, path: &Path);

// markdown.rs / html.rs / neo4j.rs / graphml.rs / obsidian.rs
pub fn write_report(...) -> ...;       // Markdown
pub fn write_html(...) -> ...;
pub fn write_cypher(...) -> ...;       // Neo4j
pub fn write_graphml(...) -> ...;
pub fn write_obsidian_vault(...) -> ...;

// diff_*.rs / trend_*.rs
pub fn write_diff_json(report: &DiffReport, path: &Path);
pub fn write_diff_markdown(report: &DiffReport, path: &Path);
pub fn write_trend_json(...);
pub fn write_trend_markdown(...);

// check_report.rs (public types — consumed by CLI producer + pr_summary consumer)
pub struct CheckReport { ok: bool, violations: usize, projects: Vec<ProjectCheckResult>, contracts: Option<ContractCheckResult> }
pub struct ProjectCheckResult { name: String, ok: bool, summary: ProjectCheckSummary, limits: CheckLimits, violations: Vec<CheckViolation> }
pub enum CheckViolation { Limit { ... }, Policy { ... } }

// contract_*.rs
pub fn build_contract_check_result(...) -> ContractCheckResult;
pub fn write_contract_markdown_section(...) -> String;

// pr_summary.rs
pub fn render(project_name: &str, analysis: &AnalysisSnapshot, drift: Option<&DiffReport>, check: Option<&CheckReport>) -> String;
```

## Design properties

### One module per format

Adding a new output format = one new module + one entry in `lib.rs`'s re-exports + one match arm in CLI's format dispatch. Removing a format is similarly isolated. No format knows about another.

### Pure functions

Every public function takes `&` references and writes to `&Path` (or returns a `String`). No global state, no caching. Testable in isolation with hand-built `CodeGraph`s.

### Separated **types** for `check-report.json`

Before FEAT-015, `CheckReport` lived as a private struct inside `graphify-cli/src/main.rs`. The pure-renderer design of `pr-summary` forced these types to become public (in `check_report.rs`) so the renderer could deserialize them. This split made the producer/consumer boundary honest. See [[ADR-012 PR Summary CLI]].

### HTML embeds D3 via `include_str!`

`architecture_graph.html` is built by string concatenation in `html.rs`. D3 v7, custom CSS, and visualization JS are all `include_str!`'d into a single output file. The data block is one `<script>const GRAPHIFY_DATA = {...}</script>` line. Result: one self-contained ~300–400 KB HTML file per project.

### Obsidian vault is a folder, not a file

`write_obsidian_vault()` produces a directory of `.md` files (one per node) with `[[wikilinks]]` between them. Drop the folder into any Obsidian vault and the graph view works.

## Configuration / variables

None at the crate level. Format selection happens in the CLI via `[settings].format = [...]` in `graphify.toml`.

## Testing

```bash
cargo test -p graphify-report
```

Each writer has unit tests using `tempfile::TempDir` for filesystem assertions. `pr_summary.rs` has the largest suite (~16 unit tests, golden-file style with inline JSON fixtures).

## Common gotchas

- **`graph.json` schema mirrors NetworkX `node_link_data`** — don't break that without a major version bump (downstream Python consumers depend on it).
- **CSV columns are appended at the tail when added.** Position-based readers will break; column-name readers are fine.
- **`check-report.json` is now written unconditionally** by `graphify check` (FEAT-015 ecosystem change). Tooling that prunes Graphify outputs needs to know about it.
- **HTML file size** can spike with very large graphs (the data block grows linearly). Consider sharding by community for >5k nodes.
- **`pr_summary::render()` is pure.** Don't add IO to it — file loading and error handling belong in `graphify-cli`.

## Related

- [[Data Flow]] — pipeline stage 8 (report writers) and 9 (cross-cutting outputs)
- [[Crate - graphify-core]] — owns the input types
- [[Crate - graphify-cli]] — orchestrates which writers run
- [[ADR-002 Interactive HTML Visualization]] · [[ADR-007 Architectural Drift Detection]] · [[ADR-008 CI Quality Gates]] · [[ADR-011 Contract Drift Detection]] · [[ADR-012 PR Summary CLI]]
