use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use serde::Deserialize;

use graphify_core::{
    community::detect_communities,
    cycles::{find_sccs, find_simple_cycles},
    graph::CodeGraph,
    metrics::{compute_metrics, ScoringWeights},
    query::{QueryEngine, SearchFilters, SortField},
    types::Language,
};
use graphify_extract::{
    walker::discover_files, ExtractionResult, LanguageExtractor, PythonExtractor,
    TypeScriptExtractor,
};
use graphify_report::{
    write_analysis_json, write_edges_csv, write_graph_json, write_html, write_nodes_csv,
    write_report, Cycle,
};

// ---------------------------------------------------------------------------
// Config structs
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
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "graphify",
    about = "Architectural analysis of codebases via dependency graphs",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a graphify.toml template in the current directory
    Init,

    /// Extract dependency graph from source files (produces graph.json)
    Extract {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Analyze an existing graph (produces analysis.json, CSV files)
    Analyze {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Scoring weights as comma-separated floats: betweenness,pagerank,in_degree,in_cycle
        #[arg(long)]
        weights: Option<String>,
    },

    /// Generate Markdown report (produces architecture_report.md)
    Report {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Scoring weights as comma-separated floats: betweenness,pagerank,in_degree,in_cycle
        #[arg(long)]
        weights: Option<String>,

        /// Output formats: json,csv,md (comma-separated)
        #[arg(long)]
        format: Option<String>,
    },

    /// Run full pipeline: extract → analyze → report (alias for report)
    Run {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Search nodes by pattern (glob matching on node IDs)
    Query {
        /// Glob pattern to match node IDs (e.g. "app.services.*")
        pattern: String,

        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Filter by node kind: module, function, class, method
        #[arg(long)]
        kind: Option<String>,

        /// Sort results: score (default), name, in_degree
        #[arg(long, default_value = "score")]
        sort: String,

        /// Filter to a specific project
        #[arg(long)]
        project: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Explain a module: profile card + impact analysis
    Explain {
        /// Node ID to explain (e.g. "app.services.llm")
        node_id: String,

        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Filter to a specific project
        #[arg(long)]
        project: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Find dependency paths between two nodes
    Path {
        /// Source node ID
        source: String,
        /// Target node ID
        target: String,

        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Show all paths (default: shortest only)
        #[arg(long)]
        all: bool,

        /// Maximum path depth for --all (default: 10)
        #[arg(long, default_value = "10")]
        max_depth: usize,

        /// Maximum number of paths for --all (default: 20)
        #[arg(long, default_value = "20")]
        max_paths: usize,

        /// Filter to a specific project
        #[arg(long)]
        project: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cmd_init(),

        Commands::Extract { config, output } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            for project in &cfg.project {
                let (graph, _excludes) = run_extract(project, &cfg.settings);
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                write_graph_json(&graph, &proj_out.join("graph.json"));
                println!(
                    "[{}] Extracted {} nodes, {} edges → {}",
                    project.name,
                    graph.node_count(),
                    graph.edge_count(),
                    proj_out.join("graph.json").display()
                );
            }
        }

        Commands::Analyze { config, output, weights } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            for project in &cfg.project {
                let (graph, _) = run_extract(project, &cfg.settings);
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (mut metrics, communities, cycles_simple) = run_analyze(&graph, &w);
                assign_community_ids(&mut metrics, &communities);
                let cycles_for_report: Vec<Cycle> = cycles_simple;
                write_analysis_json(&metrics, &communities, &cycles_for_report, graph.edge_count(), &proj_out.join("analysis.json"));
                write_nodes_csv(&metrics, &graph, &proj_out.join("graph_nodes.csv"));
                write_edges_csv(&graph, &proj_out.join("graph_edges.csv"));
                println!(
                    "[{}] Analyzed {} nodes, {} communities, {} cycles",
                    project.name,
                    metrics.len(),
                    communities.len(),
                    cycles_for_report.len()
                );
            }
        }

        Commands::Report { config, output, weights, format } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            let formats = resolve_formats(&cfg, format.as_deref());
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let (graph, _) = run_extract(project, &cfg.settings);
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (mut metrics, communities, cycles_simple) = run_analyze(&graph, &w);
                assign_community_ids(&mut metrics, &communities);
                let cycles_for_report: Vec<Cycle> = cycles_simple;
                write_all_outputs(
                    &project.name,
                    &graph,
                    &metrics,
                    &communities,
                    &cycles_for_report,
                    &proj_out,
                    &formats,
                );
                println!(
                    "[{}] Report written to {}",
                    project.name,
                    proj_out.display()
                );
                project_data.push(ProjectData {
                    name: project.name.clone(),
                    graph,
                    metrics,
                    cycles: cycles_for_report,
                });
            }
            if project_data.len() > 1 {
                write_summary(&project_data, &out_dir);
            }
        }

        Commands::Run { config, output } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, None);
            let formats = resolve_formats(&cfg, None);
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let (graph, _) = run_extract(project, &cfg.settings);
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (mut metrics, communities, cycles_simple) = run_analyze(&graph, &w);
                assign_community_ids(&mut metrics, &communities);
                let cycles_for_report: Vec<Cycle> = cycles_simple;
                write_all_outputs(
                    &project.name,
                    &graph,
                    &metrics,
                    &communities,
                    &cycles_for_report,
                    &proj_out,
                    &formats,
                );
                println!(
                    "[{}] Pipeline complete → {}",
                    project.name,
                    proj_out.display()
                );
                project_data.push(ProjectData {
                    name: project.name.clone(),
                    graph,
                    metrics,
                    cycles: cycles_for_report,
                });
            }
            if project_data.len() > 1 {
                write_summary(&project_data, &out_dir);
            }
        }

        Commands::Query { pattern, config, kind, sort, project, json } => {
            let cfg = load_config(&config);
            let projects = filter_projects(&cfg, project.as_deref());
            let multi_project = cfg.project.len() > 1;

            let sort_field = match sort.to_lowercase().as_str() {
                "name" => SortField::Name,
                "in_degree" | "indegree" => SortField::InDegree,
                _ => SortField::Score,
            };

            let filters = SearchFilters {
                kind: kind.as_deref().and_then(parse_node_kind),
                sort_by: sort_field,
                local_only: false,
            };

            let mut all_results: Vec<(String, Vec<graphify_core::query::QueryMatch>)> = Vec::new();

            for proj in &projects {
                let engine = build_query_engine(proj, &cfg.settings);
                let results = engine.search(&pattern, &filters);
                if !results.is_empty() {
                    all_results.push((proj.name.clone(), results));
                }
            }

            if json {
                let json_output: Vec<serde_json::Value> = all_results
                    .iter()
                    .flat_map(|(proj_name, results)| {
                        results.iter().map(move |r| {
                            let mut val = serde_json::to_value(r).unwrap();
                            if multi_project {
                                val.as_object_mut().unwrap().insert(
                                    "project".to_string(),
                                    serde_json::Value::String(proj_name.clone()),
                                );
                            }
                            val
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
            } else {
                let total: usize = all_results.iter().map(|(_, r)| r.len()).sum();
                if total == 0 {
                    println!("No matches for pattern '{}'.", pattern);
                } else {
                    println!("Found {} match(es) for '{}':", total, pattern);
                    for (proj_name, results) in &all_results {
                        for r in results {
                            if multi_project {
                                println!(
                                    "  [{}] {} ({:?}) score={:.3} community={} cycle={}",
                                    proj_name, r.node_id, r.kind, r.score, r.community_id, r.in_cycle
                                );
                            } else {
                                println!(
                                    "  {} ({:?}) score={:.3} community={} cycle={}",
                                    r.node_id, r.kind, r.score, r.community_id, r.in_cycle
                                );
                            }
                        }
                    }
                }
            }
        }

        Commands::Explain { node_id, config, project, json } => {
            let cfg = load_config(&config);
            let projects = filter_projects(&cfg, project.as_deref());
            let multi_project = cfg.project.len() > 1;
            let mut found = false;

            for proj in &projects {
                let engine = build_query_engine(proj, &cfg.settings);
                if let Some(report) = engine.explain(&node_id) {
                    found = true;
                    if json {
                        let mut val = serde_json::to_value(&report).unwrap();
                        if multi_project {
                            val.as_object_mut().unwrap().insert(
                                "project".to_string(),
                                serde_json::Value::String(proj.name.clone()),
                            );
                        }
                        println!("{}", serde_json::to_string_pretty(&val).unwrap());
                    } else {
                        print_explain_report(&report, &proj.name, multi_project);
                    }
                    break;
                }
            }

            if !found {
                eprintln!("Node '{}' not found.", node_id);
                // Try suggest across all projects
                for proj in &projects {
                    let engine = build_query_engine(proj, &cfg.settings);
                    let suggestions = engine.suggest(&node_id);
                    if !suggestions.is_empty() {
                        eprintln!("Did you mean?");
                        for s in &suggestions {
                            eprintln!("  {}", s);
                        }
                        break;
                    }
                }
                std::process::exit(1);
            }
        }

        Commands::Path { source, target, config, all, max_depth, max_paths, project, json } => {
            let cfg = load_config(&config);
            let projects = filter_projects(&cfg, project.as_deref());
            let multi_project = cfg.project.len() > 1;
            let mut found = false;

            for proj in &projects {
                let engine = build_query_engine(proj, &cfg.settings);

                if all {
                    let paths = engine.all_paths(&source, &target, max_depth, max_paths);
                    if !paths.is_empty() {
                        found = true;
                        if json {
                            let json_output = serde_json::json!({
                                "source": source,
                                "target": target,
                                "project": if multi_project { Some(&proj.name) } else { None },
                                "path_count": paths.len(),
                                "paths": paths,
                            });
                            println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                        } else {
                            if multi_project {
                                println!("[{}] {} path(s) from '{}' to '{}':", proj.name, paths.len(), source, target);
                            } else {
                                println!("{} path(s) from '{}' to '{}':", paths.len(), source, target);
                            }
                            for (i, path) in paths.iter().enumerate() {
                                print!("  {}. ", i + 1);
                                print_path(path);
                            }
                        }
                        break;
                    }
                } else {
                    if let Some(path) = engine.shortest_path(&source, &target) {
                        found = true;
                        if json {
                            let json_output = serde_json::json!({
                                "source": source,
                                "target": target,
                                "project": if multi_project { Some(&proj.name) } else { None },
                                "hops": path.len().saturating_sub(1),
                                "path": path,
                            });
                            println!("{}", serde_json::to_string_pretty(&json_output).unwrap());
                        } else {
                            if multi_project {
                                print!("[{}] ", proj.name);
                            }
                            print_path(&path);
                        }
                        break;
                    }
                }
            }

            if !found {
                eprintln!("No path found from '{}' to '{}'.", source, target);
                std::process::exit(1);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// init command
// ---------------------------------------------------------------------------

fn cmd_init() {
    let template = r#"# graphify.toml — generated by `graphify init`

[settings]
output = "./report"
# weights = [0.4, 0.2, 0.2, 0.2]   # betweenness, pagerank, in_degree, in_cycle
# exclude = []                       # extra directories to skip
# format = ["json", "csv", "md", "html"]    # output formats

[[project]]
name = "my-project"
repo = "./src"
lang = ["python"]
local_prefix = "app"
"#;

    let dest = Path::new("graphify.toml");
    if dest.exists() {
        eprintln!("graphify.toml already exists — not overwriting.");
        std::process::exit(1);
    }
    std::fs::write(dest, template).expect("write graphify.toml");
    println!("Created graphify.toml — edit it to point at your repo.");
}

// ---------------------------------------------------------------------------
// Config loading helpers
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

fn resolve_output(cfg: &Config, override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    cfg.settings
        .output
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./report"))
}

fn resolve_weights(cfg: &Config, override_str: Option<&str>) -> ScoringWeights {
    // CLI --weights flag takes priority, then config [settings] weights.
    let vec: Option<Vec<f64>> = if let Some(s) = override_str {
        let parsed: Vec<f64> = s
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        if parsed.len() == 4 {
            Some(parsed)
        } else {
            eprintln!("Warning: --weights must be 4 comma-separated floats; using defaults.");
            None
        }
    } else {
        cfg.settings.weights.clone()
    };

    if let Some(v) = vec {
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

fn resolve_formats(cfg: &Config, override_str: Option<&str>) -> Vec<String> {
    if let Some(s) = override_str {
        return s.split(',').map(|f| f.trim().to_lowercase()).collect();
    }
    cfg.settings
        .format
        .clone()
        .unwrap_or_else(|| vec!["json".to_string(), "csv".to_string(), "md".to_string()])
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
// Extraction pipeline
// ---------------------------------------------------------------------------

fn run_extract(project: &ProjectConfig, settings: &Settings) -> (CodeGraph, Vec<String>) {
    let repo_path = PathBuf::from(&project.repo);
    let languages = parse_languages(&project.lang);
    let local_prefix = project.local_prefix.as_deref().unwrap_or("");

    // Build extra excludes as Vec<&str> slices.
    let extra_owned: Vec<String> = settings.exclude.clone().unwrap_or_default();
    let extra_excludes: Vec<&str> = extra_owned.iter().map(|s| s.as_str()).collect();

    // Discover files.
    let files = discover_files(&repo_path, &languages, local_prefix, &extra_excludes);

    // BUG-009: Warn when discovery finds very few files — likely misconfigured
    // repo path or local_prefix.
    if files.len() <= 1 {
        eprintln!(
            "Warning: project '{}' discovered only {} file(s). Check repo path ('{}') and local_prefix ('{}') configuration.",
            project.name,
            files.len(),
            project.repo,
            local_prefix,
        );
    }

    // Also warn if local_prefix looks like a directory but doesn't exist inside repo.
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

    // Build extractors.
    let python_extractor = PythonExtractor::new();
    let typescript_extractor = TypeScriptExtractor::new();

    // Build resolver.
    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    for file in &files {
        resolver.register_module(&file.module_name);
    }

    // Load tsconfig if TypeScript is in the language list.
    if languages.contains(&Language::TypeScript) {
        let tsconfig = repo_path.join("tsconfig.json");
        if tsconfig.exists() {
            resolver.load_tsconfig(&tsconfig);
        }
    }

    // Extract each file in parallel via rayon, then collect results.
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

    // Merge results sequentially into graph.
    let mut all_nodes = Vec::new();
    let mut all_raw_edges: Vec<(String, String, graphify_core::types::Edge)> = Vec::new();
    for result in results {
        all_nodes.extend(result.nodes);
        all_raw_edges.extend(result.edges);
    }

    // Build graph: add all nodes first.
    let mut graph = CodeGraph::new();

    // Set the default language for placeholder nodes so that unresolved
    // imports are tagged with the project's language instead of always
    // defaulting to Python.
    if let Some(lang) = languages.first() {
        graph.set_default_language(lang.clone());
    }

    for node in all_nodes {
        graph.add_node(node);
    }

    // Build a set of module names that are package entry points (__init__.py,
    // index.ts), so the resolver knows not to pop the leaf for relative imports.
    let package_modules: HashSet<&str> = files
        .iter()
        .filter(|f| f.is_package)
        .map(|f| f.module_name.as_str())
        .collect();

    // Resolve edges and add them.
    for (src_id, raw_target, edge) in all_raw_edges {
        let is_package = package_modules.contains(src_id.as_str());
        let (resolved_target, _is_local) = resolver.resolve(&raw_target, &src_id, is_package);
        graph.add_edge(&src_id, &resolved_target, edge);
    }

    (graph, extra_owned)
}

// ---------------------------------------------------------------------------
// Analysis pipeline
// ---------------------------------------------------------------------------

type AnalysisResult = (
    Vec<graphify_core::metrics::NodeMetrics>,
    Vec<graphify_core::community::Community>,
    Vec<Cycle>,
);

fn run_analyze(graph: &CodeGraph, weights: &ScoringWeights) -> AnalysisResult {
    let metrics = compute_metrics(graph, weights);
    let communities = detect_communities(graph);
    let sccs = find_sccs(graph);

    // Build simple cycles from SCCs (capped at 500).
    let simple_cycles = find_simple_cycles(graph, 500);

    // Convert to Cycle (Vec<String>) — already the right type.
    // Also include SCC node_ids as cycles for completeness when simple_cycles is empty.
    let cycles: Vec<Cycle> = if !simple_cycles.is_empty() {
        simple_cycles
    } else {
        sccs.into_iter().map(|g| g.node_ids).collect()
    };

    (metrics, communities, cycles)
}

// ---------------------------------------------------------------------------
// Query engine helpers
// ---------------------------------------------------------------------------

fn build_query_engine(project: &ProjectConfig, settings: &Settings) -> QueryEngine {
    let (graph, _) = run_extract(project, settings);
    let w = ScoringWeights::default();
    let (mut metrics, communities, _cycles_simple) = run_analyze(&graph, &w);
    assign_community_ids(&mut metrics, &communities);
    let cycles = find_sccs(&graph);
    QueryEngine::from_analyzed(graph, metrics, communities, cycles)
}

fn filter_projects<'a>(cfg: &'a Config, project_name: Option<&str>) -> Vec<&'a ProjectConfig> {
    if let Some(name) = project_name {
        let matched: Vec<&ProjectConfig> = cfg.project.iter().filter(|p| p.name == name).collect();
        if matched.is_empty() {
            eprintln!("Project '{}' not found in config.", name);
            std::process::exit(1);
        }
        matched
    } else {
        cfg.project.iter().collect()
    }
}

fn parse_node_kind(s: &str) -> Option<graphify_core::types::NodeKind> {
    match s.to_lowercase().as_str() {
        "module" | "mod" => Some(graphify_core::types::NodeKind::Module),
        "function" | "func" | "fn" => Some(graphify_core::types::NodeKind::Function),
        "class" => Some(graphify_core::types::NodeKind::Class),
        "method" => Some(graphify_core::types::NodeKind::Method),
        _ => {
            eprintln!("Warning: unknown kind '{}', ignoring filter.", s);
            None
        }
    }
}

fn print_explain_report(report: &graphify_core::query::ExplainReport, project_name: &str, multi_project: bool) {
    println!();
    println!("═══ {} ═══", report.node_id);
    if multi_project {
        println!("  Project:     {}", project_name);
    }
    println!("  Kind:        {:?}", report.kind);
    println!("  File:        {}", report.file_path.display());
    println!("  Language:    {:?}", report.language);
    println!("  Community:   {}", report.community_id);
    if report.in_cycle {
        println!("  In cycle:    yes (with: {})", report.cycle_peers.join(", "));
    } else {
        println!("  In cycle:    no");
    }

    println!();
    println!("  ── Metrics ──");
    println!("  Score:         {:.3}", report.metrics.score);
    println!("  Betweenness:   {:.3}", report.metrics.betweenness);
    println!("  PageRank:      {:.4}", report.metrics.pagerank);
    println!("  In-degree:     {}", report.metrics.in_degree);
    println!("  Out-degree:    {}", report.metrics.out_degree);

    println!();
    println!("  ── Dependencies ({}) ──", report.direct_dependencies.len());
    for dep in &report.direct_dependencies {
        println!("  → {}", dep);
    }

    println!();
    println!("  ── Dependents ({}) ──", report.direct_dependents.len());
    let max_show = 5;
    for dep in report.direct_dependents.iter().take(max_show) {
        println!("  ← {}", dep);
    }
    if report.direct_dependents.len() > max_show {
        println!("  ... and {} more", report.direct_dependents.len() - max_show);
    }

    println!();
    println!("  ── Impact ──");
    println!("  Transitive dependents: {} modules", report.transitive_dependent_count);
    println!();
}

fn print_path(path: &[graphify_core::query::PathStep]) {
    for (i, step) in path.iter().enumerate() {
        if i > 0 {
            if let Some(ref kind) = path[i - 1].edge_kind {
                print!(" ─[{:?}]→ ", kind);
            } else {
                print!(" → ");
            }
        }
        print!("{}", step.node_id);
    }
    println!();
}

// ---------------------------------------------------------------------------
// Assign community IDs back to NodeMetrics
// ---------------------------------------------------------------------------

fn assign_community_ids(
    metrics: &mut Vec<graphify_core::metrics::NodeMetrics>,
    communities: &[graphify_core::community::Community],
) {
    // Build a reverse map: node_id → community_id.
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

// ---------------------------------------------------------------------------
// Write outputs based on format list
// ---------------------------------------------------------------------------

fn write_all_outputs(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[graphify_core::metrics::NodeMetrics],
    communities: &[graphify_core::community::Community],
    cycles: &[Cycle],
    out_dir: &Path,
    formats: &[String],
) {
    for fmt in formats {
        match fmt.as_str() {
            "json" => {
                write_graph_json(graph, &out_dir.join("graph.json"));
                write_analysis_json(metrics, communities, cycles, graph.edge_count(), &out_dir.join("analysis.json"));
            }
            "csv" => {
                write_nodes_csv(metrics, graph, &out_dir.join("graph_nodes.csv"));
                write_edges_csv(graph, &out_dir.join("graph_edges.csv"));
            }
            "md" | "markdown" => {
                write_report(
                    project_name,
                    metrics,
                    communities,
                    cycles,
                    &out_dir.join("architecture_report.md"),
                );
            }
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
            other => {
                eprintln!("Warning: unknown format '{other}', skipping.");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-project summary
// ---------------------------------------------------------------------------

/// Aggregated per-project data used by the cross-project summary.
struct ProjectData {
    name: String,
    graph: CodeGraph,
    metrics: Vec<graphify_core::metrics::NodeMetrics>,
    cycles: Vec<Cycle>,
}

/// Write a cross-project summary with aggregate metrics, coupling data,
/// cycle counts, and top hotspots across all projects.
fn write_summary(projects: &[ProjectData], out_dir: &Path) {
    let project_names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();

    // --- Per-project stats ---------------------------------------------------
    let per_project: Vec<serde_json::Value> = projects
        .iter()
        .map(|p| {
            let node_count = p.graph.node_count();
            let edge_count = p.graph.edge_count();
            let cycle_count = p.cycles.len();
            // Include the top hotspot (highest-scoring node) per project.
            let top_hotspot = p.metrics.iter()
                .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
                .map(|m| serde_json::json!({
                    "id": m.id,
                    "score": (m.score * 1000.0).round() / 1000.0,
                }));
            serde_json::json!({
                "name": p.name,
                "nodes": node_count,
                "edges": edge_count,
                "cycles": cycle_count,
                "top_hotspot": top_hotspot,
            })
        })
        .collect();

    // --- Aggregate totals ----------------------------------------------------
    let total_nodes: usize = projects.iter().map(|p| p.graph.node_count()).sum();
    let total_edges: usize = projects.iter().map(|p| p.graph.edge_count()).sum();
    let total_cycles: usize = projects.iter().map(|p| p.cycles.len()).sum();

    // --- Top hotspots across all projects (top 10 by score) ------------------
    let mut all_hotspots: Vec<(&str, &graphify_core::metrics::NodeMetrics)> = projects
        .iter()
        .flat_map(|p| p.metrics.iter().map(move |m| (p.name.as_str(), m)))
        .collect();
    all_hotspots.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap_or(std::cmp::Ordering::Equal));
    all_hotspots.truncate(10);
    let top_hotspots: Vec<serde_json::Value> = all_hotspots
        .iter()
        .map(|(proj, m)| {
            serde_json::json!({
                "id": m.id,
                "project": proj,
                "score": (m.score * 1000.0).round() / 1000.0,
                "betweenness": (m.betweenness * 1000.0).round() / 1000.0,
                "pagerank": (m.pagerank * 10000.0).round() / 10000.0,
                "in_degree": m.in_degree,
                "in_cycle": m.in_cycle,
            })
        })
        .collect();

    // --- Node ownership map --------------------------------------------------
    let mut node_owners: HashMap<String, HashSet<String>> = HashMap::new();
    for p in projects {
        for id in p.graph.node_ids() {
            node_owners
                .entry(id.to_string())
                .or_default()
                .insert(p.name.clone());
        }
    }

    // --- Cross-project coupling (aggregate counts only, no full edge list) ---
    struct CouplingStats {
        edge_count: usize,
        imports: usize,
        defines: usize,
        calls: usize,
        shared_modules: HashSet<String>,
    }

    let mut cross_deps: HashMap<(String, String), CouplingStats> = HashMap::new();

    for p in projects {
        for (_src_id, tgt_id, edge) in p.graph.edges() {
            if let Some(owners) = node_owners.get(tgt_id) {
                for owner in owners {
                    if owner != &p.name {
                        let stats = cross_deps
                            .entry((p.name.clone(), owner.clone()))
                            .or_insert_with(|| CouplingStats {
                                edge_count: 0,
                                imports: 0,
                                defines: 0,
                                calls: 0,
                                shared_modules: HashSet::new(),
                            });
                        stats.edge_count += 1;
                        match edge.kind {
                            graphify_core::types::EdgeKind::Imports => stats.imports += 1,
                            graphify_core::types::EdgeKind::Defines => stats.defines += 1,
                            graphify_core::types::EdgeKind::Calls => stats.calls += 1,
                        }
                        stats.shared_modules.insert(tgt_id.to_string());
                    }
                }
            }
        }
    }

    // Build the cross_dependencies array sorted by (from, to) for determinism.
    let mut dep_keys: Vec<(String, String)> = cross_deps.keys().cloned().collect();
    dep_keys.sort();
    let cross_dependencies: Vec<serde_json::Value> = dep_keys
        .into_iter()
        .map(|(from_proj, to_proj)| {
            let stats = cross_deps
                .remove(&(from_proj.clone(), to_proj.clone()))
                .unwrap();
            serde_json::json!({
                "from_project": from_proj,
                "to_project": to_proj,
                "edge_count": stats.edge_count,
                "shared_modules": stats.shared_modules.len(),
                "by_kind": {
                    "imports": stats.imports,
                    "defines": stats.defines,
                    "calls": stats.calls,
                },
            })
        })
        .collect();

    let total_cross_edges: usize = cross_dependencies
        .iter()
        .filter_map(|d| d.get("edge_count").and_then(|e| e.as_u64()).map(|n| n as usize))
        .sum();

    // --- Shared modules ------------------------------------------------------
    let mut shared_modules: Vec<serde_json::Value> = node_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(id, owners)| {
            let mut projs: Vec<&str> = owners.iter().map(|s| s.as_str()).collect();
            projs.sort();
            serde_json::json!({
                "module": id,
                "projects": projs,
            })
        })
        .collect();
    shared_modules.sort_by(|a, b| {
        let ma = a.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let mb = b.get("module").and_then(|v| v.as_str()).unwrap_or("");
        ma.cmp(mb)
    });

    // --- Assemble final JSON -------------------------------------------------
    let summary = serde_json::json!({
        "projects": per_project,
        "summary": {
            "total_projects": project_names.len(),
            "total_nodes": total_nodes,
            "total_edges": total_edges,
            "total_cycles": total_cycles,
            "total_cross_edges": total_cross_edges,
            "total_shared_modules": shared_modules.len(),
        },
        "top_hotspots": top_hotspots,
        "cross_dependencies": cross_dependencies,
        "shared_modules": shared_modules,
    });

    let path = out_dir.join("graphify-summary.json");
    let text = serde_json::to_string_pretty(&summary).expect("serialize summary");
    std::fs::write(&path, text).expect("write graphify-summary.json");
    println!("Summary written to {}", path.display());
}
