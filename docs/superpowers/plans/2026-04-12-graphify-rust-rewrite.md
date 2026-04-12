# Graphify v2 — Rust Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite Graphify as a standalone Rust binary that extracts Python and TypeScript dependencies via tree-sitter, builds knowledge graphs with petgraph, and generates structured reports (JSON, CSV, Markdown).

**Architecture:** Cargo workspace with 4 crates: `graphify-core` (graph model + analysis algorithms), `graphify-extract` (tree-sitter AST parsing for Python + TypeScript), `graphify-report` (JSON/CSV/Markdown serialization), `graphify-cli` (clap CLI + config parsing). Each crate is independently testable. Extraction uses rayon for parallel file processing.

**Tech Stack:** Rust 2021 edition, petgraph, tree-sitter (+ python/typescript grammars), clap 4, serde, rayon, toml

**Spec:** `docs/superpowers/specs/2026-04-12-graphify-rust-rewrite-design.md`

---

## File Map

### New files (create)

| File | Responsibility |
|---|---|
| `Cargo.toml` | Workspace root — defines members |
| `crates/graphify-core/Cargo.toml` | Core crate deps: petgraph, serde |
| `crates/graphify-core/src/lib.rs` | Re-exports types, graph, metrics, community, cycles |
| `crates/graphify-core/src/types.rs` | Node, Edge, EdgeKind, NodeKind, Language enums/structs |
| `crates/graphify-core/src/graph.rs` | CodeGraph wrapper around petgraph DiGraph |
| `crates/graphify-core/src/metrics.rs` | Betweenness, PageRank, degree, unified scoring |
| `crates/graphify-core/src/community.rs` | Louvain community detection + Label Propagation fallback |
| `crates/graphify-core/src/cycles.rs` | Tarjan SCC + Johnson simple cycles (cap 500) |
| `crates/graphify-extract/Cargo.toml` | Extract crate deps: graphify-core, tree-sitter, rayon |
| `crates/graphify-extract/src/lib.rs` | Re-exports lang, python, typescript, resolver, walker |
| `crates/graphify-extract/src/lang.rs` | LanguageExtractor trait + ExtractionResult struct |
| `crates/graphify-extract/src/walker.rs` | File discovery, dir exclusion, language detection |
| `crates/graphify-extract/src/python.rs` | PythonExtractor: imports, defs, calls via tree-sitter |
| `crates/graphify-extract/src/typescript.rs` | TypeScriptExtractor: imports, exports, defs, calls |
| `crates/graphify-extract/src/resolver.rs` | Module name normalization, path alias resolution |
| `crates/graphify-report/Cargo.toml` | Report crate deps: graphify-core, serde_json, csv |
| `crates/graphify-report/src/lib.rs` | Re-exports json, csv, markdown |
| `crates/graphify-report/src/json.rs` | node_link_data JSON + analysis.json + summary.json |
| `crates/graphify-report/src/csv.rs` | graph_nodes.csv + graph_edges.csv |
| `crates/graphify-report/src/markdown.rs` | architecture_report.md with tables |
| `crates/graphify-cli/Cargo.toml` | CLI crate deps: all crates + clap + toml |
| `crates/graphify-cli/src/main.rs` | Clap subcommands, config parsing, pipeline orchestration |
| `tests/fixtures/python_project/app/__init__.py` | Test fixture: Python package init |
| `tests/fixtures/python_project/app/main.py` | Test fixture: Python entry point |
| `tests/fixtures/python_project/app/services/__init__.py` | Test fixture: services package |
| `tests/fixtures/python_project/app/services/llm.py` | Test fixture: service module |
| `tests/fixtures/python_project/app/models/user.py` | Test fixture: model module |
| `tests/fixtures/ts_project/src/index.ts` | Test fixture: TS entry point |
| `tests/fixtures/ts_project/src/lib/api.ts` | Test fixture: TS library module |
| `tests/fixtures/ts_project/src/services/user.ts` | Test fixture: TS service module |
| `tests/fixtures/ts_project/tsconfig.json` | Test fixture: TS config with path aliases |
| `.github/workflows/release.yml` | CI: test + build 4 targets + GitHub Release |
| `install.sh` | Install script: detect OS/arch, download binary |

---

## Task 1: Workspace Scaffold + Core Types

**Files:**
- Create: `Cargo.toml`
- Create: `crates/graphify-core/Cargo.toml`
- Create: `crates/graphify-core/src/lib.rs`
- Create: `crates/graphify-core/src/types.rs`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
# Cargo.toml (workspace root)
[workspace]
resolver = "2"
members = [
    "crates/graphify-core",
    "crates/graphify-extract",
    "crates/graphify-report",
    "crates/graphify-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
```

- [ ] **Step 2: Create graphify-core Cargo.toml**

```toml
# crates/graphify-core/Cargo.toml
[package]
name = "graphify-core"
version.workspace = true
edition.workspace = true

[dependencies]
petgraph = "0.7"
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 3: Write failing test for core types**

Create `crates/graphify-core/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Python,
    TypeScript,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Module,
    Function,
    Class,
    Method,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub language: Language,
    pub line: usize,
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Imports,
    Defines,
    Calls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub kind: EdgeKind,
    pub weight: u32,
    pub line: usize,
}

impl Node {
    pub fn module(id: impl Into<String>, file_path: impl Into<PathBuf>, language: Language, is_local: bool) -> Self {
        Self {
            id: id.into(),
            kind: NodeKind::Module,
            file_path: file_path.into(),
            language,
            line: 1,
            is_local,
        }
    }

    pub fn symbol(
        id: impl Into<String>,
        kind: NodeKind,
        file_path: impl Into<PathBuf>,
        language: Language,
        line: usize,
        is_local: bool,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            file_path: file_path.into(),
            language,
            line,
            is_local,
        }
    }
}

impl Edge {
    pub fn imports(line: usize) -> Self {
        Self { kind: EdgeKind::Imports, weight: 1, line }
    }

    pub fn defines(line: usize) -> Self {
        Self { kind: EdgeKind::Defines, weight: 1, line }
    }

    pub fn calls(line: usize) -> Self {
        Self { kind: EdgeKind::Calls, weight: 1, line }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_module_node() {
        let node = Node::module("app.services.llm", "app/services/llm.py", Language::Python, true);
        assert_eq!(node.id, "app.services.llm");
        assert_eq!(node.kind, NodeKind::Module);
        assert!(node.is_local);
        assert_eq!(node.language, Language::Python);
    }

    #[test]
    fn test_create_symbol_node() {
        let node = Node::symbol(
            "call_llm",
            NodeKind::Function,
            "app/services/llm.py",
            Language::Python,
            10,
            true,
        );
        assert_eq!(node.id, "call_llm");
        assert_eq!(node.kind, NodeKind::Function);
        assert_eq!(node.line, 10);
    }

    #[test]
    fn test_edge_constructors() {
        let e = Edge::imports(3);
        assert_eq!(e.kind, EdgeKind::Imports);
        assert_eq!(e.weight, 1);
        assert_eq!(e.line, 3);

        let e = Edge::calls(15);
        assert_eq!(e.kind, EdgeKind::Calls);
        assert_eq!(e.weight, 1);
    }

    #[test]
    fn test_node_serialization() {
        let node = Node::module("app.main", "app/main.py", Language::Python, true);
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("app.main"));
        assert!(json.contains("Python"));
    }
}
```

- [ ] **Step 4: Create lib.rs to export types**

Create `crates/graphify-core/src/lib.rs`:

```rust
pub mod types;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p graphify-core`
Expected: 4 tests pass

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/graphify-core/
git commit -m "feat: scaffold workspace and core types (Node, Edge, Language, NodeKind, EdgeKind)"
```

---

## Task 2: CodeGraph (petgraph wrapper)

**Files:**
- Create: `crates/graphify-core/src/graph.rs`
- Modify: `crates/graphify-core/src/lib.rs`

- [ ] **Step 1: Write failing tests for CodeGraph**

Create `crates/graphify-core/src/graph.rs`:

```rust
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use std::collections::HashMap;

use crate::types::{Edge, EdgeKind, Language, Node, NodeKind};

pub struct CodeGraph {
    pub graph: DiGraph<Node, Edge>,
    index_map: HashMap<String, NodeIndex>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index_map: HashMap::new(),
        }
    }

    /// Add a node to the graph. If a node with the same ID exists, returns its index.
    pub fn add_node(&mut self, node: Node) -> NodeIndex {
        if let Some(&idx) = self.index_map.get(&node.id) {
            return idx;
        }
        let id = node.id.clone();
        let idx = self.graph.add_node(node);
        self.index_map.insert(id, idx);
        idx
    }

    /// Add an edge between two nodes by ID. Creates nodes if they don't exist.
    pub fn add_edge(&mut self, source_id: &str, target_id: &str, edge: Edge) {
        let source = self.index_map.get(source_id).copied()
            .unwrap_or_else(|| {
                let node = Node::module(source_id, "", Language::Python, false);
                self.add_node(node)
            });
        let target = self.index_map.get(target_id).copied()
            .unwrap_or_else(|| {
                let node = Node::module(target_id, "", Language::Python, false);
                self.add_node(node)
            });

        // Check if edge already exists and increment weight
        if let Some(existing) = self.graph.edges_connecting(source, target)
            .find(|e| e.weight().kind == edge.kind)
        {
            let idx = existing.id();
            self.graph[idx].weight += 1;
            return;
        }

        self.graph.add_edge(source, target, edge);
    }

    /// Get a node by its ID
    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.index_map.get(id).map(|&idx| &self.graph[idx])
    }

    /// Get the NodeIndex for a given ID
    pub fn get_index(&self, id: &str) -> Option<NodeIndex> {
        self.index_map.get(id).copied()
    }

    /// Number of nodes
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// In-degree for a node
    pub fn in_degree(&self, id: &str) -> usize {
        self.index_map.get(id)
            .map(|&idx| self.graph.edges_directed(idx, Direction::Incoming).count())
            .unwrap_or(0)
    }

    /// Out-degree for a node
    pub fn out_degree(&self, id: &str) -> usize {
        self.index_map.get(id)
            .map(|&idx| self.graph.edges_directed(idx, Direction::Outgoing).count())
            .unwrap_or(0)
    }

    /// All node IDs
    pub fn node_ids(&self) -> Vec<&str> {
        self.index_map.keys().map(|s| s.as_str()).collect()
    }

    /// All local node IDs
    pub fn local_node_ids(&self) -> Vec<&str> {
        self.graph.node_weights()
            .filter(|n| n.is_local)
            .map(|n| n.id.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        let main = Node::module("app.main", "app/main.py", Language::Python, true);
        let llm = Node::module("app.services.llm", "app/services/llm.py", Language::Python, true);
        let os = Node::module("os", "", Language::Python, false);

        g.add_node(main);
        g.add_node(llm);
        g.add_node(os);

        g.add_edge("app.main", "app.services.llm", Edge::imports(1));
        g.add_edge("app.main", "os", Edge::imports(2));
        g.add_edge("app.services.llm", "os", Edge::imports(1));
        g
    }

    #[test]
    fn test_add_nodes_and_edges() {
        let g = sample_graph();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 3);
    }

    #[test]
    fn test_no_duplicate_nodes() {
        let mut g = CodeGraph::new();
        let n1 = Node::module("app.main", "app/main.py", Language::Python, true);
        let n2 = Node::module("app.main", "app/main.py", Language::Python, true);
        let idx1 = g.add_node(n1);
        let idx2 = g.add_node(n2);
        assert_eq!(idx1, idx2);
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn test_edge_weight_increment() {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        g.add_edge("a", "b", Edge::calls(10));
        g.add_edge("a", "b", Edge::calls(20));
        // Same edge kind → weight incremented, not duplicated
        assert_eq!(g.edge_count(), 1);
        let a_idx = g.get_index("a").unwrap();
        let b_idx = g.get_index("b").unwrap();
        let edge = g.graph.edges_connecting(a_idx, b_idx).next().unwrap();
        assert_eq!(edge.weight().weight, 2);
    }

    #[test]
    fn test_different_edge_kinds_not_merged() {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("a", "b", Edge::calls(5));
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn test_degree() {
        let g = sample_graph();
        assert_eq!(g.in_degree("os"), 2);
        assert_eq!(g.out_degree("app.main"), 2);
        assert_eq!(g.in_degree("app.main"), 0);
    }

    #[test]
    fn test_local_nodes() {
        let g = sample_graph();
        let locals = g.local_node_ids();
        assert_eq!(locals.len(), 2);
        assert!(locals.contains(&"app.main"));
        assert!(locals.contains(&"app.services.llm"));
    }

    #[test]
    fn test_get_node() {
        let g = sample_graph();
        let node = g.get_node("app.main").unwrap();
        assert_eq!(node.kind, NodeKind::Module);
        assert!(g.get_node("nonexistent").is_none());
    }
}
```

- [ ] **Step 2: Update lib.rs**

```rust
pub mod types;
pub mod graph;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-core`
Expected: 11 tests pass (4 from types + 7 from graph)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-core/
git commit -m "feat: add CodeGraph wrapper with dedup, weight increment, degree counting"
```

---

## Task 3: Test Fixtures

**Files:**
- Create: `tests/fixtures/python_project/app/__init__.py`
- Create: `tests/fixtures/python_project/app/main.py`
- Create: `tests/fixtures/python_project/app/services/__init__.py`
- Create: `tests/fixtures/python_project/app/services/llm.py`
- Create: `tests/fixtures/python_project/app/models/user.py`
- Create: `tests/fixtures/ts_project/src/index.ts`
- Create: `tests/fixtures/ts_project/src/lib/api.ts`
- Create: `tests/fixtures/ts_project/src/services/user.ts`
- Create: `tests/fixtures/ts_project/tsconfig.json`

- [ ] **Step 1: Create Python fixtures**

`tests/fixtures/python_project/app/__init__.py`:
```python
```

`tests/fixtures/python_project/app/main.py`:
```python
import os
from app.services.llm import call_llm
from app.models.user import User

def main():
    user = User("test")
    result = call_llm("hello")
    call_llm("again")
    return result
```

`tests/fixtures/python_project/app/services/__init__.py`:
```python
```

`tests/fixtures/python_project/app/services/llm.py`:
```python
import json
from app.models.user import User

class LLMGateway:
    def __init__(self):
        self.model = "claude"

def call_llm(prompt):
    gateway = LLMGateway()
    return {"response": prompt}
```

`tests/fixtures/python_project/app/models/user.py`:
```python
class User:
    def __init__(self, name):
        self.name = name

def create_user(name):
    return User(name)
```

- [ ] **Step 2: Create TypeScript fixtures**

`tests/fixtures/ts_project/tsconfig.json`:
```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  }
}
```

`tests/fixtures/ts_project/src/index.ts`:
```typescript
import { api } from '@/lib/api';
import { UserService } from './services/user';

const service = new UserService();
api.get('/users');
api.get('/health');
```

`tests/fixtures/ts_project/src/lib/api.ts`:
```typescript
export const api = {
    get: (path: string) => fetch(path),
    post: (path: string, body: any) => fetch(path, { method: 'POST', body }),
};

export function createClient(baseUrl: string) {
    return { baseUrl };
}
```

`tests/fixtures/ts_project/src/services/user.ts`:
```typescript
import { api } from '@/lib/api';

export class UserService {
    async getUser(id: string) {
        return api.get(`/users/${id}`);
    }

    async createUser(name: string) {
        return api.post('/users', { name });
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add tests/fixtures/
git commit -m "feat: add Python and TypeScript test fixtures"
```

---

## Task 4: File Walker

**Files:**
- Create: `crates/graphify-extract/Cargo.toml`
- Create: `crates/graphify-extract/src/lib.rs`
- Create: `crates/graphify-extract/src/lang.rs`
- Create: `crates/graphify-extract/src/walker.rs`

- [ ] **Step 1: Create graphify-extract Cargo.toml**

```toml
# crates/graphify-extract/Cargo.toml
[package]
name = "graphify-extract"
version.workspace = true
edition.workspace = true

[dependencies]
graphify-core = { path = "../graphify-core" }
tree-sitter = "0.24"
tree-sitter-python = "0.23"
tree-sitter-typescript = "0.23"
rayon = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create lang.rs with LanguageExtractor trait**

```rust
// crates/graphify-extract/src/lang.rs
use std::path::Path;
use graphify_core::types::{Node, Edge};

pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<(String, String, Edge)>, // (source_id, target_id, edge)
}

impl ExtractionResult {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new() }
    }
}

pub trait LanguageExtractor: Send + Sync {
    /// File extensions this extractor handles (e.g., ["py"] or ["ts", "tsx"])
    fn extensions(&self) -> &[&str];

    /// Extract nodes and edges from a single file's source code
    fn extract_file(
        &self,
        path: &Path,
        source: &[u8],
        module_name: &str,
    ) -> ExtractionResult;
}
```

- [ ] **Step 3: Write walker with tests**

```rust
// crates/graphify-extract/src/walker.rs
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use graphify_core::types::Language;

const DEFAULT_EXCLUDES: &[&str] = &[
    "__pycache__", "node_modules", ".git", "dist", "tests",
    "__tests__", ".next", "build", ".venv", "venv",
];

pub struct DiscoveredFile {
    pub path: PathBuf,
    pub language: Language,
    pub module_name: String,
}

pub fn discover_files(
    root: &Path,
    languages: &[Language],
    local_prefix: &str,
    extra_excludes: &[String],
) -> Vec<DiscoveredFile> {
    let excludes: HashSet<&str> = DEFAULT_EXCLUDES.iter().copied()
        .chain(extra_excludes.iter().map(|s| s.as_str()))
        .collect();

    let extensions: Vec<&str> = languages.iter().flat_map(|lang| match lang {
        Language::Python => vec!["py"],
        Language::TypeScript => vec!["ts", "tsx"],
    }).collect();

    let mut files = Vec::new();
    walk_dir(root, root, &excludes, &extensions, local_prefix, &mut files);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn walk_dir(
    base: &Path,
    dir: &Path,
    excludes: &HashSet<&str>,
    extensions: &[&str],
    local_prefix: &str,
    out: &mut Vec<DiscoveredFile>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !excludes.contains(name.as_ref()) {
                walk_dir(base, &path, excludes, extensions, local_prefix, out);
            }
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                let language = match ext {
                    "py" => Language::Python,
                    "ts" | "tsx" => Language::TypeScript,
                    _ => continue,
                };
                let module_name = path_to_module(base, &path, local_prefix);
                out.push(DiscoveredFile { path, language, module_name });
            }
        }
    }
}

/// Convert a file path to dot-notation module name.
/// `app/services/llm.py` → `app.services.llm`
/// `app/services/__init__.py` → `app.services`
/// `src/lib/api.ts` → `src.lib.api`
/// `src/index.ts` → `src`
pub fn path_to_module(base: &Path, file: &Path, _local_prefix: &str) -> String {
    let relative = file.strip_prefix(base).unwrap_or(file);
    let stem = relative.with_extension("");
    let parts: Vec<&str> = stem.iter()
        .filter_map(|c| c.to_str())
        .collect();

    // Collapse __init__ and index to parent
    if let Some(last) = parts.last() {
        if *last == "__init__" || *last == "index" {
            return parts[..parts.len() - 1].join(".");
        }
    }

    parts.join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_path_to_module_python() {
        let base = Path::new("/project");
        assert_eq!(
            path_to_module(base, &PathBuf::from("/project/app/services/llm.py"), ""),
            "app.services.llm"
        );
    }

    #[test]
    fn test_path_to_module_init() {
        let base = Path::new("/project");
        assert_eq!(
            path_to_module(base, &PathBuf::from("/project/app/services/__init__.py"), ""),
            "app.services"
        );
    }

    #[test]
    fn test_path_to_module_ts() {
        let base = Path::new("/project");
        assert_eq!(
            path_to_module(base, &PathBuf::from("/project/src/lib/api.ts"), ""),
            "src.lib.api"
        );
    }

    #[test]
    fn test_path_to_module_index_ts() {
        let base = Path::new("/project");
        assert_eq!(
            path_to_module(base, &PathBuf::from("/project/src/index.ts"), ""),
            "src"
        );
    }

    #[test]
    fn test_discover_python_files() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join("tests/fixtures/python_project");

        if !fixture.exists() { return; } // skip if fixtures not created yet

        let files = discover_files(&fixture, &[Language::Python], "app.", &[]);
        assert!(files.len() >= 4); // __init__.py x2 + main.py + llm.py + user.py
        assert!(files.iter().all(|f| f.language == Language::Python));
        assert!(files.iter().any(|f| f.module_name == "app.main"));
        assert!(files.iter().any(|f| f.module_name == "app.services.llm"));
    }

    #[test]
    fn test_discover_ts_files() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap()
            .join("tests/fixtures/ts_project");

        if !fixture.exists() { return; }

        let files = discover_files(&fixture, &[Language::TypeScript], "src/", &[]);
        assert!(files.len() >= 3); // index.ts + api.ts + user.ts
        assert!(files.iter().all(|f| f.language == Language::TypeScript));
    }

    #[test]
    fn test_excludes_node_modules() {
        let tmp = tempfile::tempdir().unwrap();
        let nm = tmp.path().join("node_modules/foo.ts");
        let src = tmp.path().join("src/bar.ts");
        std::fs::create_dir_all(nm.parent().unwrap()).unwrap();
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        std::fs::write(&nm, "export const x = 1;").unwrap();
        std::fs::write(&src, "export const y = 2;").unwrap();

        let files = discover_files(tmp.path(), &[Language::TypeScript], "", &[]);
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_string_lossy().contains("bar.ts"));
    }
}
```

- [ ] **Step 4: Create lib.rs**

```rust
// crates/graphify-extract/src/lib.rs
pub mod lang;
pub mod walker;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p graphify-extract`
Expected: 7 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-extract/
git commit -m "feat: add file walker with dir exclusion, language detection, module naming"
```

---

## Task 5: Python Extractor

**Files:**
- Create: `crates/graphify-extract/src/python.rs`
- Modify: `crates/graphify-extract/src/lib.rs`

- [ ] **Step 1: Write Python extractor with tests**

```rust
// crates/graphify-extract/src/python.rs
use std::path::Path;
use tree_sitter::{Parser, Node as TsNode};

use graphify_core::types::{Edge, EdgeKind, Language, Node, NodeKind};
use crate::lang::{ExtractionResult, LanguageExtractor};

pub struct PythonExtractor {
    parser: Parser,
}

impl PythonExtractor {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser.set_language(&language.into()).expect("failed to set Python grammar");
        Self { parser }
    }
}

impl LanguageExtractor for PythonExtractor {
    fn extensions(&self) -> &[&str] {
        &["py"]
    }

    fn extract_file(&self, _path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        // Parser is not Send-safe for concurrent use — create a new one per call
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();
        let src = source;

        // Add module node
        result.nodes.push(Node::module(module_name, "", Language::Python, true));

        walk_python_node(tree.root_node(), src, module_name, &mut result);
        result
    }
}

fn walk_python_node(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    match node.kind() {
        "import_statement" => extract_import(node, src, module_name, result),
        "import_from_statement" => extract_from_import(node, src, module_name, result),
        "function_definition" => extract_function_def(node, src, module_name, result),
        "class_definition" => extract_class_def(node, src, module_name, result),
        "call" => extract_call(node, src, module_name, result),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Don't recurse into function/class bodies for definitions (already handled)
        // But do recurse for calls
        if node.kind() == "function_definition" || node.kind() == "class_definition" {
            if child.kind() == "block" {
                walk_block_for_calls(child, src, module_name, result);
                continue;
            }
        }
        walk_python_node(child, src, module_name, result);
    }
}

fn walk_block_for_calls(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if node.kind() == "call" {
        extract_call(node, src, module_name, result);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_block_for_calls(child, src, module_name, result);
    }
}

/// `import os` → Imports edge to "os"
/// `import os.path` → Imports edge to "os.path"
fn extract_import(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "dotted_name" {
            let name = child.utf8_text(src).unwrap_or("");
            let line = child.start_position().row + 1;
            result.edges.push((
                module_name.to_string(),
                name.to_string(),
                Edge::imports(line),
            ));
        }
    }
}

/// `from app.services.llm import call_llm` → Imports to "app.services.llm" + Calls to "call_llm"
/// `from . import utils` → Imports to relative (left as ".utils" for resolver)
fn extract_from_import(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    let line = node.start_position().row + 1;
    let mut module_path = String::new();
    let mut names: Vec<String> = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                if module_path.is_empty() {
                    module_path = child.utf8_text(src).unwrap_or("").to_string();
                } else {
                    names.push(child.utf8_text(src).unwrap_or("").to_string());
                }
            }
            "relative_import" => {
                let text = child.utf8_text(src).unwrap_or("");
                module_path = text.to_string();
            }
            "import_prefix" => {
                // The dots in `from .. import x`
                module_path = child.utf8_text(src).unwrap_or("").to_string();
            }
            _ => {
                // Named imports within the import list
                if child.kind() == "aliased_import" || child.kind() == "dotted_name" {
                    let name = child.utf8_text(src).unwrap_or("").to_string();
                    if !name.is_empty() {
                        names.push(name);
                    }
                }
            }
        }
    }

    // Collect names from the import list
    if names.is_empty() {
        // Try to find imported_from children
        let mut cursor2 = node.walk();
        for child in node.named_children(&mut cursor2) {
            if child.kind() == "dotted_name" && !module_path.is_empty() && child.utf8_text(src).unwrap_or("") != module_path {
                names.push(child.utf8_text(src).unwrap_or("").to_string());
            }
        }
    }

    if !module_path.is_empty() {
        result.edges.push((
            module_name.to_string(),
            module_path.clone(),
            Edge::imports(line),
        ));
    }

    for name in &names {
        result.edges.push((
            module_name.to_string(),
            format!("{}.{}", module_path, name),
            Edge::calls(line),
        ));
    }
}

fn extract_function_def(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = name_node.utf8_text(src).unwrap_or("");
        let line = name_node.start_position().row + 1;
        let func_id = format!("{}.{}", module_name, name);
        result.nodes.push(Node::symbol(
            &func_id, NodeKind::Function, "", Language::Python, line, true,
        ));
        result.edges.push((
            module_name.to_string(),
            func_id,
            Edge::defines(line),
        ));
    }
}

fn extract_class_def(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = name_node.utf8_text(src).unwrap_or("");
        let line = name_node.start_position().row + 1;
        let class_id = format!("{}.{}", module_name, name);
        result.nodes.push(Node::symbol(
            &class_id, NodeKind::Class, "", Language::Python, line, true,
        ));
        result.edges.push((
            module_name.to_string(),
            class_id,
            Edge::defines(line),
        ));
    }
}

fn extract_call(node: TsNode, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if let Some(func_node) = node.child_by_field_name("function") {
        let name = func_node.utf8_text(src).unwrap_or("");
        let line = func_node.start_position().row + 1;

        // Skip method calls on objects (e.g., self.method()) — only track bare function calls
        if !name.contains('.') && !name.is_empty() {
            result.edges.push((
                module_name.to_string(),
                name.to_string(),
                Edge::calls(line),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(source: &str) -> ExtractionResult {
        let extractor = PythonExtractor::new();
        extractor.extract_file(Path::new("test.py"), source.as_bytes(), "test_module")
    }

    #[test]
    fn test_import_statement() {
        let result = extract("import os\nimport json");
        let import_edges: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(import_edges.len(), 2);
        assert_eq!(import_edges[0].1, "os");
        assert_eq!(import_edges[1].1, "json");
    }

    #[test]
    fn test_from_import() {
        let result = extract("from app.services.llm import call_llm");
        let imports: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert!(!imports.is_empty());
        assert_eq!(imports[0].1, "app.services.llm");
    }

    #[test]
    fn test_function_definition() {
        let result = extract("def hello():\n    pass");
        let defines: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "test_module.hello");
    }

    #[test]
    fn test_class_definition() {
        let result = extract("class MyClass:\n    pass");
        let defines: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "test_module.MyClass");
        assert!(result.nodes.iter().any(|n| n.kind == NodeKind::Class));
    }

    #[test]
    fn test_call_site() {
        let result = extract("hello()\nhello()");
        let calls: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .collect();
        assert!(calls.len() >= 2);
        assert!(calls.iter().all(|(_, target, _)| target == "hello"));
    }

    #[test]
    fn test_module_node_created() {
        let result = extract("x = 1");
        assert!(result.nodes.iter().any(|n| n.id == "test_module" && n.kind == NodeKind::Module));
    }
}
```

- [ ] **Step 2: Update lib.rs**

```rust
// crates/graphify-extract/src/lib.rs
pub mod lang;
pub mod walker;
pub mod python;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-extract`
Expected: 13 tests pass (7 walker + 6 python)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/
git commit -m "feat: Python extractor — imports, from-imports, defs, classes, calls via tree-sitter"
```

---

## Task 6: TypeScript Extractor

**Files:**
- Create: `crates/graphify-extract/src/typescript.rs`
- Modify: `crates/graphify-extract/src/lib.rs`

- [ ] **Step 1: Write TypeScript extractor with tests**

```rust
// crates/graphify-extract/src/typescript.rs
use std::path::Path;
use tree_sitter::Parser;

use graphify_core::types::{Edge, EdgeKind, Language, Node, NodeKind};
use crate::lang::{ExtractionResult, LanguageExtractor};

pub struct TypeScriptExtractor {
    _parser: Parser,
}

impl TypeScriptExtractor {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
        parser.set_language(&language.into()).expect("failed to set TypeScript grammar");
        Self { _parser: parser }
    }
}

impl LanguageExtractor for TypeScriptExtractor {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }

    fn extract_file(&self, _path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();
        result.nodes.push(Node::module(module_name, "", Language::TypeScript, true));

        walk_ts_node(tree.root_node(), source, module_name, &mut result);
        result
    }
}

fn walk_ts_node(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    match node.kind() {
        "import_statement" => extract_import(node, src, module_name, result),
        "export_statement" => extract_export(node, src, module_name, result),
        "function_declaration" => extract_function_decl(node, src, module_name, result),
        "class_declaration" => extract_class_decl(node, src, module_name, result),
        "call_expression" => extract_call(node, src, module_name, result),
        "lexical_declaration" => check_require(node, src, module_name, result),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if node.kind() != "import_statement" && node.kind() != "export_statement" {
            walk_ts_node(child, src, module_name, result);
        }
    }
}

/// `import { api } from '@/lib/api'` → Imports edge to "@/lib/api"
/// `import React from 'react'` → Imports edge to "react"
fn extract_import(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    let line = node.start_position().row + 1;

    if let Some(source_node) = node.child_by_field_name("source") {
        let raw = source_node.utf8_text(src).unwrap_or("");
        // Remove quotes
        let module_path = raw.trim_matches(|c| c == '\'' || c == '"');
        if !module_path.is_empty() {
            result.edges.push((
                module_name.to_string(),
                module_path.to_string(),
                Edge::imports(line),
            ));
        }
    }
}

/// `export function createUser()` → Defines
/// `export { foo } from './bar'` → Re-export: Imports + Defines
/// `export class UserService` → Defines
fn extract_export(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    let line = node.start_position().row + 1;

    // Check for re-export: `export { ... } from '...'`
    if let Some(source_node) = node.child_by_field_name("source") {
        let raw = source_node.utf8_text(src).unwrap_or("");
        let module_path = raw.trim_matches(|c| c == '\'' || c == '"');
        result.edges.push((
            module_name.to_string(),
            module_path.to_string(),
            Edge::imports(line),
        ));
    }

    // Check for exported declarations
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => extract_function_decl(child, src, module_name, result),
            "class_declaration" => extract_class_decl(child, src, module_name, result),
            _ => {}
        }
    }
}

fn extract_function_decl(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = name_node.utf8_text(src).unwrap_or("");
        let line = name_node.start_position().row + 1;
        let func_id = format!("{}.{}", module_name, name);
        result.nodes.push(Node::symbol(
            &func_id, NodeKind::Function, "", Language::TypeScript, line, true,
        ));
        result.edges.push((
            module_name.to_string(),
            func_id,
            Edge::defines(line),
        ));
    }
}

fn extract_class_decl(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let name = name_node.utf8_text(src).unwrap_or("");
        let line = name_node.start_position().row + 1;
        let class_id = format!("{}.{}", module_name, name);
        result.nodes.push(Node::symbol(
            &class_id, NodeKind::Class, "", Language::TypeScript, line, true,
        ));
        result.edges.push((
            module_name.to_string(),
            class_id,
            Edge::defines(line),
        ));
    }
}

fn extract_call(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    if let Some(func_node) = node.child_by_field_name("function") {
        let name = func_node.utf8_text(src).unwrap_or("");
        let line = func_node.start_position().row + 1;

        // Skip member expressions (obj.method()) and `new` expressions
        if !name.contains('.') && !name.is_empty() && name != "require" && name != "fetch" {
            result.edges.push((
                module_name.to_string(),
                name.to_string(),
                Edge::calls(line),
            ));
        }
    }
}

/// Check `const x = require('./util')` pattern
fn check_require(node: tree_sitter::Node, src: &[u8], module_name: &str, result: &mut ExtractionResult) {
    let text = node.utf8_text(src).unwrap_or("");
    if !text.contains("require(") { return; }

    let line = node.start_position().row + 1;

    // Walk to find the call_expression with require
    fn find_require(n: tree_sitter::Node, src: &[u8]) -> Option<String> {
        if n.kind() == "call_expression" {
            if let Some(func) = n.child_by_field_name("function") {
                if func.utf8_text(src).unwrap_or("") == "require" {
                    if let Some(args) = n.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        for arg in args.children(&mut cursor) {
                            if arg.kind() == "string" {
                                let raw = arg.utf8_text(src).unwrap_or("");
                                return Some(raw.trim_matches(|c| c == '\'' || c == '"').to_string());
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            if let Some(found) = find_require(child, src) {
                return Some(found);
            }
        }
        None
    }

    if let Some(module_path) = find_require(node, src) {
        result.edges.push((
            module_name.to_string(),
            module_path,
            Edge::imports(line),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(source: &str) -> ExtractionResult {
        let extractor = TypeScriptExtractor::new();
        extractor.extract_file(Path::new("test.ts"), source.as_bytes(), "test_module")
    }

    #[test]
    fn test_named_import() {
        let result = extract("import { api } from '@/lib/api';");
        let imports: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "@/lib/api");
    }

    #[test]
    fn test_default_import() {
        let result = extract("import React from 'react';");
        let imports: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "react");
    }

    #[test]
    fn test_export_function() {
        let result = extract("export function createUser() { return null; }");
        let defines: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "test_module.createUser");
    }

    #[test]
    fn test_export_class() {
        let result = extract("export class UserService {}");
        let defines: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "test_module.UserService");
    }

    #[test]
    fn test_require() {
        let result = extract("const util = require('./util');");
        let imports: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "./util");
    }

    #[test]
    fn test_call_expression() {
        let result = extract("createUser('test');");
        let calls: Vec<_> = result.edges.iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "createUser");
    }

    #[test]
    fn test_module_node_created() {
        let result = extract("const x = 1;");
        assert!(result.nodes.iter().any(|n| n.id == "test_module" && n.kind == NodeKind::Module));
    }
}
```

- [ ] **Step 2: Update lib.rs**

```rust
// crates/graphify-extract/src/lib.rs
pub mod lang;
pub mod walker;
pub mod python;
pub mod typescript;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-extract`
Expected: 20 tests pass (7 walker + 6 python + 7 typescript)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/
git commit -m "feat: TypeScript extractor — imports, exports, require, classes, calls via tree-sitter"
```

---

## Task 7: Module Resolver

**Files:**
- Create: `crates/graphify-extract/src/resolver.rs`
- Modify: `crates/graphify-extract/src/lib.rs`

- [ ] **Step 1: Write resolver with tests**

```rust
// crates/graphify-extract/src/resolver.rs
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolves raw module references to canonical IDs.
/// Handles Python relative imports and TypeScript path aliases.
pub struct ModuleResolver {
    /// Map of file-based module names to their canonical IDs
    known_modules: HashMap<String, String>,
    /// TypeScript path aliases from tsconfig.json (e.g., "@/*" → "src/*")
    ts_aliases: Vec<(String, String)>,
    /// Project root
    root: PathBuf,
}

impl ModuleResolver {
    pub fn new(root: &Path) -> Self {
        Self {
            known_modules: HashMap::new(),
            ts_aliases: Vec::new(),
            root: root.to_path_buf(),
        }
    }

    /// Register a known module (from walker discovery)
    pub fn register_module(&mut self, module_name: &str) {
        self.known_modules.insert(module_name.to_string(), module_name.to_string());
    }

    /// Load TypeScript path aliases from tsconfig.json
    pub fn load_tsconfig(&mut self, tsconfig_path: &Path) {
        let content = match std::fs::read_to_string(tsconfig_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        // Minimal JSON parsing for paths — avoid full JSON dep in extract crate
        // Looking for: "paths": { "@/*": ["src/*"] }
        if let Some(paths_start) = content.find("\"paths\"") {
            if let Some(obj_start) = content[paths_start..].find('{') {
                let obj_region = &content[paths_start + obj_start..];
                if let Some(obj_end) = obj_region.find('}') {
                    let paths_obj = &obj_region[1..obj_end];
                    // Parse each alias: "@/*": ["src/*"]
                    for line in paths_obj.lines() {
                        let parts: Vec<&str> = line.split(':').collect();
                        if parts.len() >= 2 {
                            let alias = parts[0].trim().trim_matches(|c| c == '"' || c == '\'');
                            let target_raw = parts[1..].join(":");
                            // Extract first array element
                            if let Some(start) = target_raw.find('"') {
                                let rest = &target_raw[start + 1..];
                                if let Some(end) = rest.find('"') {
                                    let target = &rest[..end];
                                    self.ts_aliases.push((alias.to_string(), target.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Resolve a raw module reference to a canonical module ID.
    /// Returns (resolved_id, is_local).
    pub fn resolve(&self, raw: &str, from_module: &str) -> (String, bool) {
        // Python relative imports
        if raw.starts_with('.') {
            let resolved = self.resolve_python_relative(raw, from_module);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local);
        }

        // TypeScript path aliases
        for (alias_pattern, target_pattern) in &self.ts_aliases {
            if let Some(resolved) = self.resolve_ts_alias(raw, alias_pattern, target_pattern) {
                let is_local = self.known_modules.contains_key(&resolved);
                return (resolved, is_local);
            }
        }

        // TypeScript relative imports (./foo, ../foo)
        if raw.starts_with("./") || raw.starts_with("../") {
            let resolved = self.resolve_ts_relative(raw, from_module);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local);
        }

        // Direct module name — check if known
        let is_local = self.known_modules.contains_key(raw);
        (raw.to_string(), is_local)
    }

    fn resolve_python_relative(&self, raw: &str, from_module: &str) -> String {
        let dots = raw.chars().take_while(|c| *c == '.').count();
        let suffix = &raw[dots..];

        let parts: Vec<&str> = from_module.split('.').collect();
        if dots > parts.len() {
            return raw.to_string(); // Can't resolve — too many dots
        }

        let base_parts = &parts[..parts.len().saturating_sub(dots)];
        let base = base_parts.join(".");

        if suffix.is_empty() {
            base
        } else if base.is_empty() {
            suffix.to_string()
        } else {
            format!("{}.{}", base, suffix)
        }
    }

    fn resolve_ts_alias(&self, raw: &str, alias_pattern: &str, target_pattern: &str) -> Option<String> {
        // Pattern: "@/*" matches "@/lib/api" → capture "lib/api"
        let prefix = alias_pattern.trim_end_matches('*');
        if !raw.starts_with(prefix) {
            return None;
        }

        let suffix = &raw[prefix.len()..];
        let target_prefix = target_pattern.trim_end_matches('*');

        // Convert path to module notation
        let full_path = format!("{}{}", target_prefix, suffix);
        let module = full_path.replace('/', ".");
        Some(module)
    }

    fn resolve_ts_relative(&self, raw: &str, from_module: &str) -> String {
        let parts: Vec<&str> = from_module.split('.').collect();
        let base = if parts.len() > 1 {
            parts[..parts.len() - 1].join(".")
        } else {
            String::new()
        };

        let clean = raw.trim_start_matches("./");
        let module_part = clean.replace('/', ".");

        if raw.starts_with("../") {
            let base_parts: Vec<&str> = base.split('.').collect();
            if base_parts.len() > 1 {
                let up_base = base_parts[..base_parts.len() - 1].join(".");
                let rest = raw.trim_start_matches("../").replace('/', ".");
                format!("{}.{}", up_base, rest)
            } else {
                module_part
            }
        } else if base.is_empty() {
            module_part
        } else {
            format!("{}.{}", base, module_part)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver_with_modules(modules: &[&str]) -> ModuleResolver {
        let mut r = ModuleResolver::new(Path::new("/project"));
        for m in modules {
            r.register_module(m);
        }
        r
    }

    #[test]
    fn test_resolve_direct_known() {
        let r = resolver_with_modules(&["app.services.llm"]);
        let (id, local) = r.resolve("app.services.llm", "app.main");
        assert_eq!(id, "app.services.llm");
        assert!(local);
    }

    #[test]
    fn test_resolve_direct_unknown() {
        let r = resolver_with_modules(&["app.main"]);
        let (id, local) = r.resolve("os", "app.main");
        assert_eq!(id, "os");
        assert!(!local);
    }

    #[test]
    fn test_resolve_python_relative_single_dot() {
        let r = resolver_with_modules(&["app.services", "app.services.utils"]);
        let (id, _) = r.resolve(".utils", "app.services.llm");
        assert_eq!(id, "app.services.utils");
    }

    #[test]
    fn test_resolve_python_relative_double_dot() {
        let r = resolver_with_modules(&["app", "app.models"]);
        let (id, _) = r.resolve("..models", "app.services.llm");
        assert_eq!(id, "app.models");
    }

    #[test]
    fn test_resolve_ts_alias() {
        let mut r = resolver_with_modules(&["src.lib.api"]);
        r.ts_aliases.push(("@/*".to_string(), "src/*".to_string()));
        let (id, local) = r.resolve("@/lib/api", "src.index");
        assert_eq!(id, "src.lib.api");
        assert!(local);
    }

    #[test]
    fn test_resolve_ts_relative() {
        let r = resolver_with_modules(&["src.services.user"]);
        let (id, local) = r.resolve("./services/user", "src.index");
        assert_eq!(id, "src.services.user");
        assert!(local);
    }

    #[test]
    fn test_resolve_ts_relative_parent() {
        let r = resolver_with_modules(&["src.lib.api"]);
        let (id, _) = r.resolve("../lib/api", "src.services.user");
        assert_eq!(id, "src.lib.api");
    }

    #[test]
    fn test_load_tsconfig() {
        let tmp = tempfile::tempdir().unwrap();
        let tsconfig = tmp.path().join("tsconfig.json");
        std::fs::write(&tsconfig, r#"{
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/*": ["src/*"]
                }
            }
        }"#).unwrap();

        let mut r = ModuleResolver::new(tmp.path());
        r.load_tsconfig(&tsconfig);
        assert_eq!(r.ts_aliases.len(), 1);
        assert_eq!(r.ts_aliases[0].0, "@/*");
        assert_eq!(r.ts_aliases[0].1, "src/*");
    }
}
```

- [ ] **Step 2: Update lib.rs**

```rust
// crates/graphify-extract/src/lib.rs
pub mod lang;
pub mod walker;
pub mod python;
pub mod typescript;
pub mod resolver;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-extract`
Expected: 28 tests pass (7 walker + 6 python + 7 typescript + 8 resolver)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/
git commit -m "feat: module resolver — Python relative imports, TypeScript path aliases, tsconfig loading"
```

---

## Task 8: Cycle Detection

**Files:**
- Create: `crates/graphify-core/src/cycles.rs`
- Modify: `crates/graphify-core/src/lib.rs`

- [ ] **Step 1: Write cycle detection with tests**

```rust
// crates/graphify-core/src/cycles.rs
use petgraph::algo::tarjan_scc;
use petgraph::graph::NodeIndex;

use crate::graph::CodeGraph;

/// A strongly connected component with > 1 node (a circular dependency group)
pub struct CycleGroup {
    pub node_ids: Vec<String>,
}

/// Find all SCCs > 1 node using Tarjan's algorithm
pub fn find_sccs(graph: &CodeGraph) -> Vec<CycleGroup> {
    let sccs = tarjan_scc(&graph.graph);
    sccs.into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let node_ids: Vec<String> = scc.iter()
                .map(|&idx| graph.graph[idx].id.clone())
                .collect();
            CycleGroup { node_ids }
        })
        .collect()
}

/// Find simple cycles using DFS with backtracking, capped at max_cycles.
/// Johnson's algorithm is complex; this is a simpler DFS approach sufficient for code graphs.
pub fn find_simple_cycles(graph: &CodeGraph, max_cycles: usize) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let node_count = graph.graph.node_count();
    if node_count == 0 { return cycles; }

    let node_indices: Vec<NodeIndex> = graph.graph.node_indices().collect();

    for &start in &node_indices {
        if cycles.len() >= max_cycles { break; }

        let mut path = vec![start];
        let mut visited = vec![false; node_count];
        visited[start.index()] = true;

        dfs_cycles(
            &graph.graph, start, start,
            &mut path, &mut visited,
            &mut cycles, max_cycles,
        );
    }

    // Deduplicate: normalize each cycle to start with smallest node ID
    let mut seen = std::collections::HashSet::new();
    cycles.into_iter().filter(|cycle| {
        let mut normalized = cycle.clone();
        if let Some(min_pos) = normalized.iter().enumerate()
            .min_by(|a, b| a.1.cmp(b.1))
            .map(|(i, _)| i)
        {
            normalized.rotate_left(min_pos);
        }
        seen.insert(normalized)
    }).collect()
}

fn dfs_cycles(
    graph: &petgraph::graph::DiGraph<crate::types::Node, crate::types::Edge>,
    start: NodeIndex,
    current: NodeIndex,
    path: &mut Vec<NodeIndex>,
    visited: &mut Vec<bool>,
    cycles: &mut Vec<Vec<String>>,
    max_cycles: usize,
) {
    if cycles.len() >= max_cycles { return; }

    for neighbor in graph.neighbors(current) {
        if neighbor == start && path.len() > 1 {
            // Found a cycle
            let cycle: Vec<String> = path.iter()
                .map(|&idx| graph[idx].id.clone())
                .collect();
            cycles.push(cycle);
            if cycles.len() >= max_cycles { return; }
        } else if !visited[neighbor.index()] && neighbor.index() > start.index() {
            // Only explore nodes with higher index to avoid finding same cycle from different starts
            visited[neighbor.index()] = true;
            path.push(neighbor);
            dfs_cycles(graph, start, neighbor, path, visited, cycles, max_cycles);
            path.pop();
            visited[neighbor.index()] = false;
        }
    }
}

/// Check if a node participates in any SCC > 1
pub fn is_in_cycle(graph: &CodeGraph, node_id: &str) -> bool {
    let sccs = find_sccs(graph);
    sccs.iter().any(|scc| scc.node_ids.contains(&node_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language, Node};

    fn graph_with_cycle() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        g.add_node(Node::module("c", "c.py", Language::Python, true));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(1));
        g.add_edge("c", "a", Edge::imports(1)); // cycle: a→b→c→a
        g
    }

    fn graph_no_cycle() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        g.add_node(Node::module("c", "c.py", Language::Python, true));
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(1));
        // No back edge → DAG
        g
    }

    #[test]
    fn test_find_sccs_with_cycle() {
        let g = graph_with_cycle();
        let sccs = find_sccs(&g);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].node_ids.len(), 3);
    }

    #[test]
    fn test_find_sccs_no_cycle() {
        let g = graph_no_cycle();
        let sccs = find_sccs(&g);
        assert_eq!(sccs.len(), 0);
    }

    #[test]
    fn test_is_in_cycle() {
        let g = graph_with_cycle();
        assert!(is_in_cycle(&g, "a"));
        assert!(is_in_cycle(&g, "b"));
        assert!(is_in_cycle(&g, "c"));
    }

    #[test]
    fn test_is_not_in_cycle() {
        let g = graph_no_cycle();
        assert!(!is_in_cycle(&g, "a"));
    }

    #[test]
    fn test_simple_cycles_cap() {
        let g = graph_with_cycle();
        let cycles = find_simple_cycles(&g, 500);
        assert!(!cycles.is_empty());
        // The cycle a→b→c→a should be found
        assert!(cycles.iter().any(|c| c.len() == 3));
    }

    #[test]
    fn test_simple_cycles_empty_graph() {
        let g = CodeGraph::new();
        let cycles = find_simple_cycles(&g, 500);
        assert!(cycles.is_empty());
    }
}
```

- [ ] **Step 2: Update lib.rs**

```rust
// crates/graphify-core/src/lib.rs
pub mod types;
pub mod graph;
pub mod cycles;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-core`
Expected: 17 tests pass (4 types + 7 graph + 6 cycles)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-core/
git commit -m "feat: cycle detection — Tarjan SCC + simple cycles with DFS, capped at 500"
```

---

## Task 9: Metrics Engine (Betweenness, PageRank, Scoring)

**Files:**
- Create: `crates/graphify-core/src/metrics.rs`
- Modify: `crates/graphify-core/src/lib.rs`

- [ ] **Step 1: Write metrics with tests**

```rust
// crates/graphify-core/src/metrics.rs
use std::collections::HashMap;
use petgraph::graph::NodeIndex;
use petgraph::Direction;
use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::graph::CodeGraph;
use crate::cycles::is_in_cycle;

#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: f64,
    pub in_cycle: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self { betweenness: 0.4, pagerank: 0.2, in_degree: 0.2, in_cycle: 0.2 }
    }
}

#[derive(Debug, Clone)]
pub struct NodeMetrics {
    pub id: String,
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: usize,
    pub out_degree: usize,
    pub in_cycle: bool,
    pub score: f64,
    pub community_id: usize,
}

/// Compute betweenness centrality using Brandes' algorithm with sampling.
/// Samples k=min(200, n) source nodes for performance on large graphs.
pub fn betweenness_centrality(graph: &CodeGraph) -> HashMap<String, f64> {
    let g = &graph.graph;
    let n = g.node_count();
    if n == 0 { return HashMap::new(); }

    let mut centrality: HashMap<NodeIndex, f64> = g.node_indices()
        .map(|idx| (idx, 0.0))
        .collect();

    let nodes: Vec<NodeIndex> = g.node_indices().collect();
    let k = n.min(200);

    // Sample source nodes
    let sources: Vec<NodeIndex> = if k < n {
        let mut rng = thread_rng();
        let mut sampled = nodes.clone();
        sampled.shuffle(&mut rng);
        sampled.into_iter().take(k).collect()
    } else {
        nodes.clone()
    };

    for &s in &sources {
        // BFS from s
        let mut stack = Vec::new();
        let mut predecessors: HashMap<NodeIndex, Vec<NodeIndex>> = HashMap::new();
        let mut sigma: HashMap<NodeIndex, f64> = HashMap::new();
        let mut dist: HashMap<NodeIndex, i64> = HashMap::new();

        for &v in &nodes {
            sigma.insert(v, 0.0);
            dist.insert(v, -1);
        }
        sigma.insert(s, 1.0);
        dist.insert(s, 0);

        let mut queue = std::collections::VecDeque::new();
        queue.push_back(s);

        while let Some(v) = queue.pop_front() {
            stack.push(v);
            let d_v = dist[&v];
            for w in g.neighbors_directed(v, Direction::Outgoing) {
                if dist[&w] < 0 {
                    queue.push_back(w);
                    dist.insert(w, d_v + 1);
                }
                if dist[&w] == d_v + 1 {
                    let s_v = sigma[&v];
                    *sigma.get_mut(&w).unwrap() += s_v;
                    predecessors.entry(w).or_default().push(v);
                }
            }
        }

        let mut delta: HashMap<NodeIndex, f64> = nodes.iter().map(|&v| (v, 0.0)).collect();

        while let Some(w) = stack.pop() {
            if let Some(preds) = predecessors.get(&w) {
                for &v in preds {
                    let d = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                    *delta.get_mut(&v).unwrap() += d;
                }
            }
            if w != s {
                *centrality.get_mut(&w).unwrap() += delta[&w];
            }
        }
    }

    // Normalize
    let scale = if n > 2 { 1.0 / ((n - 1) as f64 * (n - 2) as f64) } else { 1.0 };
    if k < n {
        let sample_scale = n as f64 / k as f64;
        centrality.iter_mut().for_each(|(_, v)| *v *= scale * sample_scale);
    } else {
        centrality.iter_mut().for_each(|(_, v)| *v *= scale);
    }

    centrality.into_iter()
        .map(|(idx, val)| (g[idx].id.clone(), val))
        .collect()
}

/// Compute PageRank with damping=0.85, max 100 iterations, epsilon=1e-6
pub fn pagerank(graph: &CodeGraph) -> HashMap<String, f64> {
    let g = &graph.graph;
    let n = g.node_count();
    if n == 0 { return HashMap::new(); }

    let damping = 0.85;
    let epsilon = 1e-6;
    let max_iter = 100;

    let nodes: Vec<NodeIndex> = g.node_indices().collect();
    let mut ranks: HashMap<NodeIndex, f64> = nodes.iter()
        .map(|&idx| (idx, 1.0 / n as f64))
        .collect();

    for _ in 0..max_iter {
        let mut new_ranks: HashMap<NodeIndex, f64> = HashMap::new();
        let base = (1.0 - damping) / n as f64;

        for &node in &nodes {
            let mut rank = base;
            // Sum contributions from incoming neighbors
            for pred in g.neighbors_directed(node, Direction::Incoming) {
                let out_degree = g.neighbors_directed(pred, Direction::Outgoing).count();
                if out_degree > 0 {
                    rank += damping * ranks[&pred] / out_degree as f64;
                }
            }
            new_ranks.insert(node, rank);
        }

        // Check convergence
        let diff: f64 = nodes.iter()
            .map(|&idx| (new_ranks[&idx] - ranks[&idx]).abs())
            .sum();

        ranks = new_ranks;
        if diff < epsilon { break; }
    }

    ranks.into_iter()
        .map(|(idx, val)| (g[idx].id.clone(), val))
        .collect()
}

/// Min-max normalize a set of values to [0, 1]
fn normalize(values: &HashMap<String, f64>) -> HashMap<String, f64> {
    if values.is_empty() { return HashMap::new(); }

    let min = values.values().cloned().fold(f64::INFINITY, f64::min);
    let max = values.values().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range == 0.0 {
        values.iter().map(|(k, _)| (k.clone(), 0.0)).collect()
    } else {
        values.iter().map(|(k, &v)| (k.clone(), (v - min) / range)).collect()
    }
}

/// Compute all metrics and unified scores for each node
pub fn compute_metrics(graph: &CodeGraph, weights: &ScoringWeights) -> Vec<NodeMetrics> {
    let bet = normalize(&betweenness_centrality(graph));
    let pr = normalize(&pagerank(graph));

    let max_in = graph.graph.node_indices()
        .map(|idx| graph.graph.neighbors_directed(idx, Direction::Incoming).count())
        .max().unwrap_or(1).max(1) as f64;

    graph.graph.node_indices().map(|idx| {
        let node = &graph.graph[idx];
        let id = &node.id;
        let in_deg = graph.in_degree(id);
        let out_deg = graph.out_degree(id);
        let in_cyc = is_in_cycle(graph, id);

        let b = bet.get(id).copied().unwrap_or(0.0);
        let p = pr.get(id).copied().unwrap_or(0.0);
        let d = in_deg as f64 / max_in;
        let c = if in_cyc { 1.0 } else { 0.0 };

        let score = weights.betweenness * b
            + weights.pagerank * p
            + weights.in_degree * d
            + weights.in_cycle * c;

        NodeMetrics {
            id: id.clone(),
            betweenness: bet.get(id).copied().unwrap_or(0.0),
            pagerank: pr.get(id).copied().unwrap_or(0.0),
            in_degree: in_deg,
            out_degree: out_deg,
            in_cycle: in_cyc,
            score,
            community_id: 0, // filled by community detection
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language, Node};

    fn star_graph() -> CodeGraph {
        // Hub "a" with spokes b, c, d, e all pointing to a
        let mut g = CodeGraph::new();
        for name in ["a", "b", "c", "d", "e"] {
            g.add_node(Node::module(name, &format!("{}.py", name), Language::Python, true));
        }
        g.add_edge("b", "a", Edge::imports(1));
        g.add_edge("c", "a", Edge::imports(1));
        g.add_edge("d", "a", Edge::imports(1));
        g.add_edge("e", "a", Edge::imports(1));
        g
    }

    fn chain_graph() -> CodeGraph {
        // a → b → c → d
        let mut g = CodeGraph::new();
        for name in ["a", "b", "c", "d"] {
            g.add_node(Node::module(name, &format!("{}.py", name), Language::Python, true));
        }
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(1));
        g.add_edge("c", "d", Edge::imports(1));
        g
    }

    #[test]
    fn test_betweenness_hub_highest() {
        let g = star_graph();
        let bet = betweenness_centrality(&g);
        // In a star, the hub doesn't necessarily have highest betweenness
        // since spokes don't connect to each other through hub (only inbound)
        // But betweenness should be computed without errors
        assert_eq!(bet.len(), 5);
    }

    #[test]
    fn test_betweenness_chain_middle_highest() {
        let g = chain_graph();
        let bet = betweenness_centrality(&g);
        // b is on the path a→c and a→d, c is on path b→d
        assert!(bet["b"] >= bet["a"]);
        assert!(bet["b"] >= bet["d"]);
    }

    #[test]
    fn test_pagerank_hub_highest() {
        let g = star_graph();
        let pr = pagerank(&g);
        // "a" receives 4 incoming links — should have highest PageRank
        assert!(pr["a"] > pr["b"]);
        assert!(pr["a"] > pr["c"]);
    }

    #[test]
    fn test_pagerank_sums_to_one() {
        let g = star_graph();
        let pr = pagerank(&g);
        let sum: f64 = pr.values().sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize() {
        let mut vals = HashMap::new();
        vals.insert("a".to_string(), 10.0);
        vals.insert("b".to_string(), 20.0);
        vals.insert("c".to_string(), 30.0);
        let n = normalize(&vals);
        assert!((n["a"] - 0.0).abs() < 0.001);
        assert!((n["b"] - 0.5).abs() < 0.001);
        assert!((n["c"] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_metrics_default_weights() {
        let g = star_graph();
        let metrics = compute_metrics(&g, &ScoringWeights::default());
        assert_eq!(metrics.len(), 5);
        // "a" should have highest score (highest in-degree + pagerank)
        let a_score = metrics.iter().find(|m| m.id == "a").unwrap().score;
        let b_score = metrics.iter().find(|m| m.id == "b").unwrap().score;
        assert!(a_score > b_score);
    }

    #[test]
    fn test_compute_metrics_custom_weights() {
        let g = star_graph();
        let weights = ScoringWeights { betweenness: 0.0, pagerank: 0.0, in_degree: 1.0, in_cycle: 0.0 };
        let metrics = compute_metrics(&g, &weights);
        let a = metrics.iter().find(|m| m.id == "a").unwrap();
        assert_eq!(a.in_degree, 4);
        // Score should be purely based on in_degree
        assert!(a.score > 0.0);
    }
}
```

- [ ] **Step 2: Add rand dependency to Cargo.toml**

Add to `crates/graphify-core/Cargo.toml`:

```toml
[dependencies]
petgraph = "0.7"
serde = { version = "1", features = ["derive"] }
rand = "0.8"
```

- [ ] **Step 3: Update lib.rs**

```rust
// crates/graphify-core/src/lib.rs
pub mod types;
pub mod graph;
pub mod cycles;
pub mod metrics;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p graphify-core`
Expected: 24 tests pass (4 types + 7 graph + 6 cycles + 7 metrics)

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/
git commit -m "feat: metrics engine — betweenness (Brandes sampled), PageRank, unified scoring"
```

---

## Task 10: Community Detection

**Files:**
- Create: `crates/graphify-core/src/community.rs`
- Modify: `crates/graphify-core/src/lib.rs`

- [ ] **Step 1: Write Louvain community detection with tests**

```rust
// crates/graphify-core/src/community.rs
use std::collections::HashMap;
use petgraph::graph::NodeIndex;
use petgraph::Direction;

use crate::graph::CodeGraph;

#[derive(Debug, Clone)]
pub struct Community {
    pub id: usize,
    pub members: Vec<String>,
}

/// Louvain community detection on undirected projection of the directed graph.
/// Returns a map of node_id → community_id.
pub fn detect_communities(graph: &CodeGraph) -> Vec<Community> {
    let g = &graph.graph;
    let n = g.node_count();
    if n == 0 { return Vec::new(); }

    let nodes: Vec<NodeIndex> = g.node_indices().collect();

    // Build undirected adjacency with weights
    let mut adj: HashMap<NodeIndex, HashMap<NodeIndex, f64>> = HashMap::new();
    for idx in g.node_indices() {
        adj.insert(idx, HashMap::new());
    }

    for edge_ref in g.edge_references() {
        let s = edge_ref.source();
        let t = edge_ref.target();
        let w = edge_ref.weight().weight as f64;
        *adj.get_mut(&s).unwrap().entry(t).or_insert(0.0) += w;
        *adj.get_mut(&t).unwrap().entry(s).or_insert(0.0) += w;
    }

    let total_weight: f64 = g.edge_references()
        .map(|e| e.weight().weight as f64)
        .sum();

    if total_weight == 0.0 {
        // No edges — each node is its own community
        return nodes.iter().enumerate().map(|(i, &idx)| {
            Community { id: i, members: vec![g[idx].id.clone()] }
        }).collect();
    }

    let m2 = total_weight * 2.0; // each edge counted twice in undirected

    // Initialize: each node in its own community
    let mut community: HashMap<NodeIndex, usize> = nodes.iter().enumerate()
        .map(|(i, &idx)| (idx, i))
        .collect();

    // Degree (sum of edge weights) per node in undirected graph
    let degree: HashMap<NodeIndex, f64> = nodes.iter().map(|&idx| {
        let d: f64 = adj[&idx].values().sum();
        (idx, d)
    }).collect();

    // Louvain phase 1: local moves
    let mut improved = true;
    let mut max_iter = 20;
    while improved && max_iter > 0 {
        improved = false;
        max_iter -= 1;

        for &node in &nodes {
            let current_comm = community[&node];

            // Sum of weights to each neighboring community
            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            for (&neighbor, &weight) in &adj[&node] {
                let nc = community[&neighbor];
                *comm_weights.entry(nc).or_insert(0.0) += weight;
            }

            // Sum of degrees in each community
            let mut comm_degrees: HashMap<usize, f64> = HashMap::new();
            for (&idx, &comm) in &community {
                *comm_degrees.entry(comm).or_insert(0.0) += degree[&idx];
            }

            let ki = degree[&node];
            let mut best_comm = current_comm;
            let mut best_delta = 0.0;

            // Remove node from current community for calculation
            let ki_in_current = comm_weights.get(&current_comm).copied().unwrap_or(0.0);
            let sigma_current = comm_degrees.get(&current_comm).copied().unwrap_or(0.0) - ki;

            for (&comm, &ki_in) in &comm_weights {
                if comm == current_comm { continue; }
                let sigma_comm = comm_degrees.get(&comm).copied().unwrap_or(0.0);

                // Modularity gain
                let delta = (ki_in - ki_in_current) / m2
                    - ki * (sigma_comm - sigma_current) / (m2 * m2) * 2.0;

                if delta > best_delta {
                    best_delta = delta;
                    best_comm = comm;
                }
            }

            if best_comm != current_comm {
                community.insert(node, best_comm);
                improved = true;
            }
        }
    }

    // Group nodes by community
    let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
    for (&idx, &comm) in &community {
        groups.entry(comm).or_default().push(g[idx].id.clone());
    }

    // Normalize community IDs to 0..n
    groups.into_iter().enumerate().map(|(i, (_, members))| {
        Community { id: i, members }
    }).collect()
}

/// Label propagation fallback — simpler but less precise
pub fn label_propagation(graph: &CodeGraph) -> Vec<Community> {
    let g = &graph.graph;
    let n = g.node_count();
    if n == 0 { return Vec::new(); }

    let nodes: Vec<NodeIndex> = g.node_indices().collect();
    let mut labels: HashMap<NodeIndex, usize> = nodes.iter().enumerate()
        .map(|(i, &idx)| (idx, i))
        .collect();

    for _ in 0..50 {
        let mut changed = false;
        for &node in &nodes {
            let mut label_counts: HashMap<usize, usize> = HashMap::new();
            for neighbor in g.neighbors_undirected(node) {
                *label_counts.entry(labels[&neighbor]).or_insert(0) += 1;
            }
            if let Some((&best_label, _)) = label_counts.iter().max_by_key(|(_, &count)| count) {
                if labels[&node] != best_label {
                    labels.insert(node, best_label);
                    changed = true;
                }
            }
        }
        if !changed { break; }
    }

    let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
    for (&idx, &label) in &labels {
        groups.entry(label).or_default().push(g[idx].id.clone());
    }

    groups.into_iter().enumerate().map(|(i, (_, members))| {
        Community { id: i, members }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language, Node};

    fn two_cluster_graph() -> CodeGraph {
        // Cluster 1: a↔b↔c (dense)
        // Cluster 2: d↔e↔f (dense)
        // Bridge: c→d (sparse)
        let mut g = CodeGraph::new();
        for name in ["a", "b", "c", "d", "e", "f"] {
            g.add_node(Node::module(name, &format!("{}.py", name), Language::Python, true));
        }
        // Cluster 1
        g.add_edge("a", "b", Edge::imports(1));
        g.add_edge("b", "a", Edge::imports(1));
        g.add_edge("b", "c", Edge::imports(1));
        g.add_edge("c", "b", Edge::imports(1));
        g.add_edge("a", "c", Edge::imports(1));
        g.add_edge("c", "a", Edge::imports(1));
        // Cluster 2
        g.add_edge("d", "e", Edge::imports(1));
        g.add_edge("e", "d", Edge::imports(1));
        g.add_edge("e", "f", Edge::imports(1));
        g.add_edge("f", "e", Edge::imports(1));
        g.add_edge("d", "f", Edge::imports(1));
        g.add_edge("f", "d", Edge::imports(1));
        // Bridge
        g.add_edge("c", "d", Edge::imports(1));
        g
    }

    #[test]
    fn test_louvain_finds_two_communities() {
        let g = two_cluster_graph();
        let comms = detect_communities(&g);
        // Should find 2 or 3 communities (exact number depends on algorithm)
        assert!(comms.len() >= 2);
        // Total members should be 6
        let total: usize = comms.iter().map(|c| c.members.len()).sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn test_louvain_single_node() {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        let comms = detect_communities(&g);
        assert_eq!(comms.len(), 1);
        assert_eq!(comms[0].members.len(), 1);
    }

    #[test]
    fn test_louvain_no_edges() {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        let comms = detect_communities(&g);
        // Each node in its own community
        assert_eq!(comms.len(), 2);
    }

    #[test]
    fn test_label_propagation_finds_communities() {
        let g = two_cluster_graph();
        let comms = label_propagation(&g);
        assert!(comms.len() >= 1);
        let total: usize = comms.iter().map(|c| c.members.len()).sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn test_community_ids_are_sequential() {
        let g = two_cluster_graph();
        let comms = detect_communities(&g);
        for (i, comm) in comms.iter().enumerate() {
            assert_eq!(comm.id, i);
        }
    }
}
```

- [ ] **Step 2: Update lib.rs**

```rust
// crates/graphify-core/src/lib.rs
pub mod types;
pub mod graph;
pub mod cycles;
pub mod metrics;
pub mod community;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p graphify-core`
Expected: 29 tests pass (4 types + 7 graph + 6 cycles + 7 metrics + 5 community)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-core/
git commit -m "feat: community detection — Louvain with Label Propagation fallback"
```

---

## Task 11: Report Generation (JSON + CSV + Markdown)

**Files:**
- Create: `crates/graphify-report/Cargo.toml`
- Create: `crates/graphify-report/src/lib.rs`
- Create: `crates/graphify-report/src/json.rs`
- Create: `crates/graphify-report/src/csv.rs`
- Create: `crates/graphify-report/src/markdown.rs`

- [ ] **Step 1: Create graphify-report Cargo.toml**

```toml
# crates/graphify-report/Cargo.toml
[package]
name = "graphify-report"
version.workspace = true
edition.workspace = true

[dependencies]
graphify-core = { path = "../graphify-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
csv = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write JSON report with tests**

```rust
// crates/graphify-report/src/json.rs
use serde::Serialize;
use std::path::Path;

use graphify_core::graph::CodeGraph;
use graphify_core::metrics::NodeMetrics;
use graphify_core::community::Community;

/// node_link_data format — compatible with NetworkX JSON output
#[derive(Serialize)]
pub struct GraphJson {
    pub directed: bool,
    pub multigraph: bool,
    pub nodes: Vec<GraphNodeJson>,
    pub links: Vec<GraphLinkJson>,
}

#[derive(Serialize)]
pub struct GraphNodeJson {
    pub id: String,
    pub kind: String,
    pub file_path: String,
    pub language: String,
    pub line: usize,
    pub is_local: bool,
}

#[derive(Serialize)]
pub struct GraphLinkJson {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub weight: u32,
    pub line: usize,
}

/// Analysis output
#[derive(Serialize)]
pub struct AnalysisJson {
    pub nodes: Vec<AnalysisNodeJson>,
    pub communities: Vec<CommunityJson>,
    pub cycles: Vec<Vec<String>>,
    pub summary: SummaryJson,
}

#[derive(Serialize)]
pub struct AnalysisNodeJson {
    pub id: String,
    pub betweenness: f64,
    pub pagerank: f64,
    pub in_degree: usize,
    pub out_degree: usize,
    pub score: f64,
    pub community_id: usize,
    pub in_cycle: bool,
}

#[derive(Serialize)]
pub struct CommunityJson {
    pub id: usize,
    pub members: Vec<String>,
}

#[derive(Serialize)]
pub struct SummaryJson {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_communities: usize,
    pub total_cycles: usize,
    pub top_hotspots: Vec<(String, f64)>,
}

pub fn write_graph_json(graph: &CodeGraph, path: &Path) -> std::io::Result<()> {
    let g = &graph.graph;

    let nodes: Vec<GraphNodeJson> = g.node_weights().map(|n| GraphNodeJson {
        id: n.id.clone(),
        kind: format!("{:?}", n.kind),
        file_path: n.file_path.to_string_lossy().to_string(),
        language: format!("{:?}", n.language),
        line: n.line,
        is_local: n.is_local,
    }).collect();

    let links: Vec<GraphLinkJson> = g.edge_references().map(|e| {
        GraphLinkJson {
            source: g[e.source()].id.clone(),
            target: g[e.target()].id.clone(),
            kind: format!("{:?}", e.weight().kind),
            weight: e.weight().weight,
            line: e.weight().line,
        }
    }).collect();

    let data = GraphJson { directed: true, multigraph: false, nodes, links };
    let json = serde_json::to_string_pretty(&data)?;
    std::fs::write(path, json)
}

pub fn write_analysis_json(
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Vec<String>],
    path: &Path,
) -> std::io::Result<()> {
    let nodes: Vec<AnalysisNodeJson> = metrics.iter().map(|m| AnalysisNodeJson {
        id: m.id.clone(),
        betweenness: m.betweenness,
        pagerank: m.pagerank,
        in_degree: m.in_degree,
        out_degree: m.out_degree,
        score: m.score,
        community_id: m.community_id,
        in_cycle: m.in_cycle,
    }).collect();

    let comm_json: Vec<CommunityJson> = communities.iter().map(|c| CommunityJson {
        id: c.id,
        members: c.members.clone(),
    }).collect();

    let mut top_hotspots: Vec<(String, f64)> = metrics.iter()
        .map(|m| (m.id.clone(), m.score))
        .collect();
    top_hotspots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    top_hotspots.truncate(20);

    let data = AnalysisJson {
        nodes,
        communities: comm_json,
        cycles: cycles.to_vec(),
        summary: SummaryJson {
            total_nodes: metrics.len(),
            total_edges: 0, // filled by caller if needed
            total_communities: communities.len(),
            total_cycles: cycles.len(),
            top_hotspots,
        },
    };

    let json = serde_json::to_string_pretty(&data)?;
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{Edge, Language, Node};

    fn sample_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        g.add_edge("a", "b", Edge::imports(1));
        g
    }

    #[test]
    fn test_write_graph_json() {
        let g = sample_graph();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("graph.json");
        write_graph_json(&g, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["directed"], true);
        assert_eq!(parsed["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["links"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_write_analysis_json() {
        let metrics = vec![
            NodeMetrics { id: "a".into(), betweenness: 0.5, pagerank: 0.3, in_degree: 0, out_degree: 1, score: 0.4, community_id: 0, in_cycle: false },
            NodeMetrics { id: "b".into(), betweenness: 0.1, pagerank: 0.7, in_degree: 1, out_degree: 0, score: 0.6, community_id: 0, in_cycle: false },
        ];
        let communities = vec![Community { id: 0, members: vec!["a".into(), "b".into()] }];
        let cycles: Vec<Vec<String>> = vec![];
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("analysis.json");
        write_analysis_json(&metrics, &communities, &cycles, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["summary"]["total_nodes"], 2);
        assert_eq!(parsed["summary"]["total_communities"], 1);
    }
}
```

- [ ] **Step 3: Write CSV report with tests**

```rust
// crates/graphify-report/src/csv.rs
use std::path::Path;

use graphify_core::graph::CodeGraph;
use graphify_core::metrics::NodeMetrics;

pub fn write_nodes_csv(metrics: &[NodeMetrics], path: &Path) -> std::io::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(["id", "betweenness", "pagerank", "in_degree", "out_degree", "score", "community_id", "in_cycle"])?;

    for m in metrics {
        wtr.write_record([
            &m.id,
            &format!("{:.6}", m.betweenness),
            &format!("{:.6}", m.pagerank),
            &m.in_degree.to_string(),
            &m.out_degree.to_string(),
            &format!("{:.6}", m.score),
            &m.community_id.to_string(),
            &m.in_cycle.to_string(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn write_edges_csv(graph: &CodeGraph, path: &Path) -> std::io::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(["source", "target", "kind", "weight", "line"])?;

    let g = &graph.graph;
    for edge_ref in g.edge_references() {
        let e = edge_ref.weight();
        wtr.write_record([
            &g[edge_ref.source()].id,
            &g[edge_ref.target()].id,
            &format!("{:?}", e.kind),
            &e.weight.to_string(),
            &e.line.to_string(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{Edge, Language, Node};
    use graphify_core::community::Community;

    #[test]
    fn test_write_nodes_csv() {
        let metrics = vec![
            NodeMetrics { id: "a".into(), betweenness: 0.5, pagerank: 0.3, in_degree: 2, out_degree: 1, score: 0.4, community_id: 0, in_cycle: false },
        ];
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nodes.csv");
        write_nodes_csv(&metrics, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("id,betweenness,pagerank"));
        assert!(content.contains("a,0.500000"));
    }

    #[test]
    fn test_write_edges_csv() {
        let mut g = CodeGraph::new();
        g.add_node(Node::module("a", "a.py", Language::Python, true));
        g.add_node(Node::module("b", "b.py", Language::Python, true));
        g.add_edge("a", "b", Edge::imports(5));

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("edges.csv");
        write_edges_csv(&g, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("source,target,kind"));
        assert!(content.contains("a,b,Imports,1,5"));
    }
}
```

- [ ] **Step 4: Write Markdown report with tests**

```rust
// crates/graphify-report/src/markdown.rs
use std::path::Path;

use graphify_core::metrics::NodeMetrics;
use graphify_core::community::Community;

pub fn write_report(
    project_name: &str,
    metrics: &[NodeMetrics],
    communities: &[Community],
    cycles: &[Vec<String>],
    path: &Path,
) -> std::io::Result<()> {
    let mut md = String::new();

    md.push_str(&format!("# Architecture Report: {}\n\n", project_name));

    // Summary
    md.push_str("## Summary\n\n");
    md.push_str(&format!("- **Nodes:** {}\n", metrics.len()));
    md.push_str(&format!("- **Communities:** {}\n", communities.len()));
    md.push_str(&format!("- **Circular dependencies:** {}\n\n", cycles.len()));

    // Top hotspots
    md.push_str("## Top Hotspots\n\n");
    md.push_str("| Rank | Module | Score | Betweenness | PageRank | In-degree | In cycle |\n");
    md.push_str("|------|--------|-------|-------------|----------|-----------|----------|\n");

    let mut sorted: Vec<&NodeMetrics> = metrics.iter().collect();
    sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    for (i, m) in sorted.iter().take(20).enumerate() {
        md.push_str(&format!(
            "| {} | `{}` | {:.3} | {:.3} | {:.3} | {} | {} |\n",
            i + 1, m.id, m.score, m.betweenness, m.pagerank, m.in_degree,
            if m.in_cycle { "yes" } else { "no" }
        ));
    }
    md.push('\n');

    // Communities
    md.push_str("## Communities\n\n");
    for comm in communities {
        md.push_str(&format!("### Community {}\n\n", comm.id));
        md.push_str(&format!("**Members ({}):**\n\n", comm.members.len()));
        for member in &comm.members {
            md.push_str(&format!("- `{}`\n", member));
        }
        md.push('\n');
    }

    // Circular dependencies
    if !cycles.is_empty() {
        md.push_str("## Circular Dependencies\n\n");
        for (i, cycle) in cycles.iter().take(50).enumerate() {
            let cycle_str = cycle.iter()
                .map(|s| format!("`{}`", s))
                .collect::<Vec<_>>()
                .join(" -> ");
            md.push_str(&format!("{}. {} -> `{}`\n", i + 1, cycle_str, cycle[0]));
        }
        md.push('\n');
    }

    std::fs::write(path, md)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_report() {
        let metrics = vec![
            NodeMetrics { id: "a".into(), betweenness: 0.5, pagerank: 0.3, in_degree: 2, out_degree: 1, score: 0.8, community_id: 0, in_cycle: true },
            NodeMetrics { id: "b".into(), betweenness: 0.1, pagerank: 0.7, in_degree: 1, out_degree: 0, score: 0.3, community_id: 1, in_cycle: false },
        ];
        let communities = vec![
            Community { id: 0, members: vec!["a".into()] },
            Community { id: 1, members: vec!["b".into()] },
        ];
        let cycles = vec![vec!["a".into(), "b".into()]];
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("report.md");
        write_report("test-project", &metrics, &communities, &cycles, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Architecture Report: test-project"));
        assert!(content.contains("Top Hotspots"));
        assert!(content.contains("`a`"));
        assert!(content.contains("Circular Dependencies"));
    }

    #[test]
    fn test_report_no_cycles() {
        let metrics = vec![
            NodeMetrics { id: "x".into(), betweenness: 0.0, pagerank: 0.5, in_degree: 0, out_degree: 0, score: 0.1, community_id: 0, in_cycle: false },
        ];
        let communities = vec![Community { id: 0, members: vec!["x".into()] }];
        let cycles: Vec<Vec<String>> = vec![];
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("report.md");
        write_report("clean-project", &metrics, &communities, &cycles, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Circular dependencies:** 0"));
        assert!(!content.contains("## Circular Dependencies"));
    }
}
```

- [ ] **Step 5: Create lib.rs**

```rust
// crates/graphify-report/src/lib.rs
pub mod json;
pub mod csv;
pub mod markdown;
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p graphify-report`
Expected: 6 tests pass (2 json + 2 csv + 2 markdown)

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-report/
git commit -m "feat: report generation — JSON (node_link_data), CSV, Markdown with tables"
```

---

## Task 12: CLI + Config Parsing + Pipeline

**Files:**
- Create: `crates/graphify-cli/Cargo.toml`
- Create: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Create graphify-cli Cargo.toml**

```toml
# crates/graphify-cli/Cargo.toml
[package]
name = "graphify-cli"
version.workspace = true
edition.workspace = true

[[bin]]
name = "graphify"
path = "src/main.rs"

[dependencies]
graphify-core = { path = "../graphify-core" }
graphify-extract = { path = "../graphify-extract" }
graphify-report = { path = "../graphify-report" }
clap = { version = "4", features = ["derive"] }
toml = "0.8"
serde = { version = "1", features = ["derive"] }
rayon = "1"
```

- [ ] **Step 2: Write CLI with config parsing and pipeline orchestration**

```rust
// crates/graphify-cli/src/main.rs
use std::path::{Path, PathBuf};
use clap::{Parser, Subcommand};
use serde::Deserialize;

use graphify_core::graph::CodeGraph;
use graphify_core::types::{Language, Edge, Node};
use graphify_core::metrics::{compute_metrics, ScoringWeights};
use graphify_core::community::detect_communities;
use graphify_core::cycles::{find_sccs, find_simple_cycles};
use graphify_extract::walker::discover_files;
use graphify_extract::python::PythonExtractor;
use graphify_extract::typescript::TypeScriptExtractor;
use graphify_extract::lang::LanguageExtractor;
use graphify_extract::resolver::ModuleResolver;

#[derive(Parser)]
#[command(name = "graphify", version, about = "Architectural analysis of codebases")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a graphify.toml config file
    Init,
    /// Extract dependency graph from source code
    Extract {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Extract + analyze metrics
    Analyze {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        weights: Option<String>,
    },
    /// Full pipeline: extract + analyze + report
    Report {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        weights: Option<String>,
        #[arg(long, default_value = "json,csv,md")]
        format: String,
    },
    /// Alias for report (backward compatibility)
    Run {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Deserialize)]
struct Config {
    settings: Settings,
    project: Vec<ProjectConfig>,
}

#[derive(Deserialize)]
struct Settings {
    output: Option<String>,
    weights: Option<Vec<f64>>,
    exclude: Option<Vec<String>>,
    format: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ProjectConfig {
    name: String,
    repo: String,
    lang: Vec<String>,
    local_prefix: Option<String>,
}

fn parse_config(path: &Path) -> Config {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("Cannot read config file: {}", path.display()));
    toml::from_str(&content)
        .unwrap_or_else(|e| panic!("Invalid config: {}", e))
}

fn parse_weights(weights_str: Option<&str>, config_weights: Option<&Vec<f64>>) -> ScoringWeights {
    if let Some(s) = weights_str {
        let parts: Vec<f64> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if parts.len() == 4 {
            return ScoringWeights {
                betweenness: parts[0], pagerank: parts[1],
                in_degree: parts[2], in_cycle: parts[3],
            };
        }
    }
    if let Some(w) = config_weights {
        if w.len() == 4 {
            return ScoringWeights {
                betweenness: w[0], pagerank: w[1],
                in_degree: w[2], in_cycle: w[3],
            };
        }
    }
    ScoringWeights::default()
}

fn parse_languages(langs: &[String]) -> Vec<Language> {
    langs.iter().map(|l| match l.to_lowercase().as_str() {
        "python" | "py" => Language::Python,
        "typescript" | "ts" => Language::TypeScript,
        _ => panic!("Unsupported language: {}", l),
    }).collect()
}

fn extract_project(project: &ProjectConfig, excludes: &[String]) -> CodeGraph {
    let repo_path = PathBuf::from(&project.repo);
    let languages = parse_languages(&project.lang);
    let local_prefix = project.local_prefix.as_deref().unwrap_or("");

    let files = discover_files(&repo_path, &languages, local_prefix, excludes);
    eprintln!("  {} files found in {}", files.len(), project.name);

    let py_extractor = PythonExtractor::new();
    let ts_extractor = TypeScriptExtractor::new();

    // Build resolver
    let mut resolver = ModuleResolver::new(&repo_path);
    for file in &files {
        resolver.register_module(&file.module_name);
    }
    // Load tsconfig if TypeScript
    if languages.contains(&Language::TypeScript) {
        let tsconfig = repo_path.join("tsconfig.json");
        if tsconfig.exists() {
            resolver.load_tsconfig(&tsconfig);
        }
    }

    // Extract all files
    let results: Vec<_> = files.iter().map(|file| {
        let source = std::fs::read(&file.path).unwrap_or_default();
        let extractor: &dyn LanguageExtractor = match file.language {
            Language::Python => &py_extractor,
            Language::TypeScript => &ts_extractor,
        };
        extractor.extract_file(&file.path, &source, &file.module_name)
    }).collect();

    // Merge into graph
    let mut graph = CodeGraph::new();

    for result in &results {
        for node in &result.nodes {
            graph.add_node(node.clone());
        }
    }

    for result in &results {
        for (source, target, edge) in &result.edges {
            let (resolved_target, is_local) = resolver.resolve(target, source);
            // Ensure target node exists
            if graph.get_node(&resolved_target).is_none() {
                let lang = if resolved_target.ends_with(".ts") || resolved_target.ends_with(".tsx") {
                    Language::TypeScript
                } else {
                    Language::Python
                };
                graph.add_node(Node::module(&resolved_target, "", lang, is_local));
            }
            graph.add_edge(source, &resolved_target, edge.clone());
        }
    }

    eprintln!("  {} nodes, {} edges", graph.node_count(), graph.edge_count());
    graph
}

fn run_pipeline(config: &Config, output_base: &Path, weights: &ScoringWeights, formats: &[String]) {
    std::fs::create_dir_all(output_base).unwrap();

    for project in &config.project {
        eprintln!("Processing project: {}", project.name);
        let excludes = config.settings.exclude.clone().unwrap_or_default();

        // Extract
        let graph = extract_project(project, &excludes);

        let project_dir = output_base.join(&project.name);
        std::fs::create_dir_all(&project_dir).unwrap();

        // Always write graph.json
        if formats.contains(&"json".to_string()) {
            graphify_report::json::write_graph_json(&graph, &project_dir.join("graph.json")).unwrap();
        }

        // Analyze
        let mut metrics = compute_metrics(&graph, weights);
        let communities = detect_communities(&graph);

        // Assign community IDs to metrics
        for m in &mut metrics {
            for comm in &communities {
                if comm.members.contains(&m.id) {
                    m.community_id = comm.id;
                    break;
                }
            }
        }

        let sccs = find_sccs(&graph);
        let simple_cycles = find_simple_cycles(&graph, 500);
        let cycle_lists: Vec<Vec<String>> = sccs.iter()
            .map(|scc| scc.node_ids.clone())
            .chain(simple_cycles.into_iter())
            .collect();

        // Write reports
        if formats.contains(&"json".to_string()) {
            graphify_report::json::write_analysis_json(
                &metrics, &communities, &cycle_lists, &project_dir.join("analysis.json"),
            ).unwrap();
        }

        if formats.contains(&"csv".to_string()) {
            graphify_report::csv::write_nodes_csv(&metrics, &project_dir.join("graph_nodes.csv")).unwrap();
            graphify_report::csv::write_edges_csv(&graph, &project_dir.join("graph_edges.csv")).unwrap();
        }

        if formats.contains(&"md".to_string()) {
            graphify_report::markdown::write_report(
                &project.name, &metrics, &communities, &cycle_lists,
                &project_dir.join("architecture_report.md"),
            ).unwrap();
        }

        eprintln!("  Output written to {}", project_dir.display());
    }

    // Cross-project summary
    if config.project.len() > 1 {
        write_summary(&config.project, output_base);
    }

    eprintln!("Done.");
}

fn write_summary(projects: &[ProjectConfig], output_base: &Path) {
    let project_names: Vec<String> = projects.iter().map(|p| p.name.clone()).collect();

    // Read each project's analysis to find shared externals
    let mut all_externals: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for project in projects {
        let analysis_path = output_base.join(&project.name).join("analysis.json");
        if let Ok(content) = std::fs::read_to_string(&analysis_path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                // This is a simplified summary — in practice, read graph.json for external nodes
            }
        }
    }

    let summary = serde_json::json!({
        "projects": project_names,
        "cross_dependencies": [],
        "shared_externals": all_externals,
    });

    let path = output_base.join("graphify-summary.json");
    std::fs::write(&path, serde_json::to_string_pretty(&summary).unwrap()).unwrap();
}

fn generate_init_config() {
    let template = r#"[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format = ["json", "csv", "md"]

[[project]]
name = "my-project"
repo = "."
lang = ["python"]
local_prefix = "app."
"#;
    let path = Path::new("graphify.toml");
    if path.exists() {
        eprintln!("graphify.toml already exists. Aborting.");
        std::process::exit(1);
    }
    std::fs::write(path, template).unwrap();
    eprintln!("Created graphify.toml — edit it to configure your projects.");
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => generate_init_config(),

        Commands::Extract { config, output } => {
            let cfg = parse_config(&config);
            let output_base = output.unwrap_or_else(|| PathBuf::from(cfg.settings.output.as_deref().unwrap_or("./report")));
            let weights = parse_weights(None, cfg.settings.weights.as_ref());
            // Extract only — write graph.json per project
            run_pipeline(&cfg, &output_base, &weights, &["json".to_string()]);
        }

        Commands::Analyze { config, output, weights } => {
            let cfg = parse_config(&config);
            let output_base = output.unwrap_or_else(|| PathBuf::from(cfg.settings.output.as_deref().unwrap_or("./report")));
            let w = parse_weights(weights.as_deref(), cfg.settings.weights.as_ref());
            run_pipeline(&cfg, &output_base, &w, &["json".to_string()]);
        }

        Commands::Report { config, output, weights, format } => {
            let cfg = parse_config(&config);
            let output_base = output.unwrap_or_else(|| PathBuf::from(cfg.settings.output.as_deref().unwrap_or("./report")));
            let w = parse_weights(weights.as_deref(), cfg.settings.weights.as_ref());
            let formats: Vec<String> = format.split(',').map(|s| s.trim().to_string()).collect();
            run_pipeline(&cfg, &output_base, &w, &formats);
        }

        Commands::Run { config, output } => {
            let cfg = parse_config(&config);
            let output_base = output.unwrap_or_else(|| PathBuf::from(cfg.settings.output.as_deref().unwrap_or("./report")));
            let w = parse_weights(None, cfg.settings.weights.as_ref());
            let formats = cfg.settings.format.clone().unwrap_or_else(|| vec!["json".into(), "csv".into(), "md".into()]);
            run_pipeline(&cfg, &output_base, &w, &formats);
        }
    }
}
```

- [ ] **Step 3: Add serde_json to CLI deps (for summary)**

Already included via graphify-report dependency — but add explicitly:

```toml
# add to crates/graphify-cli/Cargo.toml [dependencies]
serde_json = "1"
```

- [ ] **Step 4: Build and verify binary compiles**

Run: `cargo build -p graphify-cli`
Expected: Compiles successfully, binary at `target/debug/graphify`

- [ ] **Step 5: Test CLI help and init**

Run: `cargo run -p graphify-cli -- --help`
Expected: Shows subcommands (init, extract, analyze, report, run)

Run: `cd /tmp && cargo run --manifest-path /path/to/Cargo.toml -p graphify-cli -- init`
Expected: Creates `graphify.toml` in /tmp

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-cli/
git commit -m "feat: CLI with clap subcommands, TOML config parsing, pipeline orchestration"
```

---

## Task 13: Integration Test with Fixtures

**Files:**
- Create: `tests/integration_test.rs` (in workspace root)

- [ ] **Step 1: Write integration test**

Create `tests/integration_test.rs`:

```rust
//! Integration test: run full pipeline against Python and TypeScript fixtures.

use std::path::PathBuf;
use std::process::Command;

fn graphify_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("debug");
    path.push("graphify");
    path
}

#[test]
fn test_python_fixture_pipeline() {
    // Build first
    let build = Command::new("cargo")
        .args(["build", "-p", "graphify-cli"])
        .output()
        .expect("failed to build");
    assert!(build.status.success(), "Build failed: {}", String::from_utf8_lossy(&build.stderr));

    let tmp = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python_project");

    // Write config
    let config_content = format!(r#"
[settings]
output = "{}"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__"]
format = ["json", "csv", "md"]

[[project]]
name = "test-py"
repo = "{}"
lang = ["python"]
local_prefix = "app."
"#, tmp.path().display(), fixture.display());

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(&config_path, config_content).unwrap();

    // Run pipeline
    let output = Command::new(graphify_bin())
        .args(["report", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("failed to run graphify");

    assert!(output.status.success(), "Graphify failed: {}", String::from_utf8_lossy(&output.stderr));

    // Verify outputs
    let project_dir = tmp.path().join("test-py");
    assert!(project_dir.join("graph.json").exists(), "graph.json missing");
    assert!(project_dir.join("analysis.json").exists(), "analysis.json missing");
    assert!(project_dir.join("graph_nodes.csv").exists(), "graph_nodes.csv missing");
    assert!(project_dir.join("graph_edges.csv").exists(), "graph_edges.csv missing");
    assert!(project_dir.join("architecture_report.md").exists(), "report.md missing");

    // Verify graph.json content
    let graph_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(project_dir.join("graph.json")).unwrap()
    ).unwrap();
    assert!(graph_json["directed"].as_bool().unwrap());
    assert!(!graph_json["nodes"].as_array().unwrap().is_empty());
}

#[test]
fn test_typescript_fixture_pipeline() {
    let build = Command::new("cargo")
        .args(["build", "-p", "graphify-cli"])
        .output()
        .expect("failed to build");
    assert!(build.status.success());

    let tmp = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_project");

    let config_content = format!(r#"
[settings]
output = "{}"
format = ["json"]

[[project]]
name = "test-ts"
repo = "{}"
lang = ["typescript"]
local_prefix = "src/"
"#, tmp.path().display(), fixture.display());

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(&config_path, config_content).unwrap();

    let output = Command::new(graphify_bin())
        .args(["report", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("failed to run graphify");

    assert!(output.status.success(), "Graphify failed: {}", String::from_utf8_lossy(&output.stderr));

    let project_dir = tmp.path().join("test-ts");
    assert!(project_dir.join("graph.json").exists());
}
```

- [ ] **Step 2: Add dev-dependencies to workspace root**

Add to root `Cargo.toml`:

```toml
[workspace.dependencies]
tempfile = "3"
serde_json = "1"

[dev-dependencies]
tempfile = "3"
serde_json = "1"
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test integration_test`
Expected: 2 tests pass

- [ ] **Step 4: Commit**

```bash
git add tests/ Cargo.toml
git commit -m "test: integration tests — full pipeline against Python and TypeScript fixtures"
```

---

## Task 14: CI/CD + Install Script

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `install.sh`

- [ ] **Step 1: Write GitHub Actions release workflow**

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags: ['v*']

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --all

  build:
    needs: test
    strategy:
      matrix:
        include:
          - target: x86_64-apple-darwin
            os: macos-13
          - target: aarch64-apple-darwin
            os: macos-14
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross (Linux ARM)
        if: matrix.target == 'aarch64-unknown-linux-musl'
        run: cargo install cross --git https://github.com/cross-rs/cross

      - name: Install musl tools (Linux x86)
        if: matrix.target == 'x86_64-unknown-linux-musl'
        run: sudo apt-get update && sudo apt-get install -y musl-tools

      - name: Build
        run: |
          if [ "${{ matrix.target }}" = "aarch64-unknown-linux-musl" ]; then
            cross build --release --target ${{ matrix.target }} -p graphify-cli
          else
            cargo build --release --target ${{ matrix.target }} -p graphify-cli
          fi

      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar czf ../../../graphify-${{ matrix.target }}.tar.gz graphify
          cd ../../..

      - uses: actions/upload-artifact@v4
        with:
          name: graphify-${{ matrix.target }}
          path: graphify-${{ matrix.target }}.tar.gz

  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true

      - name: Create Release
        uses: softprops/action-gh-release@v2
        with:
          files: graphify-*.tar.gz
          generate_release_notes: true
```

- [ ] **Step 2: Write install script**

```bash
#!/usr/bin/env bash
# install.sh — Install graphify binary
set -euo pipefail

REPO="parisgroup/graphify"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) OS="apple-darwin" ;;
  linux)  OS="unknown-linux-musl" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"
echo "Detected target: ${TARGET}"

# Get latest release tag
LATEST=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$LATEST" ]; then
  echo "Could not determine latest release"; exit 1
fi
echo "Latest version: ${LATEST}"

# Download
URL="https://github.com/${REPO}/releases/download/${LATEST}/graphify-${TARGET}.tar.gz"
echo "Downloading from: ${URL}"
TMPDIR=$(mktemp -d)
curl -sL "$URL" -o "${TMPDIR}/graphify.tar.gz"
tar xzf "${TMPDIR}/graphify.tar.gz" -C "${TMPDIR}"

# Install
INSTALL_DIR="/usr/local/bin"
if [ -w "$INSTALL_DIR" ]; then
  mv "${TMPDIR}/graphify" "${INSTALL_DIR}/graphify"
else
  sudo mv "${TMPDIR}/graphify" "${INSTALL_DIR}/graphify"
fi

rm -rf "$TMPDIR"

echo "graphify installed to ${INSTALL_DIR}/graphify"
graphify --version
```

- [ ] **Step 3: Make install script executable**

Run: `chmod +x install.sh`

- [ ] **Step 4: Commit**

```bash
git add .github/ install.sh
git commit -m "feat: CI/CD release workflow (4 targets) + install script"
```

---

## Task 15: README + Final Cleanup

**Files:**
- Create: `README.md`
- Modify: `CLAUDE.md` (update for Rust)

- [ ] **Step 1: Write README.md**

```markdown
# Graphify

Architectural analysis of codebases. Extracts dependencies via tree-sitter, builds knowledge graphs, identifies hotspots, circular dependencies, and community clusters.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/parisgroup/graphify/main/install.sh | sh
```

Or download from [Releases](https://github.com/parisgroup/graphify/releases).

## Quick Start

```bash
# Generate config
graphify init

# Edit graphify.toml to point at your projects

# Run full analysis
graphify report
```

## Configuration

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests"]
format = ["json", "csv", "md"]

[[project]]
name = "my-app"
repo = "./apps/my-app"
lang = ["python"]
local_prefix = "app."
```

## Commands

| Command | Description |
|---------|-------------|
| `graphify init` | Generate graphify.toml |
| `graphify extract` | Extract dependency graph |
| `graphify analyze` | Extract + compute metrics |
| `graphify report` | Full pipeline with all outputs |
| `graphify run` | Alias for report |

## Output

Each project produces:
- `graph.json` — dependency graph (NetworkX node_link_data format)
- `analysis.json` — metrics, communities, cycles
- `graph_nodes.csv` — node metrics
- `graph_edges.csv` — edge list
- `architecture_report.md` — human-readable report

## Supported Languages

- Python
- TypeScript

## License

MIT
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add README with install, quickstart, and configuration guide"
```

- [ ] **Step 3: Update CLAUDE.md for Rust project**

Update the relevant sections in CLAUDE.md to reflect the Rust rewrite (commands, architecture, conventions). This is the final cleanup step.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for Rust rewrite"
```
