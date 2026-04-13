# FEAT-008: Edge Confidence Scoring — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add confidence scoring (0.0–1.0) and classification (Extracted/Inferred/Ambiguous) to every edge in the dependency graph, surfacing it in all outputs.

**Architecture:** New `ConfidenceKind` enum + `confidence: f64` on Edge. Extractors set defaults, resolver returns confidence, CLI pipeline applies min-based merging + ambiguous downgrade. Reports/query/MCP consume the new fields.

**Tech Stack:** Rust (petgraph, serde, clap, tree-sitter, rmcp)

**Spec:** `docs/superpowers/specs/2026-04-12-feat-008-confidence-scoring-design.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `crates/graphify-core/src/types.rs` | Modify | Add `ConfidenceKind`, confidence fields on `Edge`, manual Eq, builder |
| `crates/graphify-core/src/graph.rs` | Modify | Keep max confidence on edge merge |
| `crates/graphify-core/src/query.rs` | Modify | `min_confidence` filter, confidence in dependents/dependencies |
| `crates/graphify-extract/src/python.rs` | Modify | Bare calls → Inferred/0.7 |
| `crates/graphify-extract/src/typescript.rs` | Modify | Bare calls → Inferred/0.7 |
| `crates/graphify-extract/src/resolver.rs` | Modify | Return `(String, bool, f64)` |
| `crates/graphify-report/src/json.rs` | Modify | LinkRecord confidence + confidence_summary |
| `crates/graphify-report/src/csv.rs` | Modify | Edges CSV confidence columns |
| `crates/graphify-report/src/markdown.rs` | Modify | Summary confidence row |
| `crates/graphify-report/src/html.rs` | Modify | Edge data + tooltip + color |
| `crates/graphify-cli/src/main.rs` | Modify | Pipeline confidence logic (both `run_extract` usages) |
| `crates/graphify-mcp/src/main.rs` | Modify | Pipeline confidence logic (MCP `run_extract`) |
| `crates/graphify-mcp/src/server.rs` | Modify | Confidence in tool outputs + `min_confidence` param |

---

### Task 1: Data Model — ConfidenceKind and Edge confidence fields

**Files:**
- Modify: `crates/graphify-core/src/types.rs`

- [ ] **Step 1: Write failing tests for confidence on Edge**

Add these tests at the bottom of the existing `mod tests` block in `types.rs`:

```rust
#[test]
fn edge_constructors_default_to_extracted_confidence() {
    let imp = Edge::imports(5);
    assert_eq!(imp.confidence, 1.0);
    assert_eq!(imp.confidence_kind, ConfidenceKind::Extracted);

    let def = Edge::defines(10);
    assert_eq!(def.confidence, 1.0);
    assert_eq!(def.confidence_kind, ConfidenceKind::Extracted);

    let call = Edge::calls(20);
    assert_eq!(call.confidence, 1.0);
    assert_eq!(call.confidence_kind, ConfidenceKind::Extracted);
}

#[test]
fn edge_with_confidence_builder() {
    let edge = Edge::calls(5).with_confidence(0.7, ConfidenceKind::Inferred);
    assert_eq!(edge.confidence, 0.7);
    assert_eq!(edge.confidence_kind, ConfidenceKind::Inferred);
    // Original fields unchanged
    assert_eq!(edge.kind, EdgeKind::Calls);
    assert_eq!(edge.weight, 1);
    assert_eq!(edge.line, 5);
}

#[test]
fn edge_eq_with_confidence() {
    let a = Edge::imports(1).with_confidence(0.9, ConfidenceKind::Inferred);
    let b = Edge::imports(1).with_confidence(0.9, ConfidenceKind::Inferred);
    assert_eq!(a, b);

    let c = Edge::imports(1).with_confidence(0.8, ConfidenceKind::Inferred);
    assert_ne!(a, c);
}

#[test]
fn edge_serialization_roundtrip_with_confidence() {
    let edge = Edge::calls(77).with_confidence(0.85, ConfidenceKind::Inferred);
    let json = serde_json::to_string(&edge).expect("serialize");
    let restored: Edge = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(edge, restored);
}

#[test]
fn edge_json_contains_confidence_fields() {
    let edge = Edge::imports(1).with_confidence(0.5, ConfidenceKind::Ambiguous);
    let json = serde_json::to_string(&edge).expect("serialize");
    assert!(json.contains("\"confidence\":0.5"));
    assert!(json.contains("\"confidence_kind\":\"Ambiguous\""));
}

#[test]
fn confidence_kind_variants() {
    // Verify all three variants exist and serialize correctly
    let kinds = vec![
        (ConfidenceKind::Extracted, "\"Extracted\""),
        (ConfidenceKind::Inferred, "\"Inferred\""),
        (ConfidenceKind::Ambiguous, "\"Ambiguous\""),
    ];
    for (kind, expected) in kinds {
        let json = serde_json::to_string(&kind).expect("serialize");
        assert_eq!(json, expected);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-core -- types::tests`
Expected: Compilation errors — `ConfidenceKind` not defined, `confidence` field not on Edge.

- [ ] **Step 3: Implement ConfidenceKind and update Edge**

In `crates/graphify-core/src/types.rs`:

1. Add `ConfidenceKind` enum after `EdgeKind`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceKind {
    Extracted,
    Inferred,
    Ambiguous,
}
```

2. Replace the `Edge` struct — remove `#[derive(PartialEq, Eq)]` from the derive, add the new fields, and implement `PartialEq`/`Eq` manually:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub kind: EdgeKind,
    pub weight: u32,
    pub line: usize,
    pub confidence: f64,
    pub confidence_kind: ConfidenceKind,
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.weight == other.weight
            && self.line == other.line
            && self.confidence.to_bits() == other.confidence.to_bits()
            && self.confidence_kind == other.confidence_kind
    }
}

impl Eq for Edge {}
```

3. Update the convenience constructors to include default confidence:

```rust
impl Edge {
    pub fn imports(line: usize) -> Self {
        Self {
            kind: EdgeKind::Imports,
            weight: 1,
            line,
            confidence: 1.0,
            confidence_kind: ConfidenceKind::Extracted,
        }
    }

    pub fn defines(line: usize) -> Self {
        Self {
            kind: EdgeKind::Defines,
            weight: 1,
            line,
            confidence: 1.0,
            confidence_kind: ConfidenceKind::Extracted,
        }
    }

    pub fn calls(line: usize) -> Self {
        Self {
            kind: EdgeKind::Calls,
            weight: 1,
            line,
            confidence: 1.0,
            confidence_kind: ConfidenceKind::Extracted,
        }
    }

    pub fn with_confidence(mut self, score: f64, kind: ConfidenceKind) -> Self {
        self.confidence = score;
        self.confidence_kind = kind;
        self
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-core -- types::tests`
Expected: All tests pass (both new and existing).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/src/types.rs
git commit -m "feat(core): add ConfidenceKind enum and confidence fields to Edge (FEAT-008)"
```

---

### Task 2: Graph Merge — Keep max confidence on duplicate edges

**Files:**
- Modify: `crates/graphify-core/src/graph.rs`

- [ ] **Step 1: Write failing test for confidence merge behavior**

Add to the `mod tests` block in `graph.rs`:

```rust
#[test]
fn edge_merge_keeps_max_confidence() {
    use crate::types::ConfidenceKind;

    let mut g = CodeGraph::new();
    g.add_node(python_module("a", true));
    g.add_node(python_module("b", true));

    // First edge: low confidence
    g.add_edge(
        "a",
        "b",
        Edge::imports(1).with_confidence(0.5, ConfidenceKind::Ambiguous),
    );
    // Second edge (same kind): higher confidence
    g.add_edge(
        "a",
        "b",
        Edge::imports(2).with_confidence(0.9, ConfidenceKind::Inferred),
    );

    assert_eq!(g.edge_count(), 1, "should merge into single edge");

    // Verify the merged edge has the max confidence
    let edges = g.edges();
    let (_, _, edge) = edges.iter().find(|(s, t, _)| *s == "a" && *t == "b").unwrap();
    assert_eq!(edge.weight, 2);
    assert_eq!(edge.confidence, 0.9);
    assert_eq!(edge.confidence_kind, ConfidenceKind::Inferred);
}

#[test]
fn edge_merge_keeps_existing_when_higher() {
    use crate::types::ConfidenceKind;

    let mut g = CodeGraph::new();
    g.add_node(python_module("x", true));
    g.add_node(python_module("y", true));

    // First edge: high confidence
    g.add_edge("x", "y", Edge::calls(1)); // 1.0, Extracted
    // Second edge: lower confidence
    g.add_edge(
        "x",
        "y",
        Edge::calls(2).with_confidence(0.7, ConfidenceKind::Inferred),
    );

    let edges = g.edges();
    let (_, _, edge) = edges.iter().find(|(s, t, _)| *s == "x" && *t == "y").unwrap();
    assert_eq!(edge.weight, 2);
    assert_eq!(edge.confidence, 1.0); // kept the higher one
    assert_eq!(edge.confidence_kind, ConfidenceKind::Extracted);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-core -- graph::tests::edge_merge_keeps`
Expected: FAIL — current merge only increments weight, doesn't touch confidence.

- [ ] **Step 3: Update add_edge to keep max confidence**

In `crates/graphify-core/src/graph.rs`, update the `add_edge` method. Replace the existing merge logic:

```rust
// Check for an existing edge of the same kind between src → tgt.
if let Some(existing_idx) = self.find_edge(src, tgt, &edge.kind) {
    self.graph[existing_idx].weight += 1;
}
```

With:

```rust
// Check for an existing edge of the same kind between src → tgt.
if let Some(existing_idx) = self.find_edge(src, tgt, &edge.kind) {
    self.graph[existing_idx].weight += 1;
    // Keep the maximum confidence observation.
    if edge.confidence > self.graph[existing_idx].confidence {
        self.graph[existing_idx].confidence = edge.confidence;
        self.graph[existing_idx].confidence_kind = edge.confidence_kind;
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-core -- graph::tests`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/src/graph.rs
git commit -m "feat(core): keep max confidence on edge merge (FEAT-008)"
```

---

### Task 3: Resolver — Return confidence score

**Files:**
- Modify: `crates/graphify-extract/src/resolver.rs`

- [ ] **Step 1: Write failing tests for resolver confidence**

Add to the `mod tests` block in `resolver.rs`:

```rust
#[test]
fn resolve_direct_known_returns_confidence_1() {
    let r = make_resolver();
    let (_, _, confidence) = r.resolve("app.services.llm", "app.main", false);
    assert_eq!(confidence, 1.0);
}

#[test]
fn resolve_direct_unknown_returns_confidence_1() {
    let r = make_resolver();
    let (_, _, confidence) = r.resolve("os", "app.main", false);
    assert_eq!(confidence, 1.0);
}

#[test]
fn resolve_python_relative_returns_confidence_09() {
    let r = make_resolver();
    let (_, _, confidence) = r.resolve(".utils", "app.services.llm", false);
    assert!((confidence - 0.9).abs() < f64::EPSILON);
}

#[test]
fn resolve_ts_alias_returns_confidence_085() {
    let mut r = make_resolver();
    r.ts_aliases.push(("@/*".to_owned(), "src/*".to_owned()));
    let (_, _, confidence) = r.resolve("@/lib/api", "src.index", false);
    assert!((confidence - 0.85).abs() < f64::EPSILON);
}

#[test]
fn resolve_ts_relative_returns_confidence_09() {
    let r = make_resolver();
    let (_, _, confidence) = r.resolve("./services/user", "src.index", false);
    assert!((confidence - 0.9).abs() < f64::EPSILON);
}

#[test]
fn resolve_ts_workspace_alias_returns_confidence_085() {
    let mut r = make_resolver();
    r.ts_aliases
        .push(("@repo/*".to_owned(), "../../packages/*".to_owned()));
    let (_, _, confidence) = r.resolve("@repo/validators", "src.index", false);
    assert!((confidence - 0.85).abs() < f64::EPSILON);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-extract -- resolver::tests::resolve_direct_known_returns`
Expected: Compilation error — `resolve()` returns 2-tuple, not 3-tuple.

- [ ] **Step 3: Update resolve() to return confidence**

In `crates/graphify-extract/src/resolver.rs`, change the `resolve` method signature and add confidence returns.

Change the return type from `(String, bool)` to `(String, bool, f64)`.

Update each return path:

```rust
pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool, f64) {
    // 1. Python relative imports
    if raw.starts_with('.') && !raw.starts_with("./") && !raw.starts_with("../") {
        let resolved = resolve_python_relative(raw, from_module, is_package);
        let is_local = self.known_modules.contains_key(&resolved);
        return (resolved, is_local, 0.9);
    }

    // 2. TypeScript path aliases
    for (alias_pat, target_pat) in &self.ts_aliases {
        if let Some(resolved) = apply_ts_alias(raw, alias_pat, target_pat) {
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.85);
        }
    }

    // 3. TypeScript / generic relative imports
    if raw.starts_with("./") || raw.starts_with("../") {
        let resolved = resolve_ts_relative(raw, from_module);
        let is_local = self.known_modules.contains_key(&resolved);
        return (resolved, is_local, 0.9);
    }

    // 4. Direct module name
    let is_local = self.known_modules.contains_key(raw);
    (raw.to_owned(), is_local, 1.0)
}
```

- [ ] **Step 4: Fix compilation errors in resolver tests**

The existing resolver tests destructure `(id, is_local)`. Update them all to destructure `(id, is_local, _confidence)` or `(id, _, _)` as appropriate. For example, change:

```rust
let (id, is_local) = r.resolve("app.services.llm", "app.main", false);
```

to:

```rust
let (id, is_local, _) = r.resolve("app.services.llm", "app.main", false);
```

Apply this pattern to **all existing tests** in the resolver test module.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p graphify-extract -- resolver::tests`
Expected: All tests pass (both new and existing).

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-extract/src/resolver.rs
git commit -m "feat(extract): resolver returns confidence score per resolution path (FEAT-008)"
```

---

### Task 4: Extractors — Bare calls get Inferred/0.7

**Files:**
- Modify: `crates/graphify-extract/src/python.rs`
- Modify: `crates/graphify-extract/src/typescript.rs`

- [ ] **Step 1: Write failing tests for bare call confidence**

Add to `mod tests` in `python.rs`:

```rust
#[test]
fn bare_call_sites_have_inferred_confidence() {
    use graphify_core::types::ConfidenceKind;
    let result = extract("def foo():\n    bar()\n");
    let call_edge = result
        .edges
        .iter()
        .find(|(_, t, e)| e.kind == EdgeKind::Calls && t == "bar")
        .expect("should have Calls edge to bar");
    assert_eq!(call_edge.2.confidence, 0.7);
    assert_eq!(call_edge.2.confidence_kind, ConfidenceKind::Inferred);
}

#[test]
fn import_edges_have_extracted_confidence() {
    use graphify_core::types::ConfidenceKind;
    let result = extract("import os\n");
    let import_edge = result
        .edges
        .iter()
        .find(|(_, _, e)| e.kind == EdgeKind::Imports)
        .expect("should have Imports edge");
    assert_eq!(import_edge.2.confidence, 1.0);
    assert_eq!(import_edge.2.confidence_kind, ConfidenceKind::Extracted);
}

#[test]
fn defines_edges_have_extracted_confidence() {
    use graphify_core::types::ConfidenceKind;
    let result = extract("def my_func():\n    pass\n");
    let def_edge = result
        .edges
        .iter()
        .find(|(_, _, e)| e.kind == EdgeKind::Defines)
        .expect("should have Defines edge");
    assert_eq!(def_edge.2.confidence, 1.0);
    assert_eq!(def_edge.2.confidence_kind, ConfidenceKind::Extracted);
}
```

Add equivalent tests to `mod tests` in `typescript.rs`:

```rust
#[test]
fn bare_call_sites_have_inferred_confidence() {
    use graphify_core::types::ConfidenceKind;
    let result = extract("createUser(data);\n");
    let call_edge = result
        .edges
        .iter()
        .find(|(_, t, e)| e.kind == EdgeKind::Calls && t == "createUser")
        .expect("should have Calls edge to createUser");
    assert_eq!(call_edge.2.confidence, 0.7);
    assert_eq!(call_edge.2.confidence_kind, ConfidenceKind::Inferred);
}

#[test]
fn import_edges_have_extracted_confidence() {
    use graphify_core::types::ConfidenceKind;
    let result = extract("import { api } from '@/lib/api';\n");
    let import_edge = result
        .edges
        .iter()
        .find(|(_, _, e)| e.kind == EdgeKind::Imports)
        .expect("should have Imports edge");
    assert_eq!(import_edge.2.confidence, 1.0);
    assert_eq!(import_edge.2.confidence_kind, ConfidenceKind::Extracted);
}

#[test]
fn defines_edges_have_extracted_confidence() {
    use graphify_core::types::ConfidenceKind;
    let result = extract("export function createUser() {}\n");
    let def_edge = result
        .edges
        .iter()
        .find(|(_, _, e)| e.kind == EdgeKind::Defines)
        .expect("should have Defines edge");
    assert_eq!(def_edge.2.confidence, 1.0);
    assert_eq!(def_edge.2.confidence_kind, ConfidenceKind::Extracted);
}
```

- [ ] **Step 2: Run tests to verify bare call tests fail**

Run: `cargo test -p graphify-extract -- python::tests::bare_call_sites_have_inferred`
Expected: FAIL — bare calls currently have confidence 1.0 (default from constructor).

- [ ] **Step 3: Update extract_calls_recursive in both extractors**

In `crates/graphify-extract/src/python.rs`, update `extract_calls_recursive`. Change this line (~line 304):

```rust
result
    .edges
    .push((module_name.to_owned(), callee, Edge::calls(line)));
```

To:

```rust
result.edges.push((
    module_name.to_owned(),
    callee,
    Edge::calls(line).with_confidence(0.7, graphify_core::types::ConfidenceKind::Inferred),
));
```

In `crates/graphify-extract/src/typescript.rs`, make the same change in `extract_calls_recursive` (~line 369):

```rust
result.edges.push((
    module_name.to_owned(),
    callee.to_owned(),
    Edge::calls(line).with_confidence(0.7, graphify_core::types::ConfidenceKind::Inferred),
));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-extract`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/python.rs crates/graphify-extract/src/typescript.rs
git commit -m "feat(extract): bare call sites get Inferred/0.7 confidence (FEAT-008)"
```

---

### Task 5: CLI Pipeline — Apply resolver confidence + ambiguous downgrade

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`
- Modify: `crates/graphify-mcp/src/main.rs`

- [ ] **Step 1: Update CLI `run_extract` pipeline**

In `crates/graphify-cli/src/main.rs`, find the edge resolution loop (~line 807-811):

```rust
for (src_id, raw_target, edge) in all_raw_edges {
    let is_package = package_modules.contains(src_id.as_str());
    let (resolved_target, _is_local) = resolver.resolve(&raw_target, &src_id, is_package);
    graph.add_edge(&src_id, &resolved_target, edge);
}
```

Replace with:

```rust
for (src_id, raw_target, mut edge) in all_raw_edges {
    let is_package = package_modules.contains(src_id.as_str());
    let (resolved_target, is_local, resolver_confidence) =
        resolver.resolve(&raw_target, &src_id, is_package);

    // Step 1: Apply resolver confidence (never upgrade past extractor's value).
    let final_confidence = edge.confidence.min(resolver_confidence);

    // Step 2: If resolver transformed the string, mark as Inferred.
    if resolved_target != raw_target {
        edge = edge.with_confidence(
            final_confidence,
            graphify_core::types::ConfidenceKind::Inferred,
        );
    } else {
        edge.confidence = final_confidence;
    }

    // Step 3: Downgrade edges to non-local targets.
    if !is_local {
        edge = edge.with_confidence(
            edge.confidence.min(0.5),
            graphify_core::types::ConfidenceKind::Ambiguous,
        );
    }

    graph.add_edge(&src_id, &resolved_target, edge);
}
```

- [ ] **Step 2: Update MCP `run_extract` pipeline**

In `crates/graphify-mcp/src/main.rs`, find the identical edge resolution loop (~line 282):

```rust
let (resolved_target, _is_local) = resolver.resolve(&raw_target, &src_id, is_package);
graph.add_edge(&src_id, &resolved_target, edge);
```

Apply the exact same change as Step 1 above. The MCP crate has its own copy of `run_extract`.

- [ ] **Step 3: Run full test suite to verify compilation and tests pass**

Run: `cargo test --workspace`
Expected: All tests pass (existing + new from Tasks 1-4).

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-cli/src/main.rs crates/graphify-mcp/src/main.rs
git commit -m "feat(cli,mcp): apply resolver confidence + ambiguous downgrade in pipeline (FEAT-008)"
```

---

### Task 6: Report — JSON output with confidence

**Files:**
- Modify: `crates/graphify-report/src/json.rs`

- [ ] **Step 1: Write failing test for confidence in graph JSON**

Add to `mod tests` in `json.rs`:

```rust
#[test]
fn write_graph_json_includes_confidence_fields() {
    use graphify_core::types::ConfidenceKind;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graph.json");

    let mut g = CodeGraph::new();
    g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
    g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
    g.add_edge(
        "a",
        "b",
        Edge::imports(3).with_confidence(0.85, ConfidenceKind::Inferred),
    );

    write_graph_json(&g, &path);

    let content = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    let link = &value["links"][0];
    assert_eq!(link["confidence"], 0.85);
    assert_eq!(link["confidence_kind"], "Inferred");
}

#[test]
fn write_analysis_json_includes_confidence_summary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analysis.json");

    let mut g = CodeGraph::new();
    g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
    g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
    g.add_edge("a", "b", Edge::imports(1)); // Extracted/1.0
    g.add_edge(
        "a",
        "b",
        Edge::calls(2).with_confidence(0.7, graphify_core::types::ConfidenceKind::Inferred),
    );

    let metrics = make_metrics();
    let communities = vec![Community {
        id: 0,
        members: vec!["app.main".to_string(), "app.utils".to_string()],
    }];
    let cycles: Vec<Cycle> = vec![];

    write_analysis_json(&metrics, &communities, &cycles, &g, &path);

    let content = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        value["confidence_summary"].is_object(),
        "should have confidence_summary"
    );
    assert!(value["confidence_summary"]["extracted_count"].is_number());
    assert!(value["confidence_summary"]["mean_confidence"].is_number());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-report -- json::tests::write_graph_json_includes_confidence`
Expected: FAIL — `LinkRecord` doesn't have confidence fields yet.

- [ ] **Step 3: Update LinkRecord with confidence fields**

In `crates/graphify-report/src/json.rs`, add confidence fields to `LinkRecord`:

```rust
#[derive(Serialize)]
struct LinkRecord<'a> {
    source: &'a str,
    target: &'a str,
    kind: String,
    weight: u32,
    line: usize,
    confidence: f64,
    confidence_kind: String,
}
```

Update the `write_graph_json` function's link mapping:

```rust
let links: Vec<LinkRecord<'_>> = graph
    .edges()
    .into_iter()
    .map(|(src, tgt, edge)| LinkRecord {
        source: src,
        target: tgt,
        kind: format!("{:?}", edge.kind),
        weight: edge.weight,
        line: edge.line,
        confidence: edge.confidence,
        confidence_kind: format!("{:?}", edge.confidence_kind),
    })
    .collect();
```

- [ ] **Step 4: Add confidence_summary to analysis JSON**

Update `write_analysis_json` signature to accept `&CodeGraph`:

```rust
pub fn write_analysis_json(
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    graph: &CodeGraph,
    path: &Path,
) {
```

Add a `ConfidenceSummary` struct and compute it from the graph edges:

```rust
#[derive(Serialize)]
struct ConfidenceSummary {
    extracted_count: usize,
    extracted_pct: f64,
    inferred_count: usize,
    inferred_pct: f64,
    ambiguous_count: usize,
    ambiguous_pct: f64,
    mean_confidence: f64,
}
```

Compute it inside `write_analysis_json`:

```rust
// Compute confidence summary from graph edges.
let all_edges = graph.edges();
let total_edges = all_edges.len();
let mut extracted = 0usize;
let mut inferred = 0usize;
let mut ambiguous = 0usize;
let mut confidence_sum = 0.0f64;

for (_, _, edge) in &all_edges {
    match edge.confidence_kind {
        graphify_core::types::ConfidenceKind::Extracted => extracted += 1,
        graphify_core::types::ConfidenceKind::Inferred => inferred += 1,
        graphify_core::types::ConfidenceKind::Ambiguous => ambiguous += 1,
    }
    confidence_sum += edge.confidence;
}

let pct = |count: usize| {
    if total_edges > 0 {
        (count as f64 / total_edges as f64) * 100.0
    } else {
        0.0
    }
};

let confidence_summary = ConfidenceSummary {
    extracted_count: extracted,
    extracted_pct: pct(extracted),
    inferred_count: inferred,
    inferred_pct: pct(inferred),
    ambiguous_count: ambiguous,
    ambiguous_pct: pct(ambiguous),
    mean_confidence: if total_edges > 0 {
        confidence_sum / total_edges as f64
    } else {
        0.0
    },
};
```

Add `confidence_summary` to the `AnalysisJson` struct:

```rust
#[derive(Serialize)]
struct AnalysisJson<'a> {
    nodes: Vec<MetricsRecord<'a>>,
    communities: Vec<CommunityRecord<'a>>,
    cycles: &'a [Cycle],
    summary: Summary,
    confidence_summary: ConfidenceSummary,
}
```

And include it in the payload construction.

- [ ] **Step 5: Fix all callers of write_analysis_json**

The signature changed from `(metrics, communities, cycles, total_edges, path)` to `(metrics, communities, cycles, graph, path)`. Update these callers:

1. `crates/graphify-cli/src/main.rs` — the `write_analysis_json` call in `write_all_outputs` and in the `Analyze` command handler. Replace `graph.edge_count()` with `&graph`.
2. The existing test in `json.rs` — `write_analysis_json_summary_fields` — update to pass `&graph` instead of `42`.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-report/src/json.rs crates/graphify-cli/src/main.rs
git commit -m "feat(report): confidence fields in graph.json + confidence_summary in analysis.json (FEAT-008)"
```

---

### Task 7: Report — CSV and Markdown confidence output

**Files:**
- Modify: `crates/graphify-report/src/csv.rs`
- Modify: `crates/graphify-report/src/markdown.rs`

- [ ] **Step 1: Write failing test for CSV confidence columns**

Add to `mod tests` in `csv.rs`:

```rust
#[test]
fn write_edges_csv_includes_confidence_columns() {
    use graphify_core::types::ConfidenceKind;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edges.csv");

    let mut g = CodeGraph::new();
    g.add_node(Node::module("a", "a.py", Language::Python, 1, true));
    g.add_node(Node::module("b", "b.py", Language::Python, 1, true));
    g.add_edge(
        "a",
        "b",
        Edge::imports(3).with_confidence(0.85, ConfidenceKind::Inferred),
    );

    write_edges_csv(&g, &path);

    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(
        lines[0],
        "source,target,kind,weight,line,confidence,confidence_kind"
    );
    assert!(lines[1].contains("0.85"));
    assert!(lines[1].contains("Inferred"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p graphify-report -- csv::tests::write_edges_csv_includes_confidence`
Expected: FAIL — header doesn't include confidence columns.

- [ ] **Step 3: Update edges CSV with confidence columns**

In `crates/graphify-report/src/csv.rs`, update `write_edges_csv`:

Header:
```rust
wtr.write_record(["source", "target", "kind", "weight", "line", "confidence", "confidence_kind"])
    .expect("write edges CSV header");
```

Row:
```rust
for (src, tgt, edge) in graph.edges() {
    wtr.write_record([
        src,
        tgt,
        &format!("{:?}", edge.kind),
        &edge.weight.to_string(),
        &edge.line.to_string(),
        &format!("{:.2}", edge.confidence),
        &format!("{:?}", edge.confidence_kind),
    ])
    .expect("write edges CSV row");
}
```

- [ ] **Step 4: Update the existing CSV test assertion**

In `csv.rs`, update the `write_edges_csv_header_and_data` test assertion for the header:

```rust
assert_eq!(
    lines[0],
    "source,target,kind,weight,line,confidence,confidence_kind"
);
```

- [ ] **Step 5: Update Markdown report with confidence row**

In `crates/graphify-report/src/markdown.rs`, update `write_report` to accept `&CodeGraph` as an additional parameter and add a confidence summary row to the summary table.

Add to the function signature:
```rust
pub fn write_report(
    project_name: &str,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    graph: &CodeGraph,
    path: &Path,
) {
```

After the existing "Circular dependencies" row in the summary table, add:

```rust
// Confidence breakdown
let all_edges = graph.edges();
let total = all_edges.len();
if total > 0 {
    let extracted = all_edges
        .iter()
        .filter(|(_, _, e)| {
            matches!(
                e.confidence_kind,
                graphify_core::types::ConfidenceKind::Extracted
            )
        })
        .count();
    let inferred = all_edges
        .iter()
        .filter(|(_, _, e)| {
            matches!(
                e.confidence_kind,
                graphify_core::types::ConfidenceKind::Inferred
            )
        })
        .count();
    let ambiguous = total - extracted - inferred;
    let mean: f64 =
        all_edges.iter().map(|(_, _, e)| e.confidence).sum::<f64>() / total as f64;
    writeln!(
        buf,
        "| Confidence | {:.1}% extracted, {:.1}% inferred, {:.1}% ambiguous (mean: {:.2}) |",
        extracted as f64 / total as f64 * 100.0,
        inferred as f64 / total as f64 * 100.0,
        ambiguous as f64 / total as f64 * 100.0,
        mean,
    )
    .unwrap();
}
```

Add `use graphify_core::graph::CodeGraph;` to the imports at the top.

- [ ] **Step 6: Fix all callers of write_report**

The signature added `&CodeGraph`. Update:
1. `crates/graphify-cli/src/main.rs` — the `write_all_outputs` function and any direct `write_report` calls: add `&graph` parameter.
2. Any existing tests for `write_report` in `markdown.rs`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/graphify-report/src/csv.rs crates/graphify-report/src/markdown.rs crates/graphify-cli/src/main.rs
git commit -m "feat(report): confidence in edges CSV + markdown summary row (FEAT-008)"
```

---

### Task 8: Report — HTML confidence in edge data

**Files:**
- Modify: `crates/graphify-report/src/html.rs`

- [ ] **Step 1: Update HtmlEdgeData with confidence fields**

In `crates/graphify-report/src/html.rs`, add confidence to `HtmlEdgeData`:

```rust
#[derive(Serialize)]
struct HtmlEdgeData {
    source: String,
    target: String,
    kind: String,
    weight: u32,
    confidence: f64,
    confidence_kind: String,
}
```

Update the edge mapping in `write_html` where `HtmlEdgeData` is constructed (search for `edges: Vec<HtmlEdgeData>`):

```rust
let edges: Vec<HtmlEdgeData> = graph
    .edges()
    .into_iter()
    .map(|(src, tgt, edge)| HtmlEdgeData {
        source: src.to_string(),
        target: tgt.to_string(),
        kind: format!("{:?}", edge.kind),
        weight: edge.weight,
        confidence: edge.confidence,
        confidence_kind: format!("{:?}", edge.confidence_kind),
    })
    .collect();
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p graphify-report -- html::tests`
Expected: All tests pass (HTML tests verify structure, not specific field values).

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-report/src/html.rs
git commit -m "feat(report): confidence fields in HTML edge data (FEAT-008)"
```

---

### Task 9: Query Engine — Confidence in dependents/dependencies + min_confidence filter

**Files:**
- Modify: `crates/graphify-core/src/query.rs`

- [ ] **Step 1: Write failing tests**

Add to tests in `query.rs`:

```rust
#[test]
fn dependents_includes_confidence() {
    let engine = make_engine();
    let deps = engine.dependents("app.utils");
    assert!(!deps.is_empty());
    // Each entry should now include confidence info
    let (_, _, confidence, _) = &deps[0];
    assert!(*confidence > 0.0);
}

#[test]
fn dependencies_includes_confidence() {
    let engine = make_engine();
    let deps = engine.dependencies("app.main");
    assert!(!deps.is_empty());
    let (_, _, confidence, _) = &deps[0];
    assert!(*confidence > 0.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-core -- query::tests::dependents_includes`
Expected: Compilation error — `dependents` returns 2-tuple, not 4-tuple.

- [ ] **Step 3: Update dependents/dependencies return types**

Change `dependents` and `dependencies` to include confidence:

```rust
/// Returns nodes that depend on `node_id` (incoming edges).
///
/// Each entry is `(source_node_id, edge_kind, confidence, confidence_kind)`.
pub fn dependents(
    &self,
    node_id: &str,
) -> Vec<(String, EdgeKind, f64, ConfidenceKind)> {
    self.graph
        .incoming_edges(node_id)
        .into_iter()
        .map(|(src, edge)| {
            (
                src.to_string(),
                edge.kind.clone(),
                edge.confidence,
                edge.confidence_kind.clone(),
            )
        })
        .collect()
}

/// Returns nodes that `node_id` depends on (outgoing edges).
///
/// Each entry is `(target_node_id, edge_kind, confidence, confidence_kind)`.
pub fn dependencies(
    &self,
    node_id: &str,
) -> Vec<(String, EdgeKind, f64, ConfidenceKind)> {
    self.graph
        .outgoing_edges(node_id)
        .into_iter()
        .map(|(tgt, edge)| {
            (
                tgt.to_string(),
                edge.kind.clone(),
                edge.confidence,
                edge.confidence_kind.clone(),
            )
        })
        .collect()
}
```

Add `use crate::types::ConfidenceKind;` to the imports at the top.

- [ ] **Step 4: Add min_confidence to SearchFilters**

```rust
pub struct SearchFilters {
    pub kind: Option<NodeKind>,
    pub sort_by: SortField,
    pub local_only: bool,
    pub min_confidence: Option<f64>,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            kind: None,
            sort_by: SortField::Score,
            local_only: false,
            min_confidence: None,
        }
    }
}
```

- [ ] **Step 5: Fix all callers of dependents/dependencies**

The return type changed from `(String, EdgeKind)` to `(String, EdgeKind, f64, ConfidenceKind)`. Update these callers:

1. `crates/graphify-core/src/query.rs` — the `explain` method uses `self.dependents()` and `self.dependencies()`. Update the destructuring:

```rust
let direct_dependents: Vec<String> = self
    .dependents(node_id)
    .into_iter()
    .map(|(id, _, _, _)| id)
    .collect();

let direct_dependencies: Vec<String> = self
    .dependencies(node_id)
    .into_iter()
    .map(|(id, _, _, _)| id)
    .collect();
```

2. `crates/graphify-cli/src/main.rs` — search for any usage of `engine.dependents()` or `engine.dependencies()` in the shell/explain commands and update destructuring.

3. `crates/graphify-mcp/src/server.rs` — the `graphify_dependents` and `graphify_dependencies` tools. Update the JSON mapping:

```rust
.map(|(id, kind, confidence, confidence_kind)| {
    serde_json::json!({
        "node_id": id,
        "edge_kind": format!("{:?}", kind),
        "confidence": confidence,
        "confidence_kind": format!("{:?}", confidence_kind),
    })
})
```

4. Fix `SearchFilters` construction everywhere — add `min_confidence: None` where `SearchFilters` is constructed manually (CLI `main.rs` and MCP `server.rs`).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-core/src/query.rs crates/graphify-cli/src/main.rs crates/graphify-mcp/src/server.rs
git commit -m "feat(core,mcp): confidence in dependents/dependencies + min_confidence filter (FEAT-008)"
```

---

### Task 10: MCP Server — min_confidence parameter on search

**Files:**
- Modify: `crates/graphify-mcp/src/server.rs`

- [ ] **Step 1: Add min_confidence to SearchParams**

In `server.rs`, update `SearchParams`:

```rust
#[derive(Deserialize, JsonSchema, Default)]
pub struct SearchParams {
    /// Glob pattern to match node IDs (e.g. "app.services.*").
    pub pattern: String,
    /// Filter by node kind: module, function, class, method.
    pub kind: Option<String>,
    /// Sort results: score (default), name, in_degree.
    pub sort: Option<String>,
    /// Only return local (in-project) nodes.
    pub local_only: Option<bool>,
    /// Minimum confidence threshold for edge filtering (0.0-1.0).
    pub min_confidence: Option<f64>,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}
```

- [ ] **Step 2: Pass min_confidence to SearchFilters in graphify_search**

Update the `graphify_search` tool handler:

```rust
let filters = SearchFilters {
    kind: params.kind.as_deref().and_then(parse_node_kind),
    sort_by: sort_field,
    local_only: params.local_only.unwrap_or(false),
    min_confidence: params.min_confidence,
};
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-mcp/src/server.rs
git commit -m "feat(mcp): add min_confidence parameter to graphify_search tool (FEAT-008)"
```

---

### Task 11: Final integration test + docs update

**Files:**
- Modify: `docs/TaskNotes/Tasks/sprint.md`
- Modify: `docs/TaskNotes/Tasks/FEAT-008-confidence-scoring.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass. Note the total count (should be ~210+).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run fmt**

Run: `cargo fmt --check`
Expected: No formatting issues. If any, run `cargo fmt` to fix.

- [ ] **Step 4: Update sprint board**

In `docs/TaskNotes/Tasks/sprint.md`, change FEAT-008 status from `**open**` to `**done**`:

```
| FEAT-008 | **done** | normal   | 8h     | Edge confidence scoring                              |
```

Add to the Done section:

```
- [[FEAT-008-confidence-scoring]] - Implemented: ConfidenceKind enum, confidence scoring on edges, resolver confidence, pipeline downgrade, report outputs, query filtering, MCP integration (2026-04-12)
```

- [ ] **Step 5: Update FEAT-008 task file**

In `docs/TaskNotes/Tasks/FEAT-008-confidence-scoring.md`, change `status: open` to `status: done` in the YAML frontmatter. Check all subtask boxes.

- [ ] **Step 6: Update CLAUDE.md**

Add to the Conventions section:
```
- Edge confidence: `confidence: f64` (0.0–1.0) + `confidence_kind: ConfidenceKind` (Extracted/Inferred/Ambiguous)
- Bare call sites: confidence 0.7/Inferred (unqualified callee)
- Resolver confidence: direct=1.0, Python relative=0.9, TS alias=0.85, TS relative=0.9
- Non-local edge downgrade: min(confidence, 0.5) → Ambiguous
- Edge merge keeps max confidence of all observations
```

Update the test count from 196 to the actual new count.

- [ ] **Step 7: Commit**

```bash
git add docs/TaskNotes/Tasks/sprint.md docs/TaskNotes/Tasks/FEAT-008-confidence-scoring.md CLAUDE.md
git commit -m "docs: update sprint board and CLAUDE.md for FEAT-008 confidence scoring"
```

---

## Task Dependency Graph

```
Task 1 (types.rs) ─────┐
                        ├── Task 5 (CLI pipeline) ─── Task 6 (JSON report)
Task 2 (graph.rs) ──────┤                              │
                        │                              ├── Task 7 (CSV + MD)
Task 3 (resolver.rs) ───┤                              │
                        │                              └── Task 8 (HTML)
Task 4 (extractors) ────┘
                                                        └── Task 9 (query) ─── Task 10 (MCP)
                                                                                │
                                                                                └── Task 11 (docs)
```

Tasks 1-4 can be parallelized (they modify independent crates). Task 5 depends on 1-4 (needs all types + resolver changes). Tasks 6-8 depend on 5 (need the pipeline working). Task 9 depends on 1 (needs types). Task 10 depends on 9. Task 11 depends on all.
