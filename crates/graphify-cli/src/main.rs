use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde::Deserialize;

use graphify_core::{
    community::detect_communities,
    cycles::{find_sccs, find_simple_cycles},
    graph::CodeGraph,
    metrics::{compute_metrics, ScoringWeights},
    types::Language,
};
use graphify_extract::{
    walker::discover_files, ExtractionResult, LanguageExtractor, PythonExtractor,
    TypeScriptExtractor,
};
use graphify_report::{
    write_analysis_json, write_edges_csv, write_graph_json, write_nodes_csv, write_report, Cycle,
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
                write_analysis_json(&metrics, &communities, &cycles_for_report, &proj_out.join("analysis.json"));
                write_nodes_csv(&metrics, &proj_out.join("graph_nodes.csv"));
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
            let mut project_names: Vec<String> = Vec::new();
            for project in &cfg.project {
                project_names.push(project.name.clone());
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
            }
            if project_names.len() > 1 {
                write_summary(&project_names, &out_dir);
            }
        }

        Commands::Run { config, output } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, None);
            let formats = resolve_formats(&cfg, None);
            let mut project_names: Vec<String> = Vec::new();
            for project in &cfg.project {
                project_names.push(project.name.clone());
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
            }
            if project_names.len() > 1 {
                write_summary(&project_names, &out_dir);
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
# format = ["json", "csv", "md"]    # output formats

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

    // Extract each file and collect raw results.
    let mut all_nodes = Vec::new();
    let mut all_raw_edges: Vec<(String, String, graphify_core::types::Edge)> = Vec::new();

    for file in &files {
        let source = match std::fs::read(&file.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("Warning: cannot read {:?}: {e}", file.path);
                continue;
            }
        };

        let extractor: &dyn LanguageExtractor = match file.language {
            Language::Python => &python_extractor,
            Language::TypeScript => &typescript_extractor,
        };

        let result: ExtractionResult =
            extractor.extract_file(&file.path, &source, &file.module_name);

        all_nodes.extend(result.nodes);
        all_raw_edges.extend(result.edges);
    }

    // Build graph: add all nodes first.
    let mut graph = CodeGraph::new();
    for node in all_nodes {
        graph.add_node(node);
    }

    // Resolve edges and add them.
    for (src_id, raw_target, edge) in all_raw_edges {
        let (resolved_target, _is_local) = resolver.resolve(&raw_target, &src_id);
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
                write_analysis_json(metrics, communities, cycles, &out_dir.join("analysis.json"));
            }
            "csv" => {
                write_nodes_csv(metrics, &out_dir.join("graph_nodes.csv"));
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
            other => {
                eprintln!("Warning: unknown format '{other}', skipping.");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-project summary
// ---------------------------------------------------------------------------

fn write_summary(project_names: &[String], out_dir: &Path) {
    let summary = serde_json::json!({
        "projects": project_names,
        "count": project_names.len(),
    });
    let path = out_dir.join("graphify-summary.json");
    let text = serde_json::to_string_pretty(&summary).expect("serialize summary");
    std::fs::write(&path, text).expect("write graphify-summary.json");
    println!("Summary written to {}", path.display());
}
