mod server;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use rayon::prelude::*;
use serde::Deserialize;

use graphify_core::{
    community::detect_communities,
    cycles::find_sccs,
    graph::CodeGraph,
    metrics::{compute_metrics, NodeMetrics, ScoringWeights},
    query::QueryEngine,
    types::Language,
};
use graphify_extract::{
    walker::discover_files, ExtractionResult, LanguageExtractor, PythonExtractor,
    TypeScriptExtractor,
};

use crate::server::GraphifyServer;

// ---------------------------------------------------------------------------
// Config structs (duplicated from graphify-cli)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    project: Vec<ProjectConfig>,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
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
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "graphify-mcp",
    about = "MCP server exposing Graphify architectural analysis",
    version
)]
struct Cli {
    /// Path to graphify.toml config
    #[arg(long, default_value = "graphify.toml")]
    config: PathBuf,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let config = load_config(&cli.config);

    if config.project.is_empty() {
        eprintln!("Error: no [[project]] entries in config.");
        std::process::exit(1);
    }

    eprintln!(
        "graphify-mcp: extracting {} project(s)...",
        config.project.len()
    );

    // Build QueryEngine for each project eagerly.
    let mut engines: HashMap<String, QueryEngine> = HashMap::new();
    let project_names: Vec<String> = config.project.iter().map(|p| p.name.clone()).collect();

    for project in &config.project {
        eprintln!("  extracting '{}'...", project.name);
        let engine = build_query_engine(project, &config.settings);
        let stats = engine.stats();
        eprintln!(
            "  '{}': {} nodes, {} edges, {} communities, {} cycles",
            project.name,
            stats.node_count,
            stats.edge_count,
            stats.community_count,
            stats.cycle_count,
        );
        engines.insert(project.name.clone(), engine);
    }

    let default_project = project_names[0].clone();
    eprintln!(
        "graphify-mcp: ready ({} project(s), default='{}')",
        engines.len(),
        default_project,
    );

    // Create MCP server and run on stdio.
    let server = GraphifyServer::new(engines, default_project, project_names);

    let transport = rmcp::transport::io::stdio();
    let service = rmcp::serve_server(server, transport)
        .await
        .expect("failed to start MCP server");
    service.waiting().await.expect("MCP server error");
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
// Parse language strings
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
// Extraction pipeline (duplicated from graphify-cli)
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
            "Warning: project '{}' discovered only {} file(s). Check repo path ('{}') and local_prefix ('{}').",
            project.name,
            files.len(),
            project.repo,
            local_prefix,
        );
    }

    if !local_prefix.is_empty() {
        let prefix_dir = repo_path.join(local_prefix);
        if !prefix_dir.is_dir() {
            eprintln!(
                "Warning: project '{}' has local_prefix '{}' but directory '{}' does not exist.",
                project.name,
                local_prefix,
                prefix_dir.display(),
            );
        }
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
// Analysis helpers
// ---------------------------------------------------------------------------

fn assign_community_ids(
    metrics: &mut [NodeMetrics],
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

fn build_query_engine(project: &ProjectConfig, settings: &Settings) -> QueryEngine {
    let graph = run_extract(project, settings);
    let w = resolve_weights(settings);
    let metrics_vec = compute_metrics(&graph, &w);
    let communities = detect_communities(&graph);
    let sccs = find_sccs(&graph);

    let mut metrics = metrics_vec;
    assign_community_ids(&mut metrics, &communities);

    QueryEngine::from_analyzed(graph, metrics, communities, sccs)
}
