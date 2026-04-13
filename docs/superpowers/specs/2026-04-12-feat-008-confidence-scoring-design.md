# FEAT-008: Edge Confidence Scoring — Design Spec

**Date:** 2026-04-12
**Status:** Draft
**Author:** Claude (Session 4)

## Problem

Graphify currently treats all edges as equally reliable. In practice, edges vary in certainty: a direct `import os` is unambiguous, a resolved `from . import utils` depends on heuristic resolution, and a bare `bar()` call may not resolve to any known module. Users have no way to distinguish high-confidence structural facts from best-effort guesses.

## Goals

1. Classify every edge as `Extracted` (direct AST observation), `Inferred` (resolved by heuristic), or `Ambiguous` (unresolved/placeholder target)
2. Assign a continuous confidence score (0.0-1.0) to each edge
3. Surface confidence breakdown in all report outputs (JSON, CSV, Markdown, HTML)
4. Allow filtering edges by minimum confidence in the query interface and MCP tools

## Non-Goals

- Node-level confidence (all nodes remain equally trusted)
- User-configurable confidence thresholds per edge kind (may add later)
- Confidence decay over time (requires FEAT-005 incremental builds first)

## Design

### 1. Data Model — `types.rs`

Add `ConfidenceKind` enum and two new fields to `Edge`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceKind {
    Extracted,
    Inferred,
    Ambiguous,
}

pub struct Edge {
    pub kind: EdgeKind,
    pub weight: u32,
    pub line: usize,
    pub confidence: f64,
    pub confidence_kind: ConfidenceKind,
}
```

**Eq implementation:** `Edge` currently derives `Eq`, but `f64` does not implement `Eq`. Replace the derive with a manual `PartialEq` + `Eq` implementation that compares `confidence` via `f64::to_bits()` for bitwise-exact equality. This preserves test assertions and the existing `Eq` contract.

**Convenience constructors** (`Edge::imports()`, `Edge::defines()`, `Edge::calls()`) default to `confidence: 1.0, confidence_kind: Extracted`. A builder method is added:

```rust
impl Edge {
    pub fn with_confidence(mut self, score: f64, kind: ConfidenceKind) -> Self {
        self.confidence = score;
        self.confidence_kind = kind;
        self
    }
}
```

### 2. Extraction — `python.rs`, `typescript.rs`

Most edges from extractors remain at **confidence 1.0, Extracted** — they are direct AST observations (import statements, function/class definitions, re-exports).

**Exception — bare call sites:** `extract_calls_recursive` in both extractors produces `Edge::calls()` for bare identifier calls (e.g., `bar()`, `setup()`). These get **confidence 0.7, Inferred** because the callee is an unqualified name that may or may not resolve to a known module. Use `Edge::calls(line).with_confidence(0.7, ConfidenceKind::Inferred)`.

### 3. Resolution — `resolver.rs`

Change `ModuleResolver::resolve()` signature to return a confidence score:

```rust
// Before:
pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool)
// After:
pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool, f64)
```

Confidence values by resolution path:

| Resolution path | Confidence | Rationale |
|---|---|---|
| Direct known module (exact match) | 1.0 | The name is in the module registry |
| Python relative import | 0.9 | Heuristic: dot-counting + package detection |
| TS path alias (tsconfig) | 0.85 | Depends on alias configuration being correct |
| TS/generic relative import | 0.9 | Path arithmetic, may not account for index files |
| Unknown module (not in known_modules) | 1.0 | The import string is accurate; we just don't know the target |

### 4. CLI Pipeline — Confidence Application

The CLI pipeline code that iterates extracted edges and calls `resolver.resolve()` applies confidence in two steps:

1. **Resolver confidence:** Take the `f64` from `resolve()` and set it on the edge via `with_confidence()`, using `ConfidenceKind::Inferred` for any resolution that transforms the raw string (relative imports, aliases).

2. **Ambiguous downgrade:** If `is_local == false` after resolution (target not in project), override to `min(current_confidence, 0.5)` and set kind to `ConfidenceKind::Ambiguous`. Rationale: edges pointing outside the project to unknown modules are inherently less reliable for architectural analysis.

Decision logic (applied per-edge after resolution):

```
resolver_confidence = resolve(...).2

# Step 1: Apply resolver confidence, but never UPGRADE the extractor's value.
# This ensures bare calls (0.7/Inferred) stay at 0.7 even if the resolver
# returns 1.0 for a known module match.
final_confidence = min(edge.confidence, resolver_confidence)

# Step 2: Set confidence_kind based on whether resolution was heuristic.
if resolver transformed the raw string:
    edge.confidence_kind = Inferred
# (otherwise keep the extractor's original confidence_kind)

# Step 3: Downgrade edges to non-local/unknown targets.
if !is_local:
    final_confidence = min(final_confidence, 0.5)
    edge.confidence_kind = Ambiguous

edge.confidence = final_confidence
```

### 5. Graph Merging — `graph.rs`

When `CodeGraph::add_edge` merges duplicate edges (same kind between same nodes), it currently increments `weight`. For confidence:

- Keep the **maximum** confidence of all merged occurrences
- Keep the `confidence_kind` of the highest-confidence observation

Rationale: if the same relationship is observed multiple times with different confidence levels, the most confident observation is the most informative.

### 6. Report Outputs

#### graph.json

`LinkRecord` gains two fields:

```rust
struct LinkRecord<'a> {
    source: &'a str,
    target: &'a str,
    kind: String,
    weight: u32,
    line: usize,
    confidence: f64,           // NEW
    confidence_kind: String,   // NEW
}
```

#### edges CSV

Header becomes: `source,target,kind,weight,line,confidence,confidence_kind`

#### analysis.json

Add a `confidence_summary` section to the analysis output:

```json
{
  "confidence_summary": {
    "extracted_count": 150,
    "extracted_pct": 78.5,
    "inferred_count": 35,
    "inferred_pct": 18.3,
    "ambiguous_count": 6,
    "ambiguous_pct": 3.1,
    "mean_confidence": 0.92
  }
}
```

This requires passing the `CodeGraph` (or extracted edge stats) to `write_analysis_json`.

#### Markdown report

Add a "Confidence" row to the summary table:

```
| Confidence | 78.5% extracted, 18.3% inferred, 3.1% ambiguous (mean: 0.92) |
```

#### HTML report

The interactive HTML graph already displays edge attributes on hover. Add confidence and confidence_kind to the edge tooltip data. Color edges by confidence kind: green (extracted), yellow (inferred), red (ambiguous).

### 7. Query Interface — `query.rs`

Add `min_confidence: Option<f64>` to `SearchFilters`. When set, edge-based queries (`dependencies`, `dependents`) only traverse edges with `confidence >= min_confidence`.

The `explain` command output includes per-edge confidence in its "Dependencies" and "Dependents" listings:

```
Dependencies:
  → app.services.llm (Imports, confidence: 1.00, Extracted)
  → utils.helpers (Calls, confidence: 0.70, Inferred)
```

### 8. MCP Server — `graphify-mcp`

MCP tools that return edge information (`dependencies`, `dependents`, `explain`) include `confidence` and `confidence_kind` in their JSON output.

The `search` tool gains an optional `min_confidence: f64` parameter that filters results to only include nodes whose edges meet the threshold.

### 9. Confidence Score Table (Summary)

| Source | Edge kind | Default confidence | ConfidenceKind |
|---|---|---|---|
| Direct import (`import os`, `import { x } from 'y'`) | Imports | 1.0 | Extracted |
| From-import (`from x import y`) | Imports + Calls | 1.0 | Extracted |
| Function/class definition | Defines | 1.0 | Extracted |
| Re-export (`export { x } from './y'`) | Imports + Defines | 1.0 | Extracted |
| require() | Imports | 1.0 | Extracted |
| Bare call site (`bar()`) | Calls | 0.7 | Inferred |
| Resolved Python relative import | (adjusted) | 0.9 | Inferred |
| Resolved TS alias | (adjusted) | 0.85 | Inferred |
| Resolved TS relative import | (adjusted) | 0.9 | Inferred |
| Edge to non-local/unknown target | (downgraded) | min(current, 0.5) | Ambiguous |

### 10. Test Plan

| Test | Location | What it verifies |
|---|---|---|
| Edge constructors default to confidence 1.0, Extracted | `types.rs` | Default values |
| `with_confidence()` builder sets fields correctly | `types.rs` | Builder pattern |
| Edge Eq works with confidence field | `types.rs` | Manual Eq impl |
| Edge serialization roundtrip includes confidence | `types.rs` | Serde |
| Python extractor: import edges are Extracted/1.0 | `python.rs` | Extraction defaults |
| Python extractor: bare calls are Inferred/0.7 | `python.rs` | Call site confidence |
| TS extractor: same patterns as Python | `typescript.rs` | Extraction defaults |
| Resolver returns confidence for each path | `resolver.rs` | Resolution confidence |
| Graph merge keeps max confidence | `graph.rs` | Merge behavior |
| Ambiguous downgrade for non-local edges | CLI pipeline | Pipeline logic |
| JSON output includes confidence fields | `json.rs` | Report serialization |
| CSV output includes confidence columns | `csv.rs` | Report serialization |
| Analysis JSON includes confidence_summary | `json.rs` | Aggregate stats |
| Query filters by min_confidence | `query.rs` | Query filtering |
| Explain output shows per-edge confidence | `query.rs` | Query display |

## Files Modified

| File | Change |
|---|---|
| `crates/graphify-core/src/types.rs` | Add `ConfidenceKind`, confidence fields, manual Eq, builder |
| `crates/graphify-core/src/graph.rs` | Merge logic: keep max confidence |
| `crates/graphify-core/src/query.rs` | `min_confidence` filter, explain output |
| `crates/graphify-extract/src/python.rs` | Bare calls → Inferred/0.7 |
| `crates/graphify-extract/src/typescript.rs` | Bare calls → Inferred/0.7 |
| `crates/graphify-extract/src/resolver.rs` | Return `(String, bool, f64)` |
| `crates/graphify-report/src/json.rs` | LinkRecord + confidence_summary |
| `crates/graphify-report/src/csv.rs` | Edges CSV columns |
| `crates/graphify-report/src/markdown.rs` | Summary row |
| `crates/graphify-report/src/html.rs` | Edge tooltip + color |
| `crates/graphify-cli/src/main.rs` | Pipeline: apply resolver confidence + ambiguous downgrade |
| `crates/graphify-mcp/src/server.rs` | Include confidence in tool outputs, min_confidence param |

## Backward Compatibility

- **graph.json**: New fields added — consumers that don't expect them will ignore them (additive change)
- **analysis.json**: New `confidence_summary` section — additive
- **edges CSV**: Two new columns appended — tools reading by position may break, but column-name-based readers are fine
- **CLI flags**: No breaking changes — `--min-confidence` is a new optional flag
- **MCP tools**: New optional parameter + new fields in output — additive
