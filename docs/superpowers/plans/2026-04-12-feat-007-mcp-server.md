# FEAT-007: MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose Graphify's graph analysis as an MCP server so AI assistants can programmatically query architecture data via 9 tools over stdio.

**Architecture:** New `graphify-mcp` crate with separate binary. Uses `rmcp` crate for JSON-RPC 2.0 over stdio. Eagerly extracts all projects on startup, holds `HashMap<String, QueryEngine>`. Tool handlers are thin wrappers around existing QueryEngine methods.

**Tech Stack:** Rust, rmcp (MCP SDK), tokio (async runtime), schemars (JSON Schema), clap (args), serde/serde_json

**Spec:** `docs/superpowers/specs/2026-04-12-feat-007-mcp-server-design.md`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` (root) | Modify | Add `graphify-mcp` to workspace members |
| `crates/graphify-mcp/Cargo.toml` | Create | Crate manifest with deps |
| `crates/graphify-mcp/src/main.rs` | Create | CLI args, config loading, extraction pipeline, tokio entry point |
| `crates/graphify-mcp/src/server.rs` | Create | `GraphifyServer` struct, `ServerHandler` impl, project resolution |
| `crates/graphify-mcp/src/tools.rs` | Create | 9 MCP tool parameter types + `#[tool_router]` impl with all tool handlers |
| `crates/graphify-mcp/tests/integration.rs` | Create | Spawn process, send JSON-RPC, verify responses |
| `docs/TaskNotes/Tasks/sprint.md` | Modify | Update FEAT-007 status |
| `CLAUDE.md` | Modify | Add `graphify-mcp` to architecture table |

---

### Task 1: Scaffold the graphify-mcp crate

**Files:**
- Modify: `Cargo.toml` (root, line 3-9)
- Create: `crates/graphify-mcp/Cargo.toml`
- Create: `crates/graphify-mcp/src/main.rs` (stub)

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p crates/graphify-mcp/src
```

- [ ] **Step 2: Create Cargo.toml for graphify-mcp**

Create `crates/graphify-mcp/Cargo.toml`:

```toml
[package]
name = "graphify-mcp"
version.workspace = true
edition.workspace = true

[[bin]]
name = "graphify-mcp"
path = "src/main.rs"

[dependencies]
graphify-core = { path = "../graphify-core" }
graphify-extract = { path = "../graphify-extract" }
rmcp = { version = "0.1", features = ["server", "transport-io", "macros"] }
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
toml = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
rayon = "1"
```

- [ ] **Step 3: Create stub main.rs**

Create `crates/graphify-mcp/src/main.rs`:

```rust
fn main() {
    println!("graphify-mcp stub");
}
```

- [ ] **Step 4: Add crate to workspace**

In root `Cargo.toml`, add `"crates/graphify-mcp"` to the `members` array:

```toml
[workspace]
resolver = "2"
members = [
    "crates/graphify-core",
    "crates/graphify-extract",
    "crates/graphify-report",
    "crates/graphify-cli",
    "crates/graphify-mcp",
    ".",
]
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo build -p graphify-mcp
```

Expected: successful build, binary at `target/debug/graphify-mcp`.

- [ ] **Step 6: Commit scaffold**

```bash
git add Cargo.toml Cargo.lock crates/graphify-mcp/
git commit -m "feat(mcp): scaffold graphify-mcp crate (FEAT-007)"
```

---

### Task 2: Config parsing and extraction pipeline

**Files:**
- Modify: `crates/graphify-mcp/src/main.rs`

Duplicate the config structs and extraction pipeline from `graphify-cli/src/main.rs`. These are ~80 lines of code that parse `graphify.toml` and run the extract→analyze pipeline.

- [ ] **Step 1: Write the config structs and extraction pipeline**

Replace `crates/graphify-mcp/src/main.rs` with:

```rust
mod server;
mod tools;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use rayon::prelude::*;
use serde::Deserialize;

use graphify_core::{
    community::detect_communities,
    cycles::{find_sccs, find_simple_cycles},
    graph::CodeGraph,
    metrics::{compute_metrics, ScoringWeights},
    query::QueryEngine,
    types::Language,
};
use graphify_extract::{
    walker::discover_files, ExtractionResult, LanguageExtractor, PythonExtractor,
    TypeScriptExtractor,
};

// ---------------------------------------------------------------------------
// Config structs (duplicated from graphify-cli — small, stable)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    project: Vec<ProjectConfig>,
}

#[derive(Deserialize, Default)]
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

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "graphify-mcp",
    about = "MCP server exposing Graphify graph analysis to AI assistants",
    version
)]
struct Cli {
    /// Path to graphify.toml config
    #[arg(long, default_value = "graphify.toml")]
    config: PathBuf,
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

fn load_config(path: &Path) -> Config {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Cannot read config {:?}: {e}", path);
            std::process::exit(1);
        }
    };
    match toml::from_str::<Config>(&text) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Invalid config {:?}: {e}", path);
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Language parsing
// ---------------------------------------------------------------------------

fn parse_languages(lang_strs: &[String]) -> Vec<Language> {
    lang_strs
        .iter()
        .filter_map(|s| match s.to_lowercase().as_str() {
            "python" | "py" => Some(Language::Python),
            "typescript" | "ts" => Some(Language::TypeScript),
            other => {
                eprintln!("Warning: unknown language '{other}', skipping.");
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Extraction pipeline
// ---------------------------------------------------------------------------

fn run_extract(project: &ProjectConfig, settings: &Settings) -> CodeGraph {
    let repo_path = PathBuf::from(&project.repo);
    let languages = parse_languages(&project.lang);
    let local_prefix = project.local_prefix.as_deref().unwrap_or("");

    let extra_owned: Vec<String> = settings.exclude.clone().unwrap_or_default();
    let extra_excludes: Vec<&str> = extra_owned.iter().map(|s| s.as_str()).collect();

    let files = discover_files(&repo_path, &languages, local_prefix, &extra_excludes);

    if files.len() <= 1 {
        eprintln!(
            "Warning: project '{}' discovered only {} file(s).",
            project.name,
            files.len(),
        );
    }

    let python_extractor = PythonExtractor::new();
    let typescript_extractor = TypeScriptExtractor::new();

    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    for file in &files {
        resolver.register_module(&file.module_name);
    }

    if languages.contains(&Language::TypeScript) {
        let tsconfig = repo_path.join("tsconfig.json");
        if tsconfig.exists() {
            resolver.load_tsconfig(&tsconfig);
        }
    }

    let results: Vec<ExtractionResult> = files
        .par_iter()
        .filter_map(|file| {
            let source = match std::fs::read(&file.path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Warning: cannot read {:?}: {e}", file.path);
                    return None;
                }
            };
            let extractor: &dyn LanguageExtractor = match file.language {
                Language::Python => &python_extractor,
                Language::TypeScript => &typescript_extractor,
            };
            Some(extractor.extract_file(&file.path, &source, &file.module_name))
        })
        .collect();

    let mut all_nodes = Vec::new();
    let mut all_raw_edges: Vec<(String, String, graphify_core::types::Edge)> = Vec::new();
    for result in results {
        all_nodes.extend(result.nodes);
        all_raw_edges.extend(result.edges);
    }

    let mut graph = CodeGraph::new();
    if let Some(lang) = languages.first() {
        graph.set_default_language(lang.clone());
    }
    for node in all_nodes {
        graph.add_node(node);
    }

    let package_modules: HashSet<&str> = files
        .iter()
        .filter(|f| f.is_package)
        .map(|f| f.module_name.as_str())
        .collect();

    for (src_id, raw_target, edge) in all_raw_edges {
        let is_package = package_modules.contains(src_id.as_str());
        let (resolved_target, _is_local) = resolver.resolve(&raw_target, &src_id, is_package);
        graph.add_edge(&src_id, &resolved_target, edge);
    }

    graph
}

// ---------------------------------------------------------------------------
// Build QueryEngine for a project
// ---------------------------------------------------------------------------

fn build_query_engine(project: &ProjectConfig, settings: &Settings) -> QueryEngine {
    let graph = run_extract(project, settings);
    let w = resolve_weights(settings);
    let mut metrics = compute_metrics(&graph, &w);
    let communities = detect_communities(&graph);
    assign_community_ids(&mut metrics, &communities);
    let cycles = find_sccs(&graph);
    QueryEngine::from_analyzed(graph, metrics, communities, cycles)
}

fn assign_community_ids(
    metrics: &mut [graphify_core::metrics::NodeMetrics],
    communities: &[graphify_core::community::Community],
) {
    let mut id_map: HashMap<&str, usize> = HashMap::new();
    for community in communities {
        for member in &community.members {
            id_map.insert(member.as_str(), community.id);
        }
    }
    for m in metrics.iter_mut() {
        if let Some(&cid) = id_map.get(m.id.as_str()) {
            m.community_id = cid;
        }
    }
}

fn resolve_weights(settings: &Settings) -> ScoringWeights {
    if let Some(ref v) = settings.weights {
        if v.len() == 4 {
            return ScoringWeights {
                betweenness: v[0],
                pagerank: v[1],
                in_degree: v[2],
                in_cycle: v[3],
            };
        }
    }
    ScoringWeights::default()
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let cfg = load_config(&cli.config);

    if cfg.project.is_empty() {
        eprintln!("No [[project]] entries in config.");
        std::process::exit(1);
    }

    // Build QueryEngines for all projects
    let mut engines: HashMap<String, QueryEngine> = HashMap::new();
    let mut project_names: Vec<String> = Vec::new();

    for project in &cfg.project {
        eprintln!("[graphify-mcp] Extracting project '{}'...", project.name);
        let engine = build_query_engine(project, &cfg.settings);
        let stats = engine.stats();
        eprintln!(
            "[graphify-mcp] {} — {} nodes, {} edges, {} communities, {} cycles",
            project.name,
            stats.node_count,
            stats.edge_count,
            stats.community_count,
            stats.cycle_count,
        );
        project_names.push(project.name.clone());
        engines.insert(project.name.clone(), engine);
    }

    let default_project = project_names[0].clone();

    eprintln!(
        "[graphify-mcp] Ready — {} project(s), default: '{}'",
        engines.len(),
        default_project,
    );

    // Start MCP server over stdio
    let graphify_server =
        server::GraphifyServer::new(engines, default_project, project_names);

    let transport = rmcp::transport::io::stdio();
    let server = rmcp::service::serve_server(graphify_server, transport)
        .await
        .expect("Failed to start MCP server");

    server.waiting().await.expect("MCP server error");
}
```

- [ ] **Step 2: Create empty module files**

Create `crates/graphify-mcp/src/server.rs`:

```rust
pub struct GraphifyServer;

impl GraphifyServer {
    pub fn new(
        _engines: std::collections::HashMap<String, graphify_core::query::QueryEngine>,
        _default_project: String,
        _project_names: Vec<String>,
    ) -> Self {
        Self
    }
}
```

Create `crates/graphify-mcp/src/tools.rs`:

```rust
// Tool implementations will go here.
```

- [ ] **Step 3: Verify it compiles (may have warnings)**

```bash
cargo build -p graphify-mcp 2>&1
```

Expected: compiles (warnings about unused variables are OK at this stage).

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-mcp/
git commit -m "feat(mcp): config parsing, extraction pipeline, CLI entry point (FEAT-007)"
```

---

### Task 3: GraphifyServer struct and ServerHandler implementation

**Files:**
- Modify: `crates/graphify-mcp/src/server.rs`

- [ ] **Step 1: Write the GraphifyServer with project resolution**

Replace `crates/graphify-mcp/src/server.rs` with:

```rust
use std::collections::HashMap;

use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::ServerInfo;
use rmcp::ServerHandler;

use graphify_core::query::QueryEngine;

use crate::tools;

// ---------------------------------------------------------------------------
// GraphifyServer
// ---------------------------------------------------------------------------

pub struct GraphifyServer {
    engines: HashMap<String, QueryEngine>,
    default_project: String,
    project_names: Vec<String>,
    tool_router: ToolRouter<Self>,
}

impl GraphifyServer {
    pub fn new(
        engines: HashMap<String, QueryEngine>,
        default_project: String,
        project_names: Vec<String>,
    ) -> Self {
        Self {
            engines,
            default_project,
            project_names,
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve the `project` parameter to a QueryEngine reference.
    ///
    /// - `Some(name)` that exists → that engine
    /// - `Some(name)` that doesn't exist → Err listing available projects
    /// - `None` → default project engine
    pub fn resolve_engine(&self, project: Option<&str>) -> Result<&QueryEngine, String> {
        let name = project.unwrap_or(&self.default_project);
        self.engines.get(name).ok_or_else(|| {
            format!(
                "Project '{}' not found. Available: {}",
                name,
                self.project_names.join(", ")
            )
        })
    }

    /// Returns the list of available project names.
    pub fn project_names(&self) -> &[String] {
        &self.project_names
    }

    /// Returns the default project name.
    pub fn default_project(&self) -> &str {
        &self.default_project
    }
}

#[rmcp::tool_router]
impl GraphifyServer {
    // Tool implementations are added in tools.rs via a separate #[tool_router] block.
    // This impl block is the primary router; tools.rs merges into it.
}

impl ServerHandler for GraphifyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Graphify MCP server. Query dependency graphs for architectural analysis. \
                 Use graphify_stats to start, then graphify_search/graphify_explain for details."
                    .into(),
            ),
            ..ServerInfo::new("graphify-mcp", env!("CARGO_PKG_VERSION"))
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p graphify-mcp 2>&1
```

Expected: compiles. If `rmcp` API differs slightly from docs, adjust imports (e.g. `ServerInfo` may be at `rmcp::model::ServerInfo` or `rmcp::ServerInfo`). Fix any compilation errors by checking `rmcp` re-exports.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-mcp/src/server.rs
git commit -m "feat(mcp): GraphifyServer struct with project resolution (FEAT-007)"
```

---

### Task 4: MCP tool parameter types

**Files:**
- Modify: `crates/graphify-mcp/src/tools.rs`

Define all input parameter structs for the 9 tools. Each struct derives `Deserialize` and `JsonSchema` (for rmcp's JSON Schema generation).

- [ ] **Step 1: Write all parameter structs**

Replace `crates/graphify-mcp/src/tools.rs` with:

```rust
use schemars::JsonSchema;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Tool parameter types
// ---------------------------------------------------------------------------

/// Parameters for graphify_stats
#[derive(Deserialize, JsonSchema, Default)]
pub struct StatsParams {
    /// Project name (optional, defaults to first project)
    pub project: Option<String>,
}

/// Parameters for graphify_search
#[derive(Deserialize, JsonSchema, Default)]
pub struct SearchParams {
    /// Glob pattern to match node IDs (e.g. "app.services.*")
    pub pattern: String,
    /// Filter by node kind: module, function, class, method
    pub kind: Option<String>,
    /// Sort results: score (default), name, in_degree
    pub sort: Option<String>,
    /// Only return local (in-project) nodes
    pub local_only: Option<bool>,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_explain
#[derive(Deserialize, JsonSchema, Default)]
pub struct ExplainParams {
    /// Full node ID (e.g. "app.services.llm")
    pub node_id: String,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_path
#[derive(Deserialize, JsonSchema, Default)]
pub struct PathParams {
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_all_paths
#[derive(Deserialize, JsonSchema, Default)]
pub struct AllPathsParams {
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Maximum path length in edges (default: 10)
    pub max_depth: Option<usize>,
    /// Maximum number of paths to return (default: 20)
    pub max_paths: Option<usize>,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_dependents
#[derive(Deserialize, JsonSchema, Default)]
pub struct DependentsParams {
    /// Node ID to find dependents of
    pub node_id: String,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_dependencies
#[derive(Deserialize, JsonSchema, Default)]
pub struct DependenciesParams {
    /// Node ID to find dependencies of
    pub node_id: String,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_suggest
#[derive(Deserialize, JsonSchema, Default)]
pub struct SuggestParams {
    /// Partial node ID to autocomplete
    pub input: String,
    /// Project name (optional)
    pub project: Option<String>,
}

/// Parameters for graphify_transitive_dependents
#[derive(Deserialize, JsonSchema, Default)]
pub struct TransitiveDepsParams {
    /// Node ID to find transitive dependents of
    pub node_id: String,
    /// Maximum number of hops to traverse (default: 5)
    pub max_depth: Option<usize>,
    /// Project name (optional)
    pub project: Option<String>,
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p graphify-mcp 2>&1
```

Expected: compiles (unused warnings are OK).

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-mcp/src/tools.rs
git commit -m "feat(mcp): tool parameter types for all 9 MCP tools (FEAT-007)"
```

---

### Task 5: Tool handler implementations

**Files:**
- Modify: `crates/graphify-mcp/src/tools.rs` (append tool_router impl)
- Modify: `crates/graphify-mcp/src/server.rs` (merge tool routers)

This is the core task — implement all 9 tool handlers using the `#[tool_router]` and `#[tool]` macros.

- [ ] **Step 1: Add tool handler implementations to tools.rs**

Append to the bottom of `crates/graphify-mcp/src/tools.rs`:

```rust
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::Content;
use rmcp::{tool, tool_router};

use graphify_core::query::{SearchFilters, SortField};
use graphify_core::types::NodeKind;

use crate::server::GraphifyServer;

// ---------------------------------------------------------------------------
// Helper: serialize result to MCP text content
// ---------------------------------------------------------------------------

fn json_content<T: serde::Serialize>(value: &T) -> Content {
    Content::text(serde_json::to_string_pretty(value).unwrap_or_else(|e| {
        format!("{{\"error\": \"serialization failed: {}\"}}", e)
    }))
}

fn error_content(msg: &str) -> Content {
    Content::text(format!("{{\"error\": \"{}\"}}", msg))
}

// ---------------------------------------------------------------------------
// Helper: parse NodeKind from string
// ---------------------------------------------------------------------------

fn parse_node_kind(s: &str) -> Option<NodeKind> {
    match s.to_lowercase().as_str() {
        "module" | "mod" => Some(NodeKind::Module),
        "function" | "func" | "fn" => Some(NodeKind::Function),
        "class" => Some(NodeKind::Class),
        "method" => Some(NodeKind::Method),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl GraphifyServer {
    #[tool(
        name = "graphify_stats",
        description = "Get high-level statistics about the dependency graph: node count, edge count, local modules, communities, and cycles."
    )]
    fn tool_stats(&self, Parameters(params): Parameters<StatsParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => vec![json_content(&engine.stats())],
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_search",
        description = "Search for code modules by glob pattern. Supports * (any chars) and ? (single char) wildcards. Filter by kind (module/function/class/method), sort by score/name/in_degree."
    )]
    fn tool_search(&self, Parameters(params): Parameters<SearchParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let sort_field = match params.sort.as_deref() {
                    Some("name") => SortField::Name,
                    Some("in_degree") | Some("indegree") => SortField::InDegree,
                    _ => SortField::Score,
                };
                let filters = SearchFilters {
                    kind: params.kind.as_deref().and_then(parse_node_kind),
                    sort_by: sort_field,
                    local_only: params.local_only.unwrap_or(false),
                };
                let results = engine.search(&params.pattern, &filters);
                vec![json_content(&results)]
            }
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_explain",
        description = "Get a detailed profile of a code module: metrics (betweenness, PageRank, score), community, cycle participation, and dependencies. Use to understand a module's architectural role."
    )]
    fn tool_explain(&self, Parameters(params): Parameters<ExplainParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => match engine.explain(&params.node_id) {
                Some(report) => vec![json_content(&report)],
                None => {
                    let suggestions = engine.suggest(&params.node_id);
                    if suggestions.is_empty() {
                        vec![error_content(&format!(
                            "Node '{}' not found.",
                            params.node_id
                        ))]
                    } else {
                        vec![error_content(&format!(
                            "Node '{}' not found. Did you mean: {}?",
                            params.node_id,
                            suggestions.join(", ")
                        ))]
                    }
                }
            },
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_path",
        description = "Find the shortest dependency path between two modules. Returns step-by-step path with edge types (Imports/Defines/Calls) and weights."
    )]
    fn tool_path(&self, Parameters(params): Parameters<PathParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => match engine.shortest_path(&params.source, &params.target) {
                Some(path) => vec![json_content(&path)],
                None => vec![error_content(&format!(
                    "No path found from '{}' to '{}'.",
                    params.source, params.target
                ))],
            },
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_all_paths",
        description = "Find all dependency paths between two modules, bounded by depth and count limits. Use when you need to understand all possible dependency chains."
    )]
    fn tool_all_paths(&self, Parameters(params): Parameters<AllPathsParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let max_depth = params.max_depth.unwrap_or(10);
                let max_paths = params.max_paths.unwrap_or(20);
                let paths =
                    engine.all_paths(&params.source, &params.target, max_depth, max_paths);
                if paths.is_empty() {
                    vec![error_content(&format!(
                        "No paths found from '{}' to '{}'.",
                        params.source, params.target
                    ))]
                } else {
                    vec![json_content(&paths)]
                }
            }
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_dependents",
        description = "List modules that depend on (import/call) the given module. Shows who would be affected if this module changes."
    )]
    fn tool_dependents(&self, Parameters(params): Parameters<DependentsParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let deps = engine.dependents(&params.node_id);
                vec![json_content(&deps)]
            }
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_dependencies",
        description = "List modules that the given module depends on (imports/calls). Shows what this module needs to function."
    )]
    fn tool_dependencies(
        &self,
        Parameters(params): Parameters<DependenciesParams>,
    ) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let deps = engine.dependencies(&params.node_id);
                vec![json_content(&deps)]
            }
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_suggest",
        description = "Autocomplete node IDs. Returns up to 3 suggestions matching the input as a case-insensitive substring. Use before other tools when unsure of exact node names."
    )]
    fn tool_suggest(&self, Parameters(params): Parameters<SuggestParams>) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let suggestions = engine.suggest(&params.input);
                vec![json_content(&suggestions)]
            }
            Err(msg) => vec![error_content(&msg)],
        }
    }

    #[tool(
        name = "graphify_transitive_dependents",
        description = "Find all transitive dependents of a module up to N hops away. Shows the full blast radius — everything affected by changes to this module."
    )]
    fn tool_transitive_deps(
        &self,
        Parameters(params): Parameters<TransitiveDepsParams>,
    ) -> Vec<Content> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let max_depth = params.max_depth.unwrap_or(5);
                let deps = engine.transitive_dependents(&params.node_id, max_depth);
                vec![json_content(&deps)]
            }
            Err(msg) => vec![error_content(&msg)],
        }
    }
}
```

- [ ] **Step 2: Update server.rs to merge tool routers**

The `#[tool_router]` macro on both `server.rs` and `tools.rs` generates two routers for `GraphifyServer`. These need to be merged. Remove the empty `#[tool_router]` block from `server.rs` and instead have the `new()` method reference the tool router generated in `tools.rs`.

In `server.rs`, change the `tool_router` field initialization in `new()`:

```rust
// The tool_router is generated by #[tool_router] in tools.rs
tool_router: Self::tool_router(),
```

Remove the empty `#[tool_router] impl GraphifyServer {}` block from `server.rs`. The `#[tool_router]` in `tools.rs` generates the `tool_router()` function.

- [ ] **Step 3: Verify it compiles**

```bash
cargo build -p graphify-mcp 2>&1
```

Expected: compiles. If `rmcp` API requires different return types (e.g., `CallToolResult` instead of `Vec<Content>`), adjust accordingly. The `#[tool]` macro typically handles wrapping the return type.

**Troubleshooting:** If the `Content::text()` constructor doesn't exist, check `rmcp::model::Content` variants. It may be `Content::Text { text: String }` or use a builder. Adjust based on actual rmcp API.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-mcp/src/tools.rs crates/graphify-mcp/src/server.rs
git commit -m "feat(mcp): implement all 9 MCP tool handlers (FEAT-007)"
```

---

### Task 6: Unit tests for project resolution and tool handlers

**Files:**
- Modify: `crates/graphify-mcp/src/server.rs` (add tests module)

- [ ] **Step 1: Write unit tests for project resolution**

Add to the bottom of `crates/graphify-mcp/src/server.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::{
        community::detect_communities,
        cycles::find_sccs,
        graph::CodeGraph,
        metrics::{compute_metrics, ScoringWeights},
        types::{Edge, Language, Node},
    };

    /// Build a small test graph with known structure:
    ///   app.main → app.services.llm → app.utils.helpers
    ///                               → app.models.user
    fn build_test_engine() -> QueryEngine {
        let mut graph = CodeGraph::new();

        graph.add_node(Node::module(
            "app.main",
            "app/main.py",
            Language::Python,
            1,
            true,
        ));
        graph.add_node(Node::module(
            "app.services.llm",
            "app/services/llm.py",
            Language::Python,
            1,
            true,
        ));
        graph.add_node(Node::module(
            "app.utils.helpers",
            "app/utils/helpers.py",
            Language::Python,
            1,
            true,
        ));
        graph.add_node(Node::module(
            "app.models.user",
            "app/models/user.py",
            Language::Python,
            1,
            true,
        ));

        graph.add_edge("app.main", "app.services.llm", Edge::imports(1));
        graph.add_edge("app.services.llm", "app.utils.helpers", Edge::imports(2));
        graph.add_edge("app.services.llm", "app.models.user", Edge::imports(3));

        let w = ScoringWeights::default();
        let metrics = compute_metrics(&graph, &w);
        let communities = detect_communities(&graph);
        let cycles = find_sccs(&graph);

        QueryEngine::from_analyzed(graph, metrics, communities, cycles)
    }

    fn build_test_server() -> GraphifyServer {
        let mut engines = HashMap::new();
        engines.insert("test-project".to_string(), build_test_engine());

        GraphifyServer::new(
            engines,
            "test-project".to_string(),
            vec!["test-project".to_string()],
        )
    }

    #[test]
    fn resolve_engine_default() {
        let server = build_test_server();
        assert!(server.resolve_engine(None).is_ok());
    }

    #[test]
    fn resolve_engine_explicit_valid() {
        let server = build_test_server();
        assert!(server.resolve_engine(Some("test-project")).is_ok());
    }

    #[test]
    fn resolve_engine_explicit_invalid() {
        let server = build_test_server();
        let err = server.resolve_engine(Some("nonexistent")).unwrap_err();
        assert!(err.contains("not found"));
        assert!(err.contains("test-project"));
    }

    #[test]
    fn stats_returns_expected_counts() {
        let server = build_test_server();
        let engine = server.resolve_engine(None).unwrap();
        let stats = engine.stats();
        assert_eq!(stats.node_count, 4);
        assert_eq!(stats.edge_count, 3);
        assert_eq!(stats.local_node_count, 4);
    }

    #[test]
    fn search_glob_pattern() {
        let server = build_test_server();
        let engine = server.resolve_engine(None).unwrap();
        let filters = graphify_core::query::SearchFilters::default();
        let results = engine.search("app.services.*", &filters);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node_id, "app.services.llm");
    }

    #[test]
    fn explain_existing_node() {
        let server = build_test_server();
        let engine = server.resolve_engine(None).unwrap();
        let report = engine.explain("app.services.llm");
        assert!(report.is_some());
        let r = report.unwrap();
        assert_eq!(r.node_id, "app.services.llm");
        assert_eq!(r.direct_dependencies.len(), 2);
    }

    #[test]
    fn explain_nonexistent_returns_none() {
        let server = build_test_server();
        let engine = server.resolve_engine(None).unwrap();
        assert!(engine.explain("nonexistent.module").is_none());
    }

    #[test]
    fn shortest_path_exists() {
        let server = build_test_server();
        let engine = server.resolve_engine(None).unwrap();
        let path = engine.shortest_path("app.main", "app.utils.helpers");
        assert!(path.is_some());
        let steps = path.unwrap();
        assert_eq!(steps.len(), 3); // main → llm → helpers
    }

    #[test]
    fn suggest_partial_match() {
        let server = build_test_server();
        let engine = server.resolve_engine(None).unwrap();
        let suggestions = engine.suggest("llm");
        assert!(suggestions.contains(&"app.services.llm".to_string()));
    }
}
```

- [ ] **Step 2: Run unit tests**

```bash
cargo test -p graphify-mcp -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-mcp/src/server.rs
git commit -m "test(mcp): unit tests for project resolution and QueryEngine (FEAT-007)"
```

---

### Task 7: End-to-end smoke test

**Files:**
- Create: `crates/graphify-mcp/tests/integration.rs`

This test spawns the `graphify-mcp` binary, sends JSON-RPC messages, and verifies responses.

- [ ] **Step 1: Create test fixture directory with Python files**

The test needs a temporary directory with some Python files and a `graphify.toml`. We'll create them programmatically.

- [ ] **Step 2: Write the integration test**

Create `crates/graphify-mcp/tests/integration.rs`:

```rust
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Send a JSON-RPC request and read the response.
fn send_jsonrpc(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    request: &serde_json::Value,
) -> serde_json::Value {
    let msg = serde_json::to_string(request).unwrap();
    stdin
        .write_all(format!("{}\n", msg).as_bytes())
        .expect("write to stdin");
    stdin.flush().expect("flush stdin");

    let mut line = String::new();
    stdout.read_line(&mut line).expect("read from stdout");
    serde_json::from_str(&line).expect("parse JSON response")
}

#[test]
fn mcp_server_responds_to_initialize() {
    // Create temp dir with a simple Python project
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project_dir = tmp.path().join("myproject");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a Python file
    std::fs::write(
        project_dir.join("main.py"),
        "from myproject import utils\n\ndef run():\n    utils.helper()\n",
    )
    .unwrap();
    std::fs::write(
        project_dir.join("utils.py"),
        "def helper():\n    return 42\n",
    )
    .unwrap();

    // Create graphify.toml
    let config_content = format!(
        r#"[settings]
output = "{}/report"

[[project]]
name = "test"
repo = "{}"
lang = ["python"]
local_prefix = "myproject"
"#,
        tmp.path().display(),
        project_dir.display()
    );
    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(&config_path, config_content).unwrap();

    // Find the binary
    let binary = std::env::current_dir()
        .unwrap()
        .join("target/debug/graphify-mcp");

    if !binary.exists() {
        eprintln!("Binary not found at {:?}, skipping integration test.", binary);
        return;
    }

    // Spawn the MCP server
    let mut child = Command::new(&binary)
        .arg("--config")
        .arg(&config_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn graphify-mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    // Send initialize request
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "0.1.0" }
        }
    });

    let response = send_jsonrpc(&mut stdin, &mut stdout, &init_request);
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response["result"]["serverInfo"]["name"]
        .as_str()
        .unwrap()
        .contains("graphify"));

    // Send initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let msg = serde_json::to_string(&initialized).unwrap();
    stdin
        .write_all(format!("{}\n", msg).as_bytes())
        .expect("write initialized");
    stdin.flush().expect("flush");

    // Request tools/list
    let list_tools = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });

    let response = send_jsonrpc(&mut stdin, &mut stdout, &list_tools);
    let tools = response["result"]["tools"].as_array().expect("tools array");
    assert!(tools.len() >= 9, "Expected at least 9 tools, got {}", tools.len());

    // Check tool names
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"graphify_stats"));
    assert!(tool_names.contains(&"graphify_search"));
    assert!(tool_names.contains(&"graphify_explain"));
    assert!(tool_names.contains(&"graphify_path"));
    assert!(tool_names.contains(&"graphify_suggest"));

    // Call graphify_stats
    let call_stats = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "graphify_stats",
            "arguments": {}
        }
    });

    let response = send_jsonrpc(&mut stdin, &mut stdout, &call_stats);
    let content = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let stats: serde_json::Value = serde_json::from_str(content).expect("parse stats JSON");
    assert!(stats["node_count"].as_u64().unwrap() > 0);

    // Clean up
    drop(stdin);
    child.kill().ok();
}
```

- [ ] **Step 3: Add test dependencies to Cargo.toml**

Add to `crates/graphify-mcp/Cargo.toml`:

```toml
[dev-dependencies]
tempfile = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 4: Build and run integration test**

```bash
cargo build -p graphify-mcp && cargo test -p graphify-mcp --test integration -- --nocapture
```

Expected: test passes — MCP server starts, responds to initialize, lists 9 tools, and returns stats.

**Troubleshooting:** If the server hangs on startup, check stderr for extraction errors. If JSON-RPC framing differs (e.g., content-length headers vs newline-delimited), adjust the `send_jsonrpc` helper.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-mcp/tests/ crates/graphify-mcp/Cargo.toml
git commit -m "test(mcp): integration test for MCP server lifecycle (FEAT-007)"
```

---

### Task 8: Quality gates and documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`

- [ ] **Step 1: Run full quality checks**

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Expected: all pass. Fix any fmt/clippy issues.

- [ ] **Step 2: Update CLAUDE.md architecture table**

In `CLAUDE.md`, add `graphify-mcp` to the architecture table:

```markdown
| `graphify-mcp` | MCP server exposing graph queries to AI assistants | rmcp, tokio, clap |
```

Also update the test count if it changed.

- [ ] **Step 3: Update sprint board**

In `docs/TaskNotes/Tasks/sprint.md`, change FEAT-007 status from `**open**` to `**done**`:

```markdown
| FEAT-007 | **done** | normal   | 16h    | MCP server for graph queries                         |
```

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs/TaskNotes/Tasks/sprint.md
git commit -m "docs: update architecture and sprint board for FEAT-007 MCP server"
```

---

## Post-Implementation Checklist

- [ ] All `cargo test --workspace` pass (including new MCP tests)
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `graphify-mcp --help` prints usage info
- [ ] `graphify-mcp --config graphify.toml` starts and logs extraction to stderr
- [ ] Integration test verifies initialize → tools/list → tools/call flow
- [ ] CLAUDE.md updated with new crate
- [ ] Sprint board updated
