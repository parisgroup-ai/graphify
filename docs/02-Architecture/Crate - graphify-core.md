---
title: "Crate: graphify-core"
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
  - "[[Crate - graphify-extract]]"
  - "[[Crate - graphify-report]]"
---

# `graphify-core`

The pure-domain crate. Owns the graph model, metrics, communities, cycles, snapshots, contracts, history, and policy. **No tree-sitter, no IO** beyond serde.

## Overview

| Property | Value |
|---|---|
| Path | `crates/graphify-core/` |
| Binary? | No (library only) |
| Lines of code | ~6000 |
| Modules | 11 |
| Depends on | `petgraph`, `rand`, `serde`, `serde_json` |
| Depended by | `graphify-extract`, `graphify-report`, `graphify-cli`, `graphify-mcp` |

## Purpose

Hold the **stable domain model** that every other crate consumes. By keeping core free of tree-sitter and disk IO, we get:

- Trivially testable algorithms (no fixtures, no temp dirs)
- Reuse across CLI, MCP, and any future consumer
- Independent evolution of extractors and report writers without touching the model

## Module map

| Module | LOC | Role |
|---|---|---|
| `types.rs` | 436 | `Node`, `Edge`, `Language`, `NodeKind`, `EdgeKind`, `ConfidenceKind` |
| `graph.rs` | 613 | `CodeGraph` — `petgraph::DiGraph<Node, Edge>` wrapper with dedup + weight increment |
| `metrics.rs` | 574 | Betweenness (Brandes), PageRank, unified scoring weights |
| `community.rs` | — | Louvain + Label Propagation fallback |
| `cycles.rs` | 275 | Tarjan SCC + simple cycles (cap 500) |
| `query.rs` | 1061 | `QueryEngine` — search, paths, explain, suggest, stats |
| `diff.rs` | 791 | `AnalysisSnapshot`, `DiffReport`, `compute_diff()` |
| `history.rs` | 611 | Historical snapshot store, `compute_trend_report()` |
| `policy.rs` | 708 | Policy rule compilation + evaluation (`CompiledPolicy`) |
| `contract.rs` | 920 | `Contract`, `ContractComparison`, `compare_contracts()` |

`lib.rs` is a 10-line manifest that just exports the modules.

## Public surface (highlights)

```rust
// types.rs
pub struct Node { id: String, kind: NodeKind, file_path: PathBuf, language: Language, line: usize, is_local: bool }
pub struct Edge { kind: EdgeKind, weight: u32, line: usize, confidence: f64, confidence_kind: ConfidenceKind }
pub enum EdgeKind { Imports, Defines, Calls }
pub enum NodeKind { Module, Function, Class, Method, Trait, Enum }
pub enum Language { Python, TypeScript, Go, Rust }
pub enum ConfidenceKind { Extracted, Inferred, Ambiguous }

// graph.rs
pub struct CodeGraph(DiGraph<Node, Edge>);
impl CodeGraph {
    pub fn add_or_get_node(&mut self, node: Node) -> NodeIndex;
    pub fn add_edge(&mut self, src: &str, dst: &str, edge: Edge);  // dedups + max-confidence merge
}

// metrics.rs
pub struct ScoringWeights { betweenness: f64, pagerank: f64, in_degree: f64, in_cycle: f64 }
pub fn compute_metrics(graph: &CodeGraph, weights: &ScoringWeights) -> Vec<NodeMetrics>;

// query.rs
pub struct QueryEngine { /* graph + metrics + communities + cycles */ }
impl QueryEngine {
    pub fn from_analyzed(graph: CodeGraph, metrics: Vec<NodeMetrics>, communities: Vec<Community>, cycles: Vec<CycleGroup>) -> Self;
    pub fn search(&self, pattern: &str, filters: SearchFilters) -> Vec<QueryMatch>;
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<PathStep>>;
    pub fn all_paths(&self, from: &str, to: &str, max_depth: usize, max_paths: usize) -> Vec<Vec<PathStep>>;
    pub fn explain(&self, node_id: &str) -> Option<ExplainReport>;
    pub fn dependents(&self, id: &str) -> Vec<(&str, &EdgeKind)>;
    pub fn dependencies(&self, id: &str) -> Vec<(&str, &EdgeKind)>;
    pub fn transitive_dependents(&self, id: &str, max_depth: usize) -> Vec<(String, usize)>;
    pub fn suggest(&self, input: &str) -> Vec<&str>;
    pub fn stats(&self) -> GraphStats;
}

// diff.rs
pub struct AnalysisSnapshot { /* deserializes analysis.json */ }
pub fn compute_diff(before: &AnalysisSnapshot, after: &AnalysisSnapshot, threshold: f64) -> DiffReport;

// contract.rs
pub fn compare_contracts(orm: &Contract, ts: &Contract, pair: &PairConfig, global: &GlobalContractConfig) -> ContractComparison;
```

## Design properties

### Pure functions everywhere it matters

`compute_diff()`, `compare_contracts()`, `compute_trend_report()`, `compute_metrics()` — all pure. No file IO, no global state. This is the property that makes them snapshot-testable in `#[cfg(test)]` with hand-built fixtures.

### Manual `Eq` on `Edge`

`Edge` carries `confidence: f64`, which doesn't implement `Eq`. We implement `Eq` manually via `f64::to_bits()` (bitwise equality). Rationale: cache lookups and `assert_eq!` in tests need it, and the confidence values are produced by deterministic code (no NaN risk in practice). See [[ADR-006 Edge Confidence Scoring]].

### Snapshot decoupling

`AnalysisSnapshot` (in `diff.rs`) is a separate type from the live `NodeMetrics`/`Community`/`Cycle` types. It exists **specifically** to deserialize `analysis.json` independently of internal evolution. Adding fields to internal types doesn't break drift detection on old snapshots.

### Algorithms — quick reference

| Algorithm | Where | Notes |
|---|---|---|
| Brandes betweenness centrality | `metrics.rs` | Sampled `k = min(200, n)` for tractability |
| PageRank | `metrics.rs` | Iterative, damping 0.85, max 100 iters, ε=1e-6 |
| Louvain communities | `community.rs` | Phase 2 merges singleton communities |
| Label Propagation | `community.rs` | Fallback when Louvain degenerates (BUG-008) |
| Tarjan SCC | `cycles.rs` | petgraph native |
| Simple cycles enumeration | `cycles.rs` | Per-SCC, capped at 500 |

## Configuration / variables

None — `graphify-core` is pure data + algorithms. Configuration arrives as parameters from `graphify-cli` or `graphify-mcp`.

## Testing

```bash
cargo test -p graphify-core
```

Each module has its own `#[cfg(test)]` section with hand-built `CodeGraph`s. The `query.rs` module alone has ~30 unit tests. Total: hundreds of pure-function tests, no fixtures or temp dirs.

## Common gotchas

- **Don't add IO here.** Anything that reads files belongs in `graphify-extract`; anything that writes belongs in `graphify-report`.
- **Don't reach into `petgraph` from outside this crate.** `CodeGraph` is the abstraction boundary. New consumers should add a method to `CodeGraph` rather than indexing the inner `DiGraph` directly.
- **Confidence is a `f64` with `Eq`.** If you add a new edge construction site, decide what confidence/kind it should have or you'll cause subtle merge drift.

## Related

- [[Data Flow]] — how core's outputs feed report writers
- [[Crate - graphify-extract]] — where AST → `Node`/`Edge` happens
- [[ADR-006 Edge Confidence Scoring]] — confidence model
- [[ADR-007 Architectural Drift Detection]] — `diff.rs`
- [[ADR-008 CI Quality Gates]] — `policy.rs`
- [[ADR-011 Contract Drift Detection]] — `contract.rs`
