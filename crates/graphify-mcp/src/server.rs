use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::{CallToolResult, Content, ServerInfo};
use rmcp::{tool, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

use graphify_core::query::{QueryEngine, SearchFilters, SortField};

// ---------------------------------------------------------------------------
// GraphifyServer
// ---------------------------------------------------------------------------

/// MCP server that exposes Graphify graph analysis tools.
#[derive(Clone)]
pub struct GraphifyServer {
    pub engines: Arc<HashMap<String, QueryEngine>>,
    pub default_project: String,
    pub project_names: Vec<String>,
}

impl GraphifyServer {
    /// Creates a new server with the given query engines.
    pub fn new(
        engines: HashMap<String, QueryEngine>,
        default_project: String,
        project_names: Vec<String>,
    ) -> Self {
        Self {
            engines: Arc::new(engines),
            default_project,
            project_names,
        }
    }

    /// Resolves a project name to its QueryEngine.
    ///
    /// If `project` is `None`, the default project is used.
    /// Returns an error message if the project is not found.
    pub fn resolve_engine(&self, project: Option<&str>) -> Result<&QueryEngine, String> {
        let name = project.unwrap_or(&self.default_project);
        self.engines.get(name).ok_or_else(|| {
            format!(
                "Project '{}' not found. Available: {:?}",
                name, self.project_names
            )
        })
    }
}

// ---------------------------------------------------------------------------
// ServerHandler implementation
// ---------------------------------------------------------------------------

#[tool(tool_box)]
impl ServerHandler for GraphifyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Graphify MCP server for architectural analysis of codebases. \
                 Use the available tools to query dependency graphs, find paths, \
                 and explore module relationships."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Parameter types
// ---------------------------------------------------------------------------

/// Parameters for the graphify_stats tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct StatsParams {
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_search tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct SearchParams {
    /// Glob pattern to match node IDs (e.g. "app.services.*").
    pub pattern: String,
    /// Filter by node kind: module, function, class, method.
    pub kind: Option<String>,
    /// Sort results: score (default), name, in_degree.
    pub sort: Option<String>,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_explain tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct ExplainParams {
    /// Node ID to explain (e.g. "app.services.llm").
    pub node_id: String,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_path tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct PathParams {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_all_paths tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct AllPathsParams {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Maximum path depth (default: 10).
    pub max_depth: Option<usize>,
    /// Maximum number of paths to return (default: 20).
    pub max_paths: Option<usize>,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_dependents tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct DependentsParams {
    /// Node ID to find dependents for.
    pub node_id: String,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_dependencies tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct DependenciesParams {
    /// Node ID to find dependencies for.
    pub node_id: String,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_suggest tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct SuggestParams {
    /// Partial node ID to auto-complete.
    pub input: String,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

/// Parameters for the graphify_transitive_dependents tool.
#[derive(Deserialize, JsonSchema, Default)]
pub struct TransitiveDepsParams {
    /// Node ID to find transitive dependents for.
    pub node_id: String,
    /// Maximum depth for transitive search (default: 10).
    pub max_depth: Option<usize>,
    /// Project name. Uses the default project if omitted.
    pub project: Option<String>,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Serializes a value to a JSON string.
fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {e}\"}}"))
}

/// Creates an error CallToolResult with a message.
fn error_result(msg: &str) -> CallToolResult {
    let err = serde_json::json!({ "error": msg });
    CallToolResult::error(vec![Content::text(err.to_string())])
}

/// Parses a node kind string into a NodeKind enum.
fn parse_node_kind(s: &str) -> Option<graphify_core::types::NodeKind> {
    match s.to_lowercase().as_str() {
        "module" | "mod" => Some(graphify_core::types::NodeKind::Module),
        "function" | "func" | "fn" => Some(graphify_core::types::NodeKind::Function),
        "class" => Some(graphify_core::types::NodeKind::Class),
        "method" => Some(graphify_core::types::NodeKind::Method),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool(tool_box)]
impl GraphifyServer {
    /// Returns high-level statistics about the dependency graph.
    #[tool(
        name = "graphify_stats",
        description = "Get graph statistics: node/edge counts, communities, cycles"
    )]
    fn graphify_stats(
        &self,
        #[tool(aggr)] params: StatsParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => Ok(CallToolResult::success(vec![Content::text(to_json(
                &engine.stats(),
            ))])),
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Searches for nodes matching a glob pattern.
    #[tool(
        name = "graphify_search",
        description = "Search nodes by glob pattern (e.g. 'app.services.*'). Returns matching modules with scores."
    )]
    fn graphify_search(
        &self,
        #[tool(aggr)] params: SearchParams,
    ) -> Result<CallToolResult, rmcp::Error> {
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
                    local_only: false,
                };
                let results = engine.search(&params.pattern, &filters);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &results,
                ))]))
            }
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Returns a detailed profile of a node.
    #[tool(
        name = "graphify_explain",
        description = "Explain a module: metrics, community, cycles, dependents, dependencies, and impact analysis"
    )]
    fn graphify_explain(
        &self,
        #[tool(aggr)] params: ExplainParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => match engine.explain(&params.node_id) {
                Some(report) => Ok(CallToolResult::success(vec![Content::text(to_json(
                    &report,
                ))])),
                None => {
                    let suggestions = engine.suggest(&params.node_id);
                    let msg = if suggestions.is_empty() {
                        format!("Node '{}' not found.", params.node_id)
                    } else {
                        format!(
                            "Node '{}' not found. Did you mean: {}?",
                            params.node_id,
                            suggestions.join(", ")
                        )
                    };
                    Ok(error_result(&msg))
                }
            },
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Finds the shortest path between two nodes using BFS.
    #[tool(
        name = "graphify_path",
        description = "Find shortest dependency path between two nodes"
    )]
    fn graphify_path(
        &self,
        #[tool(aggr)] params: PathParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => match engine.shortest_path(&params.source, &params.target) {
                Some(path) => {
                    let result = serde_json::json!({
                        "source": params.source,
                        "target": params.target,
                        "hops": path.len().saturating_sub(1),
                        "path": path,
                    });
                    Ok(CallToolResult::success(vec![Content::text(to_json(
                        &result,
                    ))]))
                }
                None => Ok(error_result(&format!(
                    "No path found from '{}' to '{}'.",
                    params.source, params.target
                ))),
            },
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Finds all paths between two nodes, limited by depth and count.
    #[tool(
        name = "graphify_all_paths",
        description = "Find all dependency paths between two nodes (with depth and count limits)"
    )]
    fn graphify_all_paths(
        &self,
        #[tool(aggr)] params: AllPathsParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        let max_depth = params.max_depth.unwrap_or(10);
        let max_paths = params.max_paths.unwrap_or(20);
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let paths = engine.all_paths(&params.source, &params.target, max_depth, max_paths);
                if paths.is_empty() {
                    Ok(error_result(&format!(
                        "No paths found from '{}' to '{}'.",
                        params.source, params.target
                    )))
                } else {
                    let result = serde_json::json!({
                        "source": params.source,
                        "target": params.target,
                        "path_count": paths.len(),
                        "paths": paths,
                    });
                    Ok(CallToolResult::success(vec![Content::text(to_json(
                        &result,
                    ))]))
                }
            }
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Returns the direct dependents (incoming edges) of a node.
    #[tool(
        name = "graphify_dependents",
        description = "List modules that depend on a given node (incoming edges)"
    )]
    fn graphify_dependents(
        &self,
        #[tool(aggr)] params: DependentsParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let deps = engine.dependents(&params.node_id);
                let result: Vec<serde_json::Value> = deps
                    .into_iter()
                    .map(|(id, kind)| {
                        serde_json::json!({
                            "node_id": id,
                            "edge_kind": format!("{:?}", kind),
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &result,
                ))]))
            }
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Returns the direct dependencies (outgoing edges) of a node.
    #[tool(
        name = "graphify_dependencies",
        description = "List modules that a given node depends on (outgoing edges)"
    )]
    fn graphify_dependencies(
        &self,
        #[tool(aggr)] params: DependenciesParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let deps = engine.dependencies(&params.node_id);
                let result: Vec<serde_json::Value> = deps
                    .into_iter()
                    .map(|(id, kind)| {
                        serde_json::json!({
                            "node_id": id,
                            "edge_kind": format!("{:?}", kind),
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &result,
                ))]))
            }
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Auto-completes a partial node ID, returning up to 3 suggestions.
    #[tool(
        name = "graphify_suggest",
        description = "Auto-complete a partial node ID (returns up to 3 suggestions)"
    )]
    fn graphify_suggest(
        &self,
        #[tool(aggr)] params: SuggestParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let suggestions = engine.suggest(&params.input);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &suggestions,
                ))]))
            }
            Err(e) => Ok(error_result(&e)),
        }
    }

    /// Returns all transitive dependents of a node up to a given depth.
    #[tool(
        name = "graphify_transitive_dependents",
        description = "Find all transitive dependents of a node up to max_depth hops"
    )]
    fn graphify_transitive_dependents(
        &self,
        #[tool(aggr)] params: TransitiveDepsParams,
    ) -> Result<CallToolResult, rmcp::Error> {
        let max_depth = params.max_depth.unwrap_or(10);
        match self.resolve_engine(params.project.as_deref()) {
            Ok(engine) => {
                let deps = engine.transitive_dependents(&params.node_id, max_depth);
                let result: Vec<serde_json::Value> = deps
                    .into_iter()
                    .map(|(id, depth)| {
                        serde_json::json!({
                            "node_id": id,
                            "depth": depth,
                        })
                    })
                    .collect();
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &result,
                ))]))
            }
            Err(e) => Ok(error_result(&e)),
        }
    }
}
