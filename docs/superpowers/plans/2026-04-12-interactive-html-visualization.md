# FEAT-001: Interactive HTML Visualization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a self-contained HTML report format to Graphify that renders the dependency graph as an interactive force-directed visualization with D3.js.

**Architecture:** A new `html.rs` module in `graphify-report` assembles an HTML page by embedding three compile-time assets via `include_str!` — D3.js (vendored), `graph.js` (our visualization), and `graph.css` (styles). Graph + analysis data is serialized to JSON and injected as a `<script>` block. The CLI gains an `"html"` format option that slots into the existing `write_all_outputs` dispatcher.

**Tech Stack:** Rust (graphify-report crate), D3.js v7, vanilla JavaScript, CSS3

**Spec:** `docs/superpowers/specs/2026-04-12-interactive-html-visualization-design.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `crates/graphify-report/assets/d3.v7.min.js` | Vendored D3.js v7 minified (~260KB) |
| Create | `crates/graphify-report/assets/graph.css` | Page layout, sidebar, controls, animation styles |
| Create | `crates/graphify-report/assets/graph.js` | Force simulation, SVG/Canvas rendering, sidebar, interactivity |
| Create | `crates/graphify-report/src/html.rs` | Data serialization, HTML assembly, `write_html()` |
| Modify | `crates/graphify-report/src/lib.rs` | Add `pub mod html; pub use html::write_html;` |
| Modify | `crates/graphify-cli/src/main.rs:21,539-562` | Add `write_html` import and `"html"` match arm |
| Create | `tests/html_integration.rs` | Integration test for HTML pipeline |

---

### Task 1: Scaffold assets directory and download D3.js

**Files:**
- Create: `crates/graphify-report/assets/d3.v7.min.js`
- Create: `crates/graphify-report/assets/graph.js` (placeholder)
- Create: `crates/graphify-report/assets/graph.css` (placeholder)

- [ ] **Step 1: Create the assets directory**

```bash
mkdir -p crates/graphify-report/assets
```

- [ ] **Step 2: Download D3.js v7 minified**

```bash
curl -sL https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js -o crates/graphify-report/assets/d3.v7.min.js
```

Verify: file should be ~260KB and contain `d3.forceSimulation`.

```bash
wc -c crates/graphify-report/assets/d3.v7.min.js
grep -c "forceSimulation" crates/graphify-report/assets/d3.v7.min.js
```

Expected: file size ~260,000 bytes, grep count >= 1.

- [ ] **Step 3: Create placeholder asset files**

Create `crates/graphify-report/assets/graph.js`:

```javascript
// Graphify interactive visualization — placeholder
(function() { 'use strict'; })();
```

Create `crates/graphify-report/assets/graph.css`:

```css
/* Graphify interactive visualization — placeholder */
body { font-family: sans-serif; }
```

- [ ] **Step 4: Commit scaffolding**

```bash
git add crates/graphify-report/assets/
git commit -m "chore: scaffold HTML report assets with vendored D3.js v7"
```

---

### Task 2: Implement html.rs — data model and HTML assembly (TDD)

**Files:**
- Create: `crates/graphify-report/src/html.rs`
- Modify: `crates/graphify-report/src/lib.rs`

- [ ] **Step 1: Write failing tests for html.rs**

Create `crates/graphify-report/src/html.rs` with tests only:

```rust
use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use graphify_core::{graph::CodeGraph, metrics::NodeMetrics};

use crate::{Community, Cycle};

// Embed assets at compile time.
const D3_JS: &str = include_str!("../assets/d3.v7.min.js");
const GRAPH_JS: &str = include_str!("../assets/graph.js");
const GRAPH_CSS: &str = include_str!("../assets/graph.css");

// ---------------------------------------------------------------------------
// Data structures for the merged JSON blob
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HtmlNodeData {
    id: String,
    kind: String,
    file_path: String,
    language: String,
    line: usize,
    is_local: bool,
    betweenness: f64,
    pagerank: f64,
    in_degree: usize,
    out_degree: usize,
    in_cycle: bool,
    score: f64,
    community_id: usize,
}

#[derive(Serialize)]
struct HtmlEdgeData {
    source: String,
    target: String,
    kind: String,
    weight: u32,
}

#[derive(Serialize)]
struct HtmlCommunityData {
    id: usize,
    members: Vec<String>,
}

#[derive(Serialize)]
struct HtmlSummary {
    total_nodes: usize,
    total_edges: usize,
    total_communities: usize,
    total_cycles: usize,
}

#[derive(Serialize)]
struct HtmlGraphData {
    project_name: String,
    nodes: Vec<HtmlNodeData>,
    edges: Vec<HtmlEdgeData>,
    communities: Vec<HtmlCommunityData>,
    cycles: Vec<Vec<String>>,
    summary: HtmlSummary,
}

// ---------------------------------------------------------------------------
// Public API — placeholder
// ---------------------------------------------------------------------------

/// Generates a self-contained interactive HTML visualization and writes it
/// to `path`.
///
/// The HTML file embeds D3.js, graph.js, graph.css, and the serialized
/// graph + analysis data. It can be opened in any modern browser with no
/// server or internet connection required.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_html(
    _project_name: &str,
    _graph: &CodeGraph,
    _metrics: &[NodeMetrics],
    _communities: &[Community],
    _cycles: &[Cycle],
    _path: &Path,
) {
    todo!("implement write_html")
}

fn build_data(
    _project_name: &str,
    _graph: &CodeGraph,
    _metrics: &[NodeMetrics],
    _communities: &[Community],
    _cycles: &[Cycle],
) -> HtmlGraphData {
    todo!("implement build_data")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::{
        graph::CodeGraph,
        metrics::NodeMetrics,
        types::{Edge, Language, Node},
    };

    fn make_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("app.main", "app/main.py", Language::Python, 1, true));
        g.add_node(Node::module("app.utils", "app/utils.py", Language::Python, 1, true));
        g.add_edge("app.main", "app.utils", Edge::imports(3));
        g
    }

    fn make_metrics() -> Vec<NodeMetrics> {
        vec![
            NodeMetrics {
                id: "app.main".to_string(),
                betweenness: 0.5,
                pagerank: 0.3,
                in_degree: 1,
                out_degree: 2,
                in_cycle: false,
                score: 0.4,
                community_id: 0,
            },
            NodeMetrics {
                id: "app.utils".to_string(),
                betweenness: 0.1,
                pagerank: 0.2,
                in_degree: 0,
                out_degree: 0,
                in_cycle: false,
                score: 0.1,
                community_id: 0,
            },
        ]
    }

    fn make_communities() -> Vec<Community> {
        vec![Community {
            id: 0,
            members: vec!["app.main".to_string(), "app.utils".to_string()],
        }]
    }

    fn make_cycles() -> Vec<Cycle> {
        vec![vec!["app.main".to_string(), "app.utils".to_string()]]
    }

    #[test]
    fn write_html_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "test-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        assert!(path.exists(), "HTML file should be created");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.is_empty(), "HTML file should not be empty");
    }

    #[test]
    fn html_contains_data_block() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "test-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("GRAPHIFY_DATA"), "should contain data block");
        // Extract the JSON and validate it parses
        let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
        let end = content[start..].find(";\n</script>").unwrap() + start;
        let json_str = &content[start..end];
        let value: serde_json::Value = serde_json::from_str(json_str).expect("data should be valid JSON");
        assert_eq!(value["project_name"], "test-project");
        assert_eq!(value["summary"]["total_nodes"], 2);
    }

    #[test]
    fn html_contains_d3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "test-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("forceSimulation"), "should contain D3.js force module");
    }

    #[test]
    fn html_contains_project_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.html");
        write_html(
            "my-cool-project",
            &make_graph(),
            &make_metrics(),
            &make_communities(),
            &make_cycles(),
            &path,
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<title>Graphify: my-cool-project</title>"));
        assert!(content.contains("my-cool-project"));
    }

    #[test]
    fn html_empty_graph() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.html");
        write_html(
            "empty",
            &CodeGraph::new(),
            &[],
            &[],
            &[],
            &path,
        );
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("GRAPHIFY_DATA"));
        let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
        let end = content[start..].find(";\n</script>").unwrap() + start;
        let json_str = &content[start..end];
        let value: serde_json::Value = serde_json::from_str(json_str).expect("should be valid JSON");
        assert_eq!(value["summary"]["total_nodes"], 0);
    }

    #[test]
    fn html_single_node_no_edges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("single.html");
        let mut g = CodeGraph::new();
        g.add_node(Node::module("only.module", "only/module.py", Language::Python, 1, true));
        let metrics = vec![NodeMetrics {
            id: "only.module".to_string(),
            betweenness: 0.0,
            pagerank: 1.0,
            in_degree: 0,
            out_degree: 0,
            in_cycle: false,
            score: 0.0,
            community_id: 0,
        }];
        write_html("single", &g, &metrics, &[], &[], &path);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("only.module"));
    }
}
```

- [ ] **Step 2: Add module to lib.rs**

Replace `crates/graphify-report/src/lib.rs` with:

```rust
pub mod csv;
pub mod html;
pub mod json;
pub mod markdown;

// Re-export the main write functions for convenience.
pub use csv::{write_edges_csv, write_nodes_csv};
pub use html::write_html;
pub use json::{write_analysis_json, write_graph_json};
pub use markdown::write_report;

// Re-export core types used across the report modules.
pub use graphify_core::community::Community;

/// A cycle represented as an ordered list of node IDs.
pub type Cycle = Vec<String>;
```

- [ ] **Step 3: Run tests — verify they fail**

```bash
cargo test -p graphify-report html -- --nocapture
```

Expected: 6 tests FAIL with `not yet implemented: implement write_html`.

- [ ] **Step 4: Implement build_data**

Replace the `build_data` placeholder in `html.rs`:

```rust
fn build_data(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
) -> HtmlGraphData {
    let metrics_map: HashMap<&str, &NodeMetrics> =
        metrics.iter().map(|m| (m.id.as_str(), m)).collect();

    let nodes: Vec<HtmlNodeData> = graph
        .nodes()
        .into_iter()
        .map(|n| {
            let m = metrics_map.get(n.id.as_str());
            HtmlNodeData {
                id: n.id.clone(),
                kind: format!("{:?}", n.kind),
                file_path: n.file_path.to_string_lossy().into_owned(),
                language: format!("{:?}", n.language),
                line: n.line,
                is_local: n.is_local,
                betweenness: m.map_or(0.0, |m| m.betweenness),
                pagerank: m.map_or(0.0, |m| m.pagerank),
                in_degree: m.map_or(0, |m| m.in_degree),
                out_degree: m.map_or(0, |m| m.out_degree),
                in_cycle: m.map_or(false, |m| m.in_cycle),
                score: m.map_or(0.0, |m| m.score),
                community_id: m.map_or(0, |m| m.community_id),
            }
        })
        .collect();

    let edges: Vec<HtmlEdgeData> = graph
        .edges()
        .into_iter()
        .map(|(src, tgt, e)| HtmlEdgeData {
            source: src.to_string(),
            target: tgt.to_string(),
            kind: format!("{:?}", e.kind),
            weight: e.weight,
        })
        .collect();

    let communities_data: Vec<HtmlCommunityData> = communities
        .iter()
        .map(|c| HtmlCommunityData {
            id: c.id,
            members: c.members.clone(),
        })
        .collect();

    HtmlGraphData {
        project_name: project_name.to_string(),
        nodes,
        edges,
        communities: communities_data,
        cycles: cycles.to_vec(),
        summary: HtmlSummary {
            total_nodes: metrics.len(),
            total_edges: graph.edge_count(),
            total_communities: communities.len(),
            total_cycles: cycles.len(),
        },
    }
}
```

- [ ] **Step 5: Implement write_html**

Replace the `write_html` placeholder in `html.rs`:

```rust
pub fn write_html(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Cycle],
    path: &Path,
) {
    let data = build_data(project_name, graph, metrics, communities, cycles);
    let data_json = serde_json::to_string(&data).expect("serialize HTML data");
    // Escape </script> sequences that could appear in node IDs or file paths.
    let data_json = data_json.replace("</script>", r"<\/script>");

    let capacity = D3_JS.len() + GRAPH_JS.len() + GRAPH_CSS.len() + data_json.len() + 4096;
    let mut html = String::with_capacity(capacity);

    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("<title>Graphify: {}</title>\n", project_name));
    html.push_str("<style>\n");
    html.push_str(GRAPH_CSS);
    html.push_str("\n</style>\n");
    html.push_str("</head>\n<body>\n");

    // D3.js library
    html.push_str("<script>\n");
    html.push_str(D3_JS);
    html.push_str("\n</script>\n");

    // Graph data
    html.push_str("<script>\nconst GRAPHIFY_DATA = ");
    html.push_str(&data_json);
    html.push_str(";\n</script>\n");

    // Page structure
    html.push_str(concat!(
        "<div id=\"app\">\n",
        "  <header id=\"header\">\n",
        "    <h1>Graphify: <span id=\"project-name\"></span></h1>\n",
        "    <button id=\"export-png\" title=\"Export as PNG\">PNG</button>\n",
        "  </header>\n",
        "  <div id=\"main\">\n",
        "    <aside id=\"sidebar\"></aside>\n",
        "    <div id=\"viewport\"></div>\n",
        "  </div>\n",
        "  <footer id=\"footer\">\n",
        "    <span id=\"tooltip\">Hover over a node to see details</span>\n",
        "  </footer>\n",
        "</div>\n",
    ));

    // Visualization script
    html.push_str("<script>\n");
    html.push_str(GRAPH_JS);
    html.push_str("\n</script>\n");

    html.push_str("</body>\n</html>\n");

    std::fs::write(path, html).expect("write HTML report");
}
```

- [ ] **Step 6: Run tests — verify they pass**

```bash
cargo test -p graphify-report html -- --nocapture
```

Expected: 6 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-report/src/html.rs crates/graphify-report/src/lib.rs
git commit -m "feat: add html.rs report module with data serialization and HTML assembly"
```

---

### Task 3: Integrate HTML format into CLI

**Files:**
- Modify: `crates/graphify-cli/src/main.rs:21` (import)
- Modify: `crates/graphify-cli/src/main.rs:539-562` (match arm)
- Modify: `crates/graphify-cli/src/main.rs:265-278` (init template)

- [ ] **Step 1: Add write_html import in main.rs**

At `crates/graphify-cli/src/main.rs:20`, change the import block from:

```rust
use graphify_report::{
    write_analysis_json, write_edges_csv, write_graph_json, write_nodes_csv, write_report, Cycle,
};
```

to:

```rust
use graphify_report::{
    write_analysis_json, write_edges_csv, write_graph_json, write_html, write_nodes_csv,
    write_report, Cycle,
};
```

- [ ] **Step 2: Add "html" match arm in write_all_outputs**

In the `write_all_outputs` function (around line 538), add a new match arm inside the `for fmt in formats` loop, after the `"md" | "markdown"` arm:

```rust
"html" => {
    write_html(
        project_name,
        graph,
        metrics,
        communities,
        cycles,
        &out_dir.join("architecture_graph.html"),
    );
}
```

- [ ] **Step 3: Update init template**

In the `cmd_init` function (around line 264), change the commented format line from:

```rust
# format = ["json", "csv", "md"]    # output formats
```

to:

```rust
# format = ["json", "csv", "md", "html"]    # output formats
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo build -p graphify-cli
```

Expected: builds successfully.

- [ ] **Step 5: Run all existing tests to check for regressions**

```bash
cargo test --workspace
```

Expected: all tests pass (existing + 6 new html tests).

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat: integrate HTML format into CLI pipeline and init template"
```

---

### Task 4: Implement graph.css — complete styles

**Files:**
- Modify: `crates/graphify-report/assets/graph.css`

- [ ] **Step 1: Write the complete stylesheet**

Replace the placeholder content of `crates/graphify-report/assets/graph.css` with:

```css
/* Graphify — Interactive Architecture Visualization */

* { margin: 0; padding: 0; box-sizing: border-box; }

html, body {
  height: 100%;
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, sans-serif;
  font-size: 13px;
  color: #333;
  background: #fafafa;
}

/* ── App layout ────────────────────────────────────────── */

#app { display: flex; flex-direction: column; height: 100vh; }

#header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  background: #1a1a2e;
  color: #eee;
  flex-shrink: 0;
}
#header h1 { font-size: 16px; font-weight: 600; }
#header button {
  background: #334;
  border: 1px solid #556;
  color: #ccc;
  padding: 4px 12px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
}
#header button:hover { background: #445; color: #fff; }

#main { display: flex; flex: 1; overflow: hidden; }

#sidebar {
  width: 280px;
  min-width: 280px;
  overflow-y: auto;
  background: #fff;
  border-right: 1px solid #e0e0e0;
  padding: 12px;
  flex-shrink: 0;
}

#viewport {
  flex: 1;
  position: relative;
  background: #f8f9fa;
  overflow: hidden;
}
#viewport svg, #viewport canvas {
  display: block;
  width: 100%;
  height: 100%;
}

#footer {
  padding: 6px 16px;
  background: #1a1a2e;
  color: #aaa;
  font-size: 12px;
  min-height: 30px;
  flex-shrink: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Sidebar sections ──────────────────────────────────── */

.section { margin-bottom: 16px; }

.section-header {
  font-weight: 600;
  font-size: 13px;
  margin-bottom: 8px;
  color: #1a1a2e;
  cursor: pointer;
  user-select: none;
}
.section-header::before { content: '\25BE '; font-size: 10px; }
.section-header.collapsed::before { content: '\25B8 '; }

.section-content { padding-left: 4px; }
.section-content.hidden { display: none; }

/* ── Summary grid ──────────────────────────────────────── */

.summary-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
.summary-item {
  background: #f0f4ff;
  padding: 8px;
  border-radius: 4px;
  text-align: center;
}
.summary-value { font-size: 18px; font-weight: 700; color: #1a1a2e; }
.summary-label { font-size: 11px; color: #666; }

/* ── Filters ───────────────────────────────────────────── */

.filter-group { margin-bottom: 8px; }
.filter-group-label {
  font-size: 11px;
  color: #666;
  text-transform: uppercase;
  font-weight: 600;
  margin-bottom: 4px;
}
.filter-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
  font-size: 12px;
}
.filter-item input[type="checkbox"] { accent-color: #1a1a2e; }

/* ── Communities ───────────────────────────────────────── */

.community-item {
  padding: 4px 8px;
  border-radius: 4px;
  cursor: pointer;
  margin-bottom: 2px;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}
.community-item:hover { background: #f0f0f0; }
.community-item.collapsed-community { opacity: 0.6; font-style: italic; }
.community-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

/* ── Cycles ────────────────────────────────────────────── */

.cycle-item {
  padding: 4px 8px;
  border-radius: 4px;
  cursor: pointer;
  margin-bottom: 2px;
  font-size: 12px;
  color: #666;
}
.cycle-item:hover { background: #fff0f0; color: #c62828; }
.cycle-item.active { background: #ffebee; color: #c62828; font-weight: 600; }

/* ── Force controls ────────────────────────────────────── */

.slider-group { margin-bottom: 8px; }
.slider-label {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: #666;
  margin-bottom: 2px;
}
.slider-group input[type="range"] { width: 100%; cursor: pointer; }

/* ── Search ────────────────────────────────────────────── */

#search-input {
  width: 100%;
  padding: 6px 8px;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 13px;
  font-family: inherit;
}
#search-input:focus { outline: none; border-color: #1a1a2e; box-shadow: 0 0 0 2px rgba(26,26,46,0.1); }

/* ── SVG styles ────────────────────────────────────────── */

.node { cursor: pointer; }
.node circle { stroke: #fff; stroke-width: 1.5; }
.node.dimmed circle { opacity: 0.1; }
.node.search-match circle { stroke: #ff9800; stroke-width: 3; }
.node.highlighted circle { stroke: #333; stroke-width: 2.5; }

.link { fill: none; pointer-events: none; }
.link.dimmed { opacity: 0.05 !important; }

/* ── Marching ants animation ───────────────────────────── */

@keyframes march {
  to { stroke-dashoffset: -20; }
}
.link.marching-ants {
  stroke: #F44336 !important;
  stroke-width: 3 !important;
  stroke-dasharray: 10 5;
  animation: march 0.5s linear infinite;
  opacity: 1 !important;
}

/* ── Empty state ───────────────────────────────────────── */

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #999;
  font-size: 16px;
}
```

- [ ] **Step 2: Verify HTML tests still pass**

```bash
cargo test -p graphify-report html -- --nocapture
```

Expected: 6 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-report/assets/graph.css
git commit -m "feat: implement complete stylesheet for HTML visualization"
```

---

### Task 5: Implement graph.js — complete interactive visualization

**Files:**
- Modify: `crates/graphify-report/assets/graph.js`

All DOM construction uses safe methods (`createElement`, `textContent`, `appendChild`). No `innerHTML`.

- [ ] **Step 1: Write the complete graph.js visualization**

Replace the placeholder content of `crates/graphify-report/assets/graph.js` with:

```javascript
// Graphify — Interactive Architecture Visualization
// Self-contained D3.js force-directed graph explorer.

(function () {
  'use strict';

  // ===========================================================================
  // Constants
  // ===========================================================================

  var CANVAS_THRESHOLD = 300;
  var MIN_R = 4;
  var MAX_R = 20;
  var EDGE_COLORS = { Imports: '#666', Defines: '#2196F3', Calls: '#4CAF50' };
  var CYCLE_COLOR = '#F44336';
  var EDGE_DASH = { Imports: null, Defines: '5,3', Calls: '2,2' };
  var COMMUNITY_COLORS = [
    '#8dd3c7','#ffffb3','#bebada','#fb8072','#80b1d3',
    '#fdb462','#b3de69','#fccde5','#d9d9d9','#bc80bd','#ccebc5','#ffed6f'
  ];

  // ===========================================================================
  // State
  // ===========================================================================

  var data = window.GRAPHIFY_DATA;
  var maxScore = 0.001;
  var maxWeight = 1;
  data.nodes.forEach(function (n) { if (n.score > maxScore) maxScore = n.score; });
  data.edges.forEach(function (e) { if (e.weight > maxWeight) maxWeight = e.weight; });

  // Build cycle edge lookup set
  var cycleEdgeSet = {};
  data.cycles.forEach(function (cycle) {
    for (var i = 0; i < cycle.length - 1; i++) {
      cycleEdgeSet[cycle[i] + '->' + cycle[i + 1]] = true;
    }
  });

  var state = {
    mode: 'svg',
    languages: {},
    edgeKinds: { Imports: true, Defines: true, Calls: true },
    collapsedCommunities: {},
    activeCycle: -1,
    highlightedNode: null,
    searchQuery: '',
    chargeStrength: -120,
    linkDistance: 80,
    centerGravity: 0.05,
    transform: d3.zoomIdentity,
    pinnedNodes: {},
    simulation: null,
    simNodes: [],
    simEdges: [],
    antOffset: 0
  };

  // Detect unique languages
  var langSet = {};
  data.nodes.forEach(function (n) { langSet[n.language] = true; });
  Object.keys(langSet).forEach(function (l) { state.languages[l] = true; });

  // ===========================================================================
  // Helpers
  // ===========================================================================

  function nodeRadius(n) {
    var s = n.isGroup ? n.groupScore : n.score;
    var base = MIN_R + (s / maxScore) * (MAX_R - MIN_R);
    return n.isGroup ? base * 1.5 : base;
  }

  function nodeColor(n) {
    return COMMUNITY_COLORS[n.community_id % COMMUNITY_COLORS.length];
  }

  function edgeOpacity(e) {
    return 0.3 + 0.7 * (e.weight / maxWeight);
  }

  function isInCycle(source, target) {
    return cycleEdgeSet[source + '->' + target] === true;
  }

  function shortName(id) {
    var parts = id.split('.');
    return parts[parts.length - 1];
  }

  // ===========================================================================
  // Working set — applies filters and community collapse
  // ===========================================================================

  function buildWorkingSet() {
    var groupNodes = {};
    var nodes = [];
    var communityOf = {};

    data.nodes.forEach(function (n) { communityOf[n.id] = n.community_id; });

    data.nodes.forEach(function (n) {
      if (!state.languages[n.language]) return;
      if (state.collapsedCommunities[n.community_id]) {
        if (!groupNodes[n.community_id]) {
          groupNodes[n.community_id] = {
            id: '__group_' + n.community_id,
            kind: 'Group',
            community_id: n.community_id,
            score: 0,
            groupScore: 0,
            memberCount: 0,
            isGroup: true,
            language: n.language,
            file_path: '',
            line: 0,
            is_local: true,
            betweenness: 0,
            pagerank: 0,
            in_degree: 0,
            out_degree: 0,
            in_cycle: false
          };
        }
        var g = groupNodes[n.community_id];
        if (n.score > g.groupScore) g.groupScore = n.score;
        g.memberCount++;
        return;
      }
      nodes.push(Object.assign({}, n));
    });

    Object.keys(groupNodes).forEach(function (cid) {
      var g = groupNodes[cid];
      g.label = 'C' + cid + ' (' + g.memberCount + ')';
      nodes.push(g);
    });

    var nodeIdSet = {};
    nodes.forEach(function (n) { nodeIdSet[n.id] = true; });

    var edgeMap = {};
    data.edges.forEach(function (e) {
      if (!state.edgeKinds[e.kind]) return;
      var srcId = e.source;
      var tgtId = e.target;
      var srcC = communityOf[srcId];
      var tgtC = communityOf[tgtId];
      if (srcC !== undefined && state.collapsedCommunities[srcC]) srcId = '__group_' + srcC;
      if (tgtC !== undefined && state.collapsedCommunities[tgtC]) tgtId = '__group_' + tgtC;
      if (srcId === tgtId) return;
      if (!nodeIdSet[srcId] || !nodeIdSet[tgtId]) return;
      var key = srcId + '->' + tgtId;
      if (edgeMap[key]) {
        edgeMap[key].weight += e.weight;
      } else {
        edgeMap[key] = {
          source: srcId, target: tgtId,
          kind: e.kind, weight: e.weight,
          inCycle: isInCycle(e.source, e.target)
        };
      }
    });

    var edges = Object.keys(edgeMap).map(function (k) { return edgeMap[k]; });
    return { nodes: nodes, edges: edges };
  }

  // ===========================================================================
  // Dimming logic
  // ===========================================================================

  function getNodeOpacity(n) {
    if (state.activeCycle >= 0) {
      var cycle = data.cycles[state.activeCycle];
      if (!cycle) return 1;
      return cycle.indexOf(n.id) >= 0 ? 1 : 0.1;
    }
    if (state.highlightedNode) {
      if (n.id === state.highlightedNode) return 1;
      var isNeighbor = state.simEdges.some(function (e) {
        var sid = typeof e.source === 'object' ? e.source.id : e.source;
        var tid = typeof e.target === 'object' ? e.target.id : e.target;
        return (sid === state.highlightedNode && tid === n.id) ||
               (tid === state.highlightedNode && sid === n.id);
      });
      return isNeighbor ? 1 : 0.1;
    }
    if (state.searchQuery) {
      return n.id.toLowerCase().indexOf(state.searchQuery.toLowerCase()) >= 0 ? 1 : 0.3;
    }
    return 1;
  }

  function getEdgeActive(e) {
    var sid = typeof e.source === 'object' ? e.source.id : e.source;
    var tid = typeof e.target === 'object' ? e.target.id : e.target;
    if (state.activeCycle >= 0) {
      var cycle = data.cycles[state.activeCycle];
      if (!cycle) return true;
      for (var i = 0; i < cycle.length - 1; i++) {
        if (cycle[i] === sid && cycle[i + 1] === tid) return true;
      }
      return false;
    }
    if (state.highlightedNode) {
      return sid === state.highlightedNode || tid === state.highlightedNode;
    }
    return true;
  }

  // ===========================================================================
  // Force Simulation
  // ===========================================================================

  function createSimulation() {
    var ws = buildWorkingSet();
    state.simNodes = ws.nodes;
    state.simEdges = ws.edges;

    state.simNodes.forEach(function (n) {
      if (state.pinnedNodes[n.id]) {
        n.fx = state.pinnedNodes[n.id].x;
        n.fy = state.pinnedNodes[n.id].y;
      }
    });

    state.mode = state.simNodes.length > CANVAS_THRESHOLD ? 'canvas' : 'svg';

    if (state.simulation) state.simulation.stop();

    state.simulation = d3.forceSimulation(state.simNodes)
      .force('link', d3.forceLink(state.simEdges).id(function (d) { return d.id; }).distance(state.linkDistance))
      .force('charge', d3.forceManyBody().strength(state.chargeStrength))
      .force('center', d3.forceCenter(0, 0).strength(state.centerGravity))
      .force('collision', d3.forceCollide().radius(function (d) { return nodeRadius(d) + 2; }))
      .alphaDecay(0.02)
      .on('tick', tick);
  }

  // ===========================================================================
  // Rendering — SVG
  // ===========================================================================

  var svgEl, svgGroup, svgLinks, svgNodes, zoomBehavior;
  var canvasEl, canvasCtx;

  function setupViewport() {
    var vp = document.getElementById('viewport');
    while (vp.firstChild) vp.removeChild(vp.firstChild);

    if (state.mode === 'svg') {
      svgEl = d3.select(vp).append('svg');
      svgGroup = svgEl.append('g');
      svgGroup.append('g').attr('class', 'links');
      svgGroup.append('g').attr('class', 'nodes');
      zoomBehavior = d3.zoom().scaleExtent([0.1, 10]).on('zoom', function (event) {
        state.transform = event.transform;
        svgGroup.attr('transform', event.transform);
      });
      svgEl.call(zoomBehavior);
      svgEl.on('dblclick.zoom', function () {
        svgEl.transition().duration(500).call(zoomBehavior.transform, d3.zoomIdentity);
      });
      svgEl.on('click', function (event) {
        if (event.target === svgEl.node()) clearHighlight();
      });
    } else {
      canvasEl = document.createElement('canvas');
      canvasEl.width = vp.clientWidth * window.devicePixelRatio;
      canvasEl.height = vp.clientHeight * window.devicePixelRatio;
      canvasEl.style.width = vp.clientWidth + 'px';
      canvasEl.style.height = vp.clientHeight + 'px';
      vp.appendChild(canvasEl);
      canvasCtx = canvasEl.getContext('2d');
      canvasCtx.scale(window.devicePixelRatio, window.devicePixelRatio);

      zoomBehavior = d3.zoom().scaleExtent([0.1, 10]).on('zoom', function (event) {
        state.transform = event.transform;
      });
      d3.select(canvasEl).call(zoomBehavior);
      d3.select(canvasEl).on('dblclick.zoom', function () {
        d3.select(canvasEl).transition().duration(500).call(zoomBehavior.transform, d3.zoomIdentity);
      });

      canvasEl.addEventListener('mousemove', function (event) {
        var rect = canvasEl.getBoundingClientRect();
        var node = findNodeAt(event.clientX - rect.left, event.clientY - rect.top);
        canvasEl.style.cursor = node ? 'pointer' : 'default';
        if (node) updateTooltip(node); else resetTooltip();
      });
      canvasEl.addEventListener('click', function (event) {
        var rect = canvasEl.getBoundingClientRect();
        var node = findNodeAt(event.clientX - rect.left, event.clientY - rect.top);
        if (node) highlightNode(node.id); else clearHighlight();
      });

      // Canvas drag
      var dragTarget = null;
      d3.select(canvasEl).call(
        d3.drag()
          .subject(function (event) { return findNodeAt(event.x, event.y); })
          .on('start', function (event) {
            dragTarget = event.subject;
            if (dragTarget) {
              state.simulation.alphaTarget(0.3).restart();
              dragTarget.fx = dragTarget.x;
              dragTarget.fy = dragTarget.y;
            }
          })
          .on('drag', function (event) {
            if (dragTarget) {
              var pt = state.transform.invert([event.x, event.y]);
              dragTarget.fx = pt[0];
              dragTarget.fy = pt[1];
            }
          })
          .on('end', function () {
            if (dragTarget) {
              state.simulation.alphaTarget(0);
              state.pinnedNodes[dragTarget.id] = { x: dragTarget.fx, y: dragTarget.fy };
              dragTarget = null;
            }
          })
      );
    }
  }

  function renderSVGGraph() {
    // Edges
    svgLinks = svgGroup.select('.links').selectAll('.link')
      .data(state.simEdges, function (d) {
        var sid = typeof d.source === 'object' ? d.source.id : d.source;
        var tid = typeof d.target === 'object' ? d.target.id : d.target;
        return sid + '->' + tid;
      });
    svgLinks.exit().remove();
    var linkEnter = svgLinks.enter().append('line').attr('class', 'link');
    svgLinks = linkEnter.merge(svgLinks);
    svgLinks.each(function (d) {
      var el = d3.select(this);
      var color = d.inCycle ? CYCLE_COLOR : (EDGE_COLORS[d.kind] || '#999');
      el.attr('stroke', color)
        .attr('stroke-width', d.inCycle ? 2.5 : 1)
        .attr('stroke-opacity', edgeOpacity(d));
      if (EDGE_DASH[d.kind] && !d.inCycle) el.attr('stroke-dasharray', EDGE_DASH[d.kind]);
      else el.attr('stroke-dasharray', null);
    });

    // Nodes
    svgNodes = svgGroup.select('.nodes').selectAll('.node')
      .data(state.simNodes, function (d) { return d.id; });
    svgNodes.exit().remove();
    var nodeEnter = svgNodes.enter().append('g').attr('class', 'node');
    nodeEnter.append('circle');
    nodeEnter.append('text')
      .attr('text-anchor', 'middle')
      .attr('font-size', '9px')
      .attr('fill', '#666')
      .attr('pointer-events', 'none');
    svgNodes = nodeEnter.merge(svgNodes);

    svgNodes.select('circle')
      .attr('r', function (d) { return nodeRadius(d); })
      .attr('fill', function (d) { return nodeColor(d); });

    svgNodes.select('text')
      .attr('dy', function (d) { return nodeRadius(d) + 12; })
      .text(function (d) { return d.isGroup ? d.label : shortName(d.id); });

    // Node events
    svgNodes.on('mouseover', function (event, d) { updateTooltip(d); })
      .on('mouseout', function () { resetTooltip(); })
      .on('click', function (event, d) {
        event.stopPropagation();
        highlightNode(d.id);
      })
      .on('dblclick', function (event, d) {
        event.stopPropagation();
        if (state.pinnedNodes[d.id]) {
          delete state.pinnedNodes[d.id];
          d.fx = null; d.fy = null;
          state.simulation.alpha(0.3).restart();
        }
      });

    // Drag on SVG nodes
    svgNodes.call(d3.drag()
      .on('start', function (event, d) {
        state.simulation.alphaTarget(0.3).restart();
        d.fx = d.x; d.fy = d.y;
      })
      .on('drag', function (event, d) {
        d.fx = event.x; d.fy = event.y;
      })
      .on('end', function (event, d) {
        state.simulation.alphaTarget(0);
        state.pinnedNodes[d.id] = { x: d.fx, y: d.fy };
      })
    );
  }

  // ===========================================================================
  // Rendering — Canvas
  // ===========================================================================

  function renderCanvasFrame() {
    if (state.mode !== 'canvas' || !canvasCtx) return;
    var w = canvasEl.width / window.devicePixelRatio;
    var h = canvasEl.height / window.devicePixelRatio;
    var ctx = canvasCtx;

    ctx.save();
    ctx.setTransform(window.devicePixelRatio, 0, 0, window.devicePixelRatio, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(state.transform.x, state.transform.y);
    ctx.scale(state.transform.k, state.transform.k);

    // Draw edges
    state.simEdges.forEach(function (e) {
      if (!e.source.x) return;
      var active = getEdgeActive(e);
      ctx.beginPath();
      ctx.moveTo(e.source.x, e.source.y);
      ctx.lineTo(e.target.x, e.target.y);
      ctx.strokeStyle = e.inCycle ? CYCLE_COLOR : (EDGE_COLORS[e.kind] || '#999');
      ctx.lineWidth = e.inCycle ? 2.5 : 1;
      ctx.globalAlpha = active ? edgeOpacity(e) : 0.05;

      if (state.activeCycle >= 0 && e.inCycle && active) {
        ctx.setLineDash([10, 5]);
        ctx.lineDashOffset = -state.antOffset;
        ctx.lineWidth = 3;
        ctx.globalAlpha = 1;
      } else if (EDGE_DASH[e.kind]) {
        ctx.setLineDash(EDGE_DASH[e.kind].split(',').map(Number));
      } else {
        ctx.setLineDash([]);
      }
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.globalAlpha = 1;
    });

    // Draw nodes
    state.simNodes.forEach(function (n) {
      if (n.x === undefined) return;
      var r = nodeRadius(n);
      ctx.globalAlpha = getNodeOpacity(n);
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, 2 * Math.PI);
      ctx.fillStyle = nodeColor(n);
      ctx.fill();
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 1.5;
      ctx.stroke();
    });
    ctx.globalAlpha = 1;
    ctx.restore();
  }

  function findNodeAt(screenX, screenY) {
    var pt = state.transform.invert([screenX, screenY]);
    var x = pt[0], y = pt[1];
    for (var i = state.simNodes.length - 1; i >= 0; i--) {
      var n = state.simNodes[i];
      if (n.x === undefined) continue;
      if (Math.hypot(n.x - x, n.y - y) <= nodeRadius(n)) return n;
    }
    return null;
  }

  // ===========================================================================
  // Tick
  // ===========================================================================

  function tick() {
    if (state.mode === 'svg') {
      svgLinks
        .attr('x1', function (d) { return d.source.x; })
        .attr('y1', function (d) { return d.source.y; })
        .attr('x2', function (d) { return d.target.x; })
        .attr('y2', function (d) { return d.target.y; });
      svgNodes.attr('transform', function (d) { return 'translate(' + d.x + ',' + d.y + ')'; });
      // Dimming
      svgNodes.each(function (d) {
        d3.select(this).select('circle').attr('opacity', getNodeOpacity(d));
        d3.select(this).select('text').attr('opacity', getNodeOpacity(d));
      });
      svgLinks.each(function (d) {
        var active = getEdgeActive(d);
        d3.select(this).classed('dimmed', !active);
        d3.select(this).classed('marching-ants', state.activeCycle >= 0 && d.inCycle && active);
      });
    } else {
      if (state.activeCycle >= 0) state.antOffset = (state.antOffset + 0.5) % 15;
      renderCanvasFrame();
    }
  }

  // ===========================================================================
  // Sidebar — safe DOM construction (no innerHTML)
  // ===========================================================================

  function renderSidebar() {
    var sb = document.getElementById('sidebar');
    while (sb.firstChild) sb.removeChild(sb.firstChild);
    renderSummary(sb);
    renderFilters(sb);
    renderCommunities(sb);
    renderCycles(sb);
    renderForceControls(sb);
    renderSearch(sb);
  }

  function makeSection(parent, title, collapsed) {
    var sec = document.createElement('div');
    sec.className = 'section';
    var hdr = document.createElement('div');
    hdr.className = 'section-header' + (collapsed ? ' collapsed' : '');
    hdr.textContent = title;
    var content = document.createElement('div');
    content.className = 'section-content' + (collapsed ? ' hidden' : '');
    hdr.addEventListener('click', function () {
      hdr.classList.toggle('collapsed');
      content.classList.toggle('hidden');
    });
    sec.appendChild(hdr);
    sec.appendChild(content);
    parent.appendChild(sec);
    return content;
  }

  function renderSummary(sb) {
    var content = makeSection(sb, 'Summary', false);
    var grid = document.createElement('div');
    grid.className = 'summary-grid';
    var items = [
      { value: data.summary.total_nodes, label: 'Nodes' },
      { value: data.summary.total_edges, label: 'Edges' },
      { value: data.summary.total_communities, label: 'Communities' },
      { value: data.summary.total_cycles, label: 'Cycles' }
    ];
    items.forEach(function (item) {
      var el = document.createElement('div');
      el.className = 'summary-item';
      var valDiv = document.createElement('div');
      valDiv.className = 'summary-value';
      valDiv.textContent = item.value;
      var lblDiv = document.createElement('div');
      lblDiv.className = 'summary-label';
      lblDiv.textContent = item.label;
      el.appendChild(valDiv);
      el.appendChild(lblDiv);
      grid.appendChild(el);
    });
    content.appendChild(grid);
  }

  function renderFilters(sb) {
    var content = makeSection(sb, 'Filters', false);

    var langGroup = document.createElement('div');
    langGroup.className = 'filter-group';
    var langLabel = document.createElement('div');
    langLabel.className = 'filter-group-label';
    langLabel.textContent = 'Language';
    langGroup.appendChild(langLabel);
    Object.keys(state.languages).forEach(function (lang) {
      var item = document.createElement('label');
      item.className = 'filter-item';
      var cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = state.languages[lang];
      cb.addEventListener('change', function () {
        state.languages[lang] = cb.checked;
        rebuild();
      });
      item.appendChild(cb);
      item.appendChild(document.createTextNode(lang));
      langGroup.appendChild(item);
    });
    content.appendChild(langGroup);

    var edgeGroup = document.createElement('div');
    edgeGroup.className = 'filter-group';
    var edgeLabel = document.createElement('div');
    edgeLabel.className = 'filter-group-label';
    edgeLabel.textContent = 'Edge Type';
    edgeGroup.appendChild(edgeLabel);
    ['Imports', 'Defines', 'Calls'].forEach(function (kind) {
      var item = document.createElement('label');
      item.className = 'filter-item';
      var cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = state.edgeKinds[kind];
      cb.addEventListener('change', function () {
        state.edgeKinds[kind] = cb.checked;
        rebuild();
      });
      item.appendChild(cb);
      item.appendChild(document.createTextNode(kind));
      edgeGroup.appendChild(item);
    });
    content.appendChild(edgeGroup);
  }

  function renderCommunities(sb) {
    var content = makeSection(sb, 'Communities', data.communities.length > 10);
    data.communities.forEach(function (c) {
      var item = document.createElement('div');
      item.className = 'community-item';
      if (state.collapsedCommunities[c.id]) item.classList.add('collapsed-community');
      var dot = document.createElement('span');
      dot.className = 'community-dot';
      dot.style.background = COMMUNITY_COLORS[c.id % COMMUNITY_COLORS.length];
      item.appendChild(dot);
      item.appendChild(document.createTextNode('C' + c.id + ' (' + c.members.length + ')'));
      item.addEventListener('click', function () {
        if (state.collapsedCommunities[c.id]) delete state.collapsedCommunities[c.id];
        else state.collapsedCommunities[c.id] = true;
        rebuild();
      });
      content.appendChild(item);
    });
  }

  function renderCycles(sb) {
    if (data.cycles.length === 0) return;
    var content = makeSection(sb, 'Cycles (' + data.cycles.length + ')', data.cycles.length > 10);
    data.cycles.forEach(function (cycle, idx) {
      var item = document.createElement('div');
      item.className = 'cycle-item';
      if (state.activeCycle === idx) item.classList.add('active');
      var chain = cycle.map(shortName).join(' \u2192 ');
      item.textContent = (idx + 1) + '. ' + chain;
      item.title = cycle.join(' \u2192 ');
      item.addEventListener('click', function () {
        if (state.activeCycle === idx) { clearHighlight(); }
        else {
          state.activeCycle = idx;
          state.highlightedNode = null;
          zoomToCycle(cycle);
          renderSidebar();
        }
      });
      content.appendChild(item);
    });
  }

  function renderForceControls(sb) {
    var content = makeSection(sb, 'Force Controls', true);
    makeSlider(content, 'Charge', state.chargeStrength, -300, -10, 1, function (v) {
      state.chargeStrength = v;
      state.simulation.force('charge').strength(v);
      state.simulation.alpha(0.3).restart();
    });
    makeSlider(content, 'Link Distance', state.linkDistance, 20, 300, 1, function (v) {
      state.linkDistance = v;
      state.simulation.force('link').distance(v);
      state.simulation.alpha(0.3).restart();
    });
    makeSlider(content, 'Gravity', state.centerGravity, 0, 0.3, 0.01, function (v) {
      state.centerGravity = v;
      state.simulation.force('center').strength(v);
      state.simulation.alpha(0.3).restart();
    });
  }

  function makeSlider(parent, label, value, min, max, step, onChange) {
    var group = document.createElement('div');
    group.className = 'slider-group';
    var lbl = document.createElement('div');
    lbl.className = 'slider-label';
    var nameSpan = document.createElement('span');
    nameSpan.textContent = label;
    var valueSpan = document.createElement('span');
    valueSpan.textContent = String(value);
    lbl.appendChild(nameSpan);
    lbl.appendChild(valueSpan);
    var input = document.createElement('input');
    input.type = 'range';
    input.min = String(min);
    input.max = String(max);
    input.step = String(step);
    input.value = String(value);
    input.addEventListener('input', function () {
      var v = parseFloat(input.value);
      valueSpan.textContent = String(v);
      onChange(v);
    });
    group.appendChild(lbl);
    group.appendChild(input);
    parent.appendChild(group);
  }

  function renderSearch(sb) {
    var content = makeSection(sb, 'Search', false);
    var input = document.createElement('input');
    input.type = 'text';
    input.id = 'search-input';
    input.placeholder = 'Search modules...';
    input.value = state.searchQuery;
    var debounceTimer;
    input.addEventListener('input', function () {
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(function () {
        state.searchQuery = input.value;
        if (state.mode === 'svg') tick();
        if (state.searchQuery) {
          var matches = state.simNodes.filter(function (n) {
            return n.id.toLowerCase().indexOf(state.searchQuery.toLowerCase()) >= 0;
          });
          if (matches.length === 1 && matches[0].x !== undefined) zoomToNode(matches[0]);
        }
      }, 200);
    });
    content.appendChild(input);
  }

  // ===========================================================================
  // Highlight / Zoom helpers
  // ===========================================================================

  function highlightNode(id) {
    state.highlightedNode = id;
    state.activeCycle = -1;
    if (state.mode === 'svg') tick();
    renderSidebar();
  }

  function clearHighlight() {
    state.highlightedNode = null;
    state.activeCycle = -1;
    if (state.mode === 'svg') tick();
    renderSidebar();
  }

  function zoomToCycle(cycle) {
    var nodes = state.simNodes.filter(function (n) { return cycle.indexOf(n.id) >= 0; });
    if (nodes.length > 0) zoomToNodes(nodes);
  }

  function zoomToNode(node) {
    var vp = document.getElementById('viewport');
    var w = vp.clientWidth, h = vp.clientHeight;
    var scale = 1.5;
    var t = d3.zoomIdentity.translate(w / 2 - node.x * scale, h / 2 - node.y * scale).scale(scale);
    var target = state.mode === 'svg' ? svgEl : d3.select(canvasEl);
    target.transition().duration(500).call(zoomBehavior.transform, t);
  }

  function zoomToNodes(nodes) {
    if (nodes.length === 0) return;
    var vp = document.getElementById('viewport');
    var w = vp.clientWidth, h = vp.clientHeight;
    var x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    nodes.forEach(function (n) {
      if (n.x < x0) x0 = n.x; if (n.y < y0) y0 = n.y;
      if (n.x > x1) x1 = n.x; if (n.y > y1) y1 = n.y;
    });
    var pad = 60;
    var dx = (x1 - x0) + pad * 2, dy = (y1 - y0) + pad * 2;
    var cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    var scale = Math.min(w / dx, h / dy, 3);
    var t = d3.zoomIdentity.translate(w / 2 - cx * scale, h / 2 - cy * scale).scale(scale);
    var target = state.mode === 'svg' ? svgEl : d3.select(canvasEl);
    target.transition().duration(500).call(zoomBehavior.transform, t);
  }

  // ===========================================================================
  // Tooltip
  // ===========================================================================

  function updateTooltip(n) {
    var tip = document.getElementById('tooltip');
    if (n.isGroup) {
      tip.textContent = n.label + ' | Community ' + n.community_id;
      return;
    }
    tip.textContent = n.id +
      ' | ' + n.kind +
      ' | Score: ' + n.score.toFixed(4) +
      ' | BT: ' + n.betweenness.toFixed(4) +
      ' | PR: ' + n.pagerank.toFixed(4) +
      ' | In: ' + n.in_degree +
      ' | Out: ' + n.out_degree +
      ' | Community: ' + n.community_id +
      (n.in_cycle ? ' | IN CYCLE' : '');
  }

  function resetTooltip() {
    document.getElementById('tooltip').textContent = 'Hover over a node to see details';
  }

  // ===========================================================================
  // PNG Export
  // ===========================================================================

  function exportPNG() {
    var filename = data.project_name + '-graph.png';
    if (state.mode === 'canvas') {
      var link = document.createElement('a');
      link.download = filename;
      link.href = canvasEl.toDataURL('image/png');
      link.click();
    } else {
      var svgData = new XMLSerializer().serializeToString(svgEl.node());
      var img = new Image();
      var svgBlob = new Blob([svgData], { type: 'image/svg+xml;charset=utf-8' });
      var url = URL.createObjectURL(svgBlob);
      img.onload = function () {
        var c = document.createElement('canvas');
        var vp = document.getElementById('viewport');
        c.width = vp.clientWidth * 2;
        c.height = vp.clientHeight * 2;
        var cx = c.getContext('2d');
        cx.scale(2, 2);
        cx.fillStyle = '#f8f9fa';
        cx.fillRect(0, 0, vp.clientWidth, vp.clientHeight);
        cx.drawImage(img, 0, 0);
        var dl = document.createElement('a');
        dl.download = filename;
        dl.href = c.toDataURL('image/png');
        dl.click();
        URL.revokeObjectURL(url);
      };
      img.src = url;
    }
  }

  // ===========================================================================
  // Rebuild (on filter/collapse change)
  // ===========================================================================

  function rebuild() {
    clearHighlight();
    createSimulation();
    setupViewport();
    if (state.mode === 'svg') renderSVGGraph();
    renderSidebar();
  }

  // ===========================================================================
  // Init
  // ===========================================================================

  function init() {
    document.getElementById('project-name').textContent = data.project_name;
    document.getElementById('export-png').addEventListener('click', exportPNG);

    if (data.nodes.length === 0) {
      var vp = document.getElementById('viewport');
      var empty = document.createElement('div');
      empty.className = 'empty-state';
      empty.textContent = 'No nodes to visualize';
      vp.appendChild(empty);
      renderSidebar();
      return;
    }

    createSimulation();
    setupViewport();
    if (state.mode === 'svg') renderSVGGraph();
    renderSidebar();
  }

  document.addEventListener('DOMContentLoaded', init);
})();
```

- [ ] **Step 2: Verify all tests still pass**

```bash
cargo test -p graphify-report html -- --nocapture
```

Expected: 6 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-report/assets/graph.js
git commit -m "feat: implement interactive D3.js graph visualization"
```

---

### Task 6: Integration test

**Files:**
- Create: `tests/html_integration.rs`

- [ ] **Step 1: Write the integration test**

Create `tests/html_integration.rs`:

```rust
//! Integration test for the HTML report pipeline.

use graphify_core::{
    community::Community,
    graph::CodeGraph,
    metrics::NodeMetrics,
    types::{Edge, Language, Node},
};
use graphify_report::{write_html, Cycle};

fn build_test_graph() -> (CodeGraph, Vec<NodeMetrics>, Vec<Community>, Vec<Cycle>) {
    let mut g = CodeGraph::new();
    g.add_node(Node::module("app.main", "app/main.py", Language::Python, 1, true));
    g.add_node(Node::module("app.utils", "app/utils.py", Language::Python, 1, true));
    g.add_node(Node::module("app.db", "app/db.py", Language::Python, 1, true));
    g.add_node(Node::module("app.api", "app/api.py", Language::Python, 1, true));

    g.add_edge("app.main", "app.utils", Edge::imports(1));
    g.add_edge("app.main", "app.db", Edge::imports(2));
    g.add_edge("app.api", "app.main", Edge::imports(3));
    g.add_edge("app.utils", "app.db", Edge::calls(5));

    let metrics = vec![
        NodeMetrics { id: "app.main".into(), betweenness: 0.8, pagerank: 0.3, in_degree: 1, out_degree: 2, in_cycle: false, score: 0.6, community_id: 0 },
        NodeMetrics { id: "app.utils".into(), betweenness: 0.3, pagerank: 0.2, in_degree: 1, out_degree: 1, in_cycle: false, score: 0.3, community_id: 0 },
        NodeMetrics { id: "app.db".into(), betweenness: 0.1, pagerank: 0.25, in_degree: 2, out_degree: 0, in_cycle: false, score: 0.2, community_id: 1 },
        NodeMetrics { id: "app.api".into(), betweenness: 0.0, pagerank: 0.15, in_degree: 0, out_degree: 1, in_cycle: false, score: 0.1, community_id: 1 },
    ];

    let communities = vec![
        Community { id: 0, members: vec!["app.main".into(), "app.utils".into()] },
        Community { id: 1, members: vec!["app.db".into(), "app.api".into()] },
    ];

    let cycles: Vec<Cycle> = vec![];

    (g, metrics, communities, cycles)
}

#[test]
fn full_pipeline_html_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("architecture_graph.html");

    let (graph, metrics, communities, cycles) = build_test_graph();
    write_html("integration-test", &graph, &metrics, &communities, &cycles, &path);

    assert!(path.exists(), "HTML file should be created");

    let content = std::fs::read_to_string(&path).unwrap();

    // Structure checks
    assert!(content.contains("<!DOCTYPE html>"));
    assert!(content.contains("<title>Graphify: integration-test</title>"));
    assert!(content.contains("GRAPHIFY_DATA"));
    assert!(content.contains("forceSimulation"));
    assert!(content.contains("id=\"sidebar\""));
    assert!(content.contains("id=\"viewport\""));
    assert!(content.contains("id=\"export-png\""));

    // Data checks
    assert!(content.contains("app.main"));
    assert!(content.contains("app.utils"));
    assert!(content.contains("app.db"));
    assert!(content.contains("app.api"));

    // Verify data block is valid JSON
    let start = content.find("GRAPHIFY_DATA = ").unwrap() + "GRAPHIFY_DATA = ".len();
    let end = content[start..].find(";\n</script>").unwrap() + start;
    let json_str = &content[start..end];
    let value: serde_json::Value = serde_json::from_str(json_str).expect("data should be valid JSON");
    assert_eq!(value["project_name"], "integration-test");
    assert_eq!(value["summary"]["total_nodes"], 4);
    assert_eq!(value["summary"]["total_edges"], 4);
    assert_eq!(value["summary"]["total_communities"], 2);
    assert_eq!(value["communities"].as_array().unwrap().len(), 2);
}
```

- [ ] **Step 2: Run the integration test**

```bash
cargo test --test html_integration -- --nocapture
```

Expected: 1 test PASS.

- [ ] **Step 3: Run full test suite to confirm no regressions**

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/html_integration.rs
git commit -m "test: integration test for HTML visualization pipeline"
```

---

### Task 7: Build verification and manual QA

**Files:** none (verification only)

- [ ] **Step 1: Build release binary**

```bash
cargo build --release -p graphify-cli
```

Expected: builds successfully.

- [ ] **Step 2: Run Graphify on a real project to generate HTML**

If a `graphify.toml` exists in the repo, add `"html"` to the format list and run:

```bash
./target/release/graphify run --config graphify.toml
```

Or create a test config pointing at any Python/TS project on disk. Check that `architecture_graph.html` is produced in the output directory.

- [ ] **Step 3: Open HTML in browser — manual QA checklist**

Open the generated `architecture_graph.html` in a browser and verify:

- [ ] Page loads without console errors
- [ ] Force-directed graph renders with nodes and edges
- [ ] Nodes are colored by community (different colors for different communities)
- [ ] Node sizes vary by score (hotspots are larger)
- [ ] Sidebar summary shows correct counts
- [ ] Language filter checkboxes toggle node visibility
- [ ] Edge type checkboxes toggle edge visibility
- [ ] Clicking a community in sidebar collapses it to a group node
- [ ] Clicking again expands it back
- [ ] Scroll to zoom, drag to pan works
- [ ] Double-click resets zoom
- [ ] Hovering a node shows details in footer tooltip
- [ ] Clicking a node highlights it and its neighbors
- [ ] Clicking background clears highlight
- [ ] Dragging a node repositions it (pins on release)
- [ ] Double-clicking a pinned node unpins it
- [ ] If cycles exist: clicking a cycle in sidebar zooms to it with marching ants
- [ ] Search input filters nodes by name
- [ ] Force control sliders adjust the layout in real-time
- [ ] PNG export downloads a valid image
- [ ] Open file with Wi-Fi off — still works (self-contained)

- [ ] **Step 4: Final commit with any fixes**

If the manual QA revealed issues, fix them and commit:

```bash
git add -A
git commit -m "fix: address issues found during HTML visualization QA"
```
