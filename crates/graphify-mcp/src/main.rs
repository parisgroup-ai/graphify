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
    walker::discover_files, ExternalStubs, ExtractionResult, GoExtractor, LanguageExtractor,
    PhpExtractor, PythonExtractor, RustExtractor, TypeScriptExtractor,
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
struct Settings {
    #[allow(dead_code)]
    output: Option<String>,
    weights: Option<Vec<f64>>,
    exclude: Option<Vec<String>>,
    #[allow(dead_code)]
    format: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ProjectConfig {
    name: String,
    repo: String,
    lang: Vec<String>,
    local_prefix: Option<String>,
    #[serde(default)]
    external_stubs: Vec<String>,
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
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            build_query_engine(project, &config.settings)
        }));
        match result {
            Ok(engine) => {
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
            Err(_) => {
                eprintln!(
                    "  Warning: extraction failed for project '{}', skipping.",
                    project.name
                );
            }
        }
    }

    if engines.is_empty() {
        eprintln!("Error: all project extractions failed. Nothing to serve.");
        std::process::exit(1);
    }

    let default_project = project_names
        .iter()
        .find(|n| engines.contains_key(n.as_str()))
        .cloned()
        .unwrap_or_else(|| engines.keys().next().unwrap().clone());
    eprintln!(
        "graphify-mcp: ready ({} project(s), default='{}')",
        engines.len(),
        default_project,
    );

    // Create MCP server and run on stdio.
    let server = GraphifyServer::new(engines, default_project);

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
            "go" => Some(Language::Go),
            "rust" | "rs" => Some(Language::Rust),
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

    let python_extractor = PythonExtractor::new();
    let typescript_extractor = TypeScriptExtractor::new();
    let go_extractor = GoExtractor::new();
    let rust_extractor = RustExtractor::new();
    let php_extractor = PhpExtractor::new();

    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    resolver.set_local_prefix(local_prefix);
    for file in &files {
        resolver.register_module(&file.module_name);
    }

    if languages.contains(&Language::TypeScript) {
        let tsconfig = repo_path.join("tsconfig.json");
        if tsconfig.exists() {
            resolver.load_tsconfig(&tsconfig);
        }
    }

    if languages.contains(&Language::Go) {
        let go_mod = repo_path.join("go.mod");
        if go_mod.exists() {
            resolver.load_go_mod(&go_mod);
        }
    }

    // Tuple `(module_name, result)` keeps the source module attached so the
    // sequential merge loop can register per-file artifacts (FEAT-031
    // `use_aliases`) against the resolver.
    let results: Vec<(String, ExtractionResult)> = files
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
                Language::Go => &go_extractor,
                Language::Rust => &rust_extractor,
                Language::Php => &php_extractor,
            };

            Some((
                file.module_name.clone(),
                extractor.extract_file(&file.path, &source, &file.module_name),
            ))
        })
        .collect();

    let mut all_nodes = Vec::new();
    let mut all_raw_edges: Vec<(String, String, graphify_core::types::Edge)> = Vec::new();
    for (module_name, result) in results {
        if !result.use_aliases.is_empty() {
            resolver.register_use_aliases(&module_name, &result.use_aliases);
        }
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

    let external_stubs = ExternalStubs::new(project.external_stubs.iter().cloned());

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

        // Step 3: Downgrade edges to non-local targets — unless target matches
        // an `external_stubs` prefix (issue #12).
        if !is_local {
            let capped = edge.confidence.min(0.5);
            let kind = if external_stubs.matches(&resolved_target) {
                graphify_core::types::ConfidenceKind::ExpectedExternal
            } else {
                graphify_core::types::ConfidenceKind::Ambiguous
            };
            edge = edge.with_confidence(capped, kind);
        }

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
