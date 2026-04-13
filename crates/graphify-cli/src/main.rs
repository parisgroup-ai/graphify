use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use graphify_core::diff::{compute_diff, AnalysisSnapshot};
use graphify_core::{
    community::detect_communities,
    cycles::{find_sccs, find_simple_cycles},
    graph::CodeGraph,
    history::{build_historical_snapshot, compute_trend_report, load_historical_snapshots},
    metrics::{compute_metrics, ScoringWeights},
    policy::{CompiledPolicy, PolicyConfig, ProjectGraph, ProjectPolicyResult},
    query::{QueryEngine, SearchFilters, SortField},
    types::Language,
};
use graphify_extract::{
    cache::{sha256_hex, CacheStats, ExtractionCache},
    walker::{detect_local_prefix, discover_files},
    ExtractionResult, GoExtractor, LanguageExtractor, PythonExtractor, RustExtractor,
    TypeScriptExtractor,
};
use graphify_report::{
    write_analysis_json, write_cypher, write_diff_json, write_diff_markdown, write_edges_csv,
    write_graph_json, write_graphml, write_html, write_nodes_csv, write_obsidian_vault,
    write_report, write_trend_json, write_trend_markdown, Cycle,
};

mod watch;

// ---------------------------------------------------------------------------
// Config structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    project: Vec<ProjectConfig>,
    #[serde(default)]
    policy: PolicyConfig,
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

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
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

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
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

        /// Output formats: json,csv,md,html,neo4j,graphml,obsidian (comma-separated)
        #[arg(long)]
        format: Option<String>,

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
    },

    /// Run full pipeline: extract → analyze → report (alias for report)
    Run {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
    },

    /// Check architectural quality gates for CI
    Check {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Maximum allowed cycle count
        #[arg(long)]
        max_cycles: Option<usize>,

        /// Maximum allowed hotspot score
        #[arg(long)]
        max_hotspot_score: Option<f64>,

        /// Filter to a specific project
        #[arg(long)]
        project: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
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

    /// Interactive shell for exploring the dependency graph
    Shell {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Specific project to load (loads all if omitted)
        #[arg(long)]
        project: Option<String>,
    },

    /// Watch source files and auto-rebuild on changes
    Watch {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Force full rebuild on first run, ignoring extraction cache
        #[arg(long)]
        force: bool,

        /// Output formats: json,csv,md,html,neo4j,graphml,obsidian (comma-separated)
        #[arg(long)]
        format: Option<String>,
    },

    /// Compare two analysis snapshots to detect architectural drift
    Diff {
        /// Path to the "before" analysis.json (file-vs-file mode)
        #[arg(long)]
        before: Option<PathBuf>,

        /// Path to the "after" analysis.json (file-vs-file mode)
        #[arg(long)]
        after: Option<PathBuf>,

        /// Path to a baseline analysis.json (baseline-vs-live mode)
        #[arg(long)]
        baseline: Option<PathBuf>,

        /// Path to graphify.toml (for live extraction in baseline mode)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Project name (for baseline mode with multi-project configs)
        #[arg(long)]
        project: Option<String>,

        /// Output directory for drift report files (default: current directory)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Minimum score delta to report as significant (default: 0.05)
        #[arg(long, default_value = "0.05")]
        threshold: f64,
    },

    /// Aggregate historical architecture trends from stored snapshots
    Trend {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Project name (required for multi-project configs)
        #[arg(long)]
        project: Option<String>,

        /// Output directory for trend report files
        #[arg(long)]
        output: Option<PathBuf>,

        /// Limit trend aggregation to the most recent N snapshots
        #[arg(long)]
        limit: Option<usize>,

        /// Output the trend report as JSON on stdout
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

        Commands::Extract {
            config,
            output,
            force,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (graph, _excludes, stats) =
                    run_extract(project, &cfg.settings, Some(&proj_out), force);
                print_cache_stats(&project.name, &stats);
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

        Commands::Analyze {
            config,
            output,
            weights,
            force,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (graph, _, stats) = run_extract(project, &cfg.settings, Some(&proj_out), force);
                print_cache_stats(&project.name, &stats);
                let (mut metrics, communities, cycles_simple) = run_analyze(&graph, &w);
                assign_community_ids(&mut metrics, &communities);
                let cycles_for_report: Vec<Cycle> = cycles_simple;
                write_analysis_json(
                    &metrics,
                    &communities,
                    &cycles_for_report,
                    &graph,
                    &proj_out.join("analysis.json"),
                );
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

        Commands::Report {
            config,
            output,
            weights,
            format,
            force,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            let formats = resolve_formats(&cfg, format.as_deref());
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let pd = run_pipeline_for_project(
                    project,
                    &cfg.settings,
                    &proj_out,
                    &w,
                    &formats,
                    force,
                );
                println!(
                    "[{}] Report written to {}",
                    project.name,
                    proj_out.display()
                );
                project_data.push(pd);
            }
            prune_stale_project_dirs(&out_dir, &cfg.project);
            if project_data.len() > 1 {
                write_summary(&project_data, &out_dir);
            }
        }

        Commands::Run {
            config,
            output,
            force,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, None);
            let formats = resolve_formats(&cfg, None);
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let pd = run_pipeline_for_project(
                    project,
                    &cfg.settings,
                    &proj_out,
                    &w,
                    &formats,
                    force,
                );
                println!(
                    "[{}] Pipeline complete → {}",
                    project.name,
                    proj_out.display()
                );
                project_data.push(pd);
            }
            prune_stale_project_dirs(&out_dir, &cfg.project);
            if project_data.len() > 1 {
                write_summary(&project_data, &out_dir);
            }
        }

        Commands::Check {
            config,
            max_cycles,
            max_hotspot_score,
            project,
            json,
            force,
        } => {
            cmd_check(
                &config,
                project.as_deref(),
                force,
                CheckLimits {
                    max_cycles,
                    max_hotspot_score,
                },
                json,
            );
        }

        Commands::Query {
            pattern,
            config,
            kind,
            sort,
            project,
            json,
        } => {
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
                min_confidence: None,
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
                                    proj_name,
                                    r.node_id,
                                    r.kind,
                                    r.score,
                                    r.community_id,
                                    r.in_cycle
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

        Commands::Explain {
            node_id,
            config,
            project,
            json,
        } => {
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

        Commands::Path {
            source,
            target,
            config,
            all,
            max_depth,
            max_paths,
            project,
            json,
        } => {
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
                                println!(
                                    "[{}] {} path(s) from '{}' to '{}':",
                                    proj.name,
                                    paths.len(),
                                    source,
                                    target
                                );
                            } else {
                                println!(
                                    "{} path(s) from '{}' to '{}':",
                                    paths.len(),
                                    source,
                                    target
                                );
                            }
                            for (i, path) in paths.iter().enumerate() {
                                print!("  {}. ", i + 1);
                                print_path(path);
                            }
                        }
                        break;
                    }
                } else if let Some(path) = engine.shortest_path(&source, &target) {
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

            if !found {
                eprintln!("No path found from '{}' to '{}'.", source, target);
                std::process::exit(1);
            }
        }

        Commands::Shell { config, project } => {
            cmd_shell(&config, project.as_deref());
        }

        Commands::Watch {
            config,
            output,
            force,
            format,
        } => {
            cmd_watch(&config, output.as_deref(), force, format.as_deref());
        }

        Commands::Diff {
            before,
            after,
            baseline,
            config,
            project,
            output,
            threshold,
        } => {
            cmd_diff(
                before.as_deref(),
                after.as_deref(),
                baseline.as_deref(),
                config.as_deref(),
                project.as_deref(),
                output.as_deref(),
                threshold,
            );
        }

        Commands::Trend {
            config,
            project,
            output,
            limit,
            json,
        } => {
            cmd_trend(&config, project.as_deref(), output.as_deref(), limit, json);
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
# format = ["json", "csv", "md", "html"]    # output formats (also: neo4j, graphml, obsidian)

[[project]]
name = "my-project"
repo = "./src"
lang = ["python"]           # Options: python, typescript, go, rust
local_prefix = "app"

# Optional policy rules for graphify check:
#
# [[policy.group]]
# name = "feature"
# match = ["src.features.*"]
# partition_by = "segment:2"
#
# [[policy.rule]]
# name = "no-cross-feature-imports"
# kind = "deny"
# from = ["group:feature"]
# to = ["group:feature"]
# allow_same_partition = true
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
// diff command
// ---------------------------------------------------------------------------

fn cmd_diff(
    before: Option<&Path>,
    after: Option<&Path>,
    baseline: Option<&Path>,
    config: Option<&Path>,
    project: Option<&str>,
    output: Option<&Path>,
    threshold: f64,
) {
    let (before_snapshot, after_snapshot) = match (before, after, baseline, config) {
        // File-vs-file mode
        (Some(before_path), Some(after_path), None, None) => {
            let b = load_snapshot(before_path);
            let a = load_snapshot(after_path);
            (b, a)
        }
        // Baseline-vs-live mode
        (None, None, Some(baseline_path), Some(config_path)) => {
            let b = load_snapshot(baseline_path);
            let cfg = load_config(config_path);
            let projects = filter_projects(&cfg, project);
            let project_cfg = projects[0];
            let w = resolve_weights(&cfg, None);
            let (graph, _, _stats) = run_extract(project_cfg, &cfg.settings, None, false);
            let (mut metrics, communities, cycles_simple) = run_analyze(&graph, &w);
            assign_community_ids(&mut metrics, &communities);
            // Build an AnalysisSnapshot from live data.
            let total_nodes = metrics.len();
            let total_edges = graph.edge_count();
            let total_communities = communities.len();
            let total_cycles = cycles_simple.len();
            let a = AnalysisSnapshot {
                nodes: metrics
                    .iter()
                    .map(|m| graphify_core::diff::NodeSnapshot {
                        id: m.id.clone(),
                        betweenness: m.betweenness,
                        pagerank: m.pagerank,
                        in_degree: m.in_degree,
                        out_degree: m.out_degree,
                        in_cycle: m.in_cycle,
                        score: m.score,
                        community_id: m.community_id,
                    })
                    .collect(),
                communities: communities
                    .iter()
                    .map(|c| graphify_core::diff::CommunitySnapshot {
                        id: c.id,
                        members: c.members.clone(),
                    })
                    .collect(),
                cycles: cycles_simple,
                summary: graphify_core::diff::SummarySnapshot {
                    total_nodes,
                    total_edges,
                    total_communities,
                    total_cycles,
                },
            };
            (b, a)
        }
        _ => {
            eprintln!(
                "Error: use either --before + --after (file mode) or --baseline + --config (live mode)"
            );
            std::process::exit(1);
        }
    };

    let report = compute_diff(&before_snapshot, &after_snapshot, threshold);

    let out_dir = output.unwrap_or(Path::new("."));
    std::fs::create_dir_all(out_dir).expect("create output directory");

    write_diff_json(&report, &out_dir.join("drift-report.json"));
    write_diff_markdown(&report, &out_dir.join("drift-report.md"));

    // Print summary to stdout.
    println!("Architectural Drift Report");
    println!(
        "  Nodes:       {} → {} ({:+})",
        report.summary_delta.nodes.before,
        report.summary_delta.nodes.after,
        report.summary_delta.nodes.change
    );
    println!(
        "  Edges:       {} → {} ({:+})",
        report.summary_delta.edges.before,
        report.summary_delta.edges.after,
        report.summary_delta.edges.change
    );
    println!(
        "  Communities: {} → {} ({:+})",
        report.summary_delta.communities.before,
        report.summary_delta.communities.after,
        report.summary_delta.communities.change
    );
    println!(
        "  Cycles:      {} → {} ({:+})",
        report.summary_delta.cycles.before,
        report.summary_delta.cycles.after,
        report.summary_delta.cycles.change
    );
    if !report.edges.added_nodes.is_empty() {
        println!("  New nodes:   {}", report.edges.added_nodes.len());
    }
    if !report.edges.removed_nodes.is_empty() {
        println!("  Removed:     {}", report.edges.removed_nodes.len());
    }
    if !report.hotspots.rising.is_empty() || !report.hotspots.falling.is_empty() {
        println!(
            "  Hotspots:    {} rising, {} falling",
            report.hotspots.rising.len(),
            report.hotspots.falling.len()
        );
    }
    if !report.communities.moved_nodes.is_empty() {
        println!(
            "  Community:   {} moved, {} stable",
            report.communities.moved_nodes.len(),
            report.communities.stable_count
        );
    }
    println!("Written to {}", out_dir.display());
}

fn cmd_trend(
    config_path: &Path,
    project_filter: Option<&str>,
    output: Option<&Path>,
    limit: Option<usize>,
    json: bool,
) {
    let cfg = load_config(config_path);
    let projects = filter_projects(&cfg, project_filter);

    if cfg.project.len() > 1 && project_filter.is_none() {
        eprintln!("Error: --project is required for multi-project trend reports.");
        std::process::exit(1);
    }

    let project = projects[0];
    let base_out = resolve_output(&cfg, None);
    let project_out = base_out.join(&project.name);
    let history_dir = project_out.join("history");

    let snapshots = match load_historical_snapshots(&history_dir) {
        Ok(snapshots) => snapshots,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    let report = match compute_trend_report(&project.name, &snapshots, limit) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("Cannot compute trend report: {err}");
            std::process::exit(1);
        }
    };

    let out_dir = output.unwrap_or(&project_out);
    std::fs::create_dir_all(out_dir).expect("create trend output directory");
    write_trend_json(&report, &out_dir.join("trend-report.json"));
    write_trend_markdown(&report, &out_dir.join("trend-report.md"));

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        println!("Architectural Trend Report");
        println!("  Project:     {}", report.project);
        println!("  Snapshots:   {}", report.snapshot_count);
        println!(
            "  Window:      {} → {}",
            report.window.first_captured_at, report.window.last_captured_at
        );
        if let Some(last) = report.points.last() {
            println!(
                "  Latest:      {} nodes, {} edges, {} cycles",
                last.total_nodes, last.total_edges, last.total_cycles
            );
        }
        println!("Written to {}", out_dir.display());
    }
}

fn load_snapshot(path: &Path) -> AnalysisSnapshot {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Cannot read {:?}: {e}", path);
            std::process::exit(1);
        }
    };
    match serde_json::from_str::<AnalysisSnapshot>(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid analysis JSON {:?}: {e}", path);
            std::process::exit(1);
        }
    }
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
        let parsed: Vec<f64> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
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
// Extraction pipeline
// ---------------------------------------------------------------------------

fn run_extract(
    project: &ProjectConfig,
    settings: &Settings,
    cache_dir: Option<&Path>,
    force: bool,
) -> (CodeGraph, Vec<String>, CacheStats) {
    let repo_path = PathBuf::from(&project.repo);
    let languages = parse_languages(&project.lang);

    // Build extra excludes as Vec<&str> slices.
    let extra_owned: Vec<String> = settings.exclude.clone().unwrap_or_default();
    let extra_excludes: Vec<&str> = extra_owned.iter().map(|s| s.as_str()).collect();

    let (effective_local_prefix, auto_detected) = match project.local_prefix.as_deref() {
        Some(prefix) => (prefix.to_owned(), false),
        None => (
            detect_local_prefix(&repo_path, &languages, &extra_excludes),
            true,
        ),
    };

    if auto_detected {
        let shown_prefix = if effective_local_prefix.is_empty() {
            "(root-level)"
        } else {
            effective_local_prefix.as_str()
        };
        eprintln!(
            "[{}] Auto-detected local_prefix: {}",
            project.name, shown_prefix
        );
    }

    // Discover files.
    let files = discover_files(
        &repo_path,
        &languages,
        &effective_local_prefix,
        &extra_excludes,
    );

    // BUG-009: Warn when discovery finds very few files — likely misconfigured
    // repo path or local_prefix.
    if files.len() <= 1 {
        eprintln!(
            "Warning: project '{}' discovered only {} file(s). Check repo path ('{}') and local_prefix ('{}') configuration.",
            project.name,
            files.len(),
            project.repo,
            effective_local_prefix,
        );
    }

    // Also warn if local_prefix looks like a directory but doesn't exist inside repo.
    if !effective_local_prefix.is_empty() {
        let prefix_dir = repo_path.join(&effective_local_prefix);
        if !prefix_dir.is_dir() {
            eprintln!(
                "Warning: project '{}' has local_prefix '{}' but directory '{}' does not exist.",
                project.name,
                effective_local_prefix,
                prefix_dir.display(),
            );
        }
    }

    // Load extraction cache (unless --force or no cache dir).
    let cache = match (force, cache_dir) {
        (false, Some(dir)) => {
            let cache_path = dir.join(".graphify-cache.json");
            ExtractionCache::load(&cache_path, &effective_local_prefix)
                .unwrap_or_else(|| ExtractionCache::new(&effective_local_prefix))
        }
        _ => ExtractionCache::new(&effective_local_prefix),
    };

    let mut stats = CacheStats {
        forced: force,
        ..Default::default()
    };

    // Build extractors.
    let python_extractor = PythonExtractor::new();
    let typescript_extractor = TypeScriptExtractor::new();
    let go_extractor = GoExtractor::new();
    let rust_extractor = RustExtractor::new();

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

    // Load go.mod if Go is in the language list.
    if languages.contains(&Language::Go) {
        let go_mod = repo_path.join("go.mod");
        if go_mod.exists() {
            resolver.load_go_mod(&go_mod);
        }
    }

    let repo_path_ref = &repo_path;

    // Extract each file in parallel: read → hash → cache check → parse on miss.
    let extraction_with_meta: Vec<(String, String, ExtractionResult, bool)> = files
        .par_iter()
        .filter_map(|file| {
            let source = match std::fs::read(&file.path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Warning: cannot read {:?}: {e}", file.path);
                    return None;
                }
            };

            let rel_path = file
                .path
                .strip_prefix(repo_path_ref)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .to_string();

            let hash = sha256_hex(&source);

            // Cache hit: reuse previous extraction.
            if let Some(cached) = cache.lookup(&rel_path, &hash) {
                return Some((rel_path, hash, cached.clone(), true));
            }

            // Cache miss: parse with tree-sitter.
            let extractor: &dyn LanguageExtractor = match file.language {
                Language::Python => &python_extractor,
                Language::TypeScript => &typescript_extractor,
                Language::Go => &go_extractor,
                Language::Rust => &rust_extractor,
            };

            let result = extractor.extract_file(&file.path, &source, &file.module_name);
            Some((rel_path, hash, result, false))
        })
        .collect();

    // Build new cache from extraction results and count stats.
    let mut new_cache = ExtractionCache::new(&effective_local_prefix);
    let mut results: Vec<ExtractionResult> = Vec::with_capacity(extraction_with_meta.len());

    for (rel_path, hash, result, was_hit) in extraction_with_meta {
        if was_hit {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
        new_cache.insert(rel_path, hash, result.clone());
        results.push(result);
    }

    // Count evictions: old cache entries whose paths aren't in the current discovered file set.
    let current_paths: HashSet<String> = new_cache.paths().cloned().collect();
    stats.evicted = cache
        .paths()
        .filter(|p| !current_paths.contains(*p))
        .count();

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
            let capped = edge.confidence.min(0.5);
            edge = edge.with_confidence(capped, graphify_core::types::ConfidenceKind::Ambiguous);
        }

        graph.add_edge(&src_id, &resolved_target, edge);
    }

    // Save updated cache.
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(dir).ok();
        new_cache.save(&dir.join(".graphify-cache.json"));
    }

    (graph, extra_owned, stats)
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
// Single-project pipeline helper
// ---------------------------------------------------------------------------

/// Runs the full pipeline for a single project: extract → analyze → write outputs.
///
/// Returns a `ProjectData` struct for use in cross-project summaries.
fn run_pipeline_for_project(
    project: &ProjectConfig,
    settings: &Settings,
    proj_out: &Path,
    weights: &ScoringWeights,
    formats: &[String],
    force: bool,
) -> ProjectData {
    let (graph, _, stats) = run_extract(project, settings, Some(proj_out), force);
    print_cache_stats(&project.name, &stats);
    let (mut metrics, communities, cycles_simple) = run_analyze(&graph, weights);
    assign_community_ids(&mut metrics, &communities);
    let cycles_for_report: Vec<Cycle> = cycles_simple;
    write_all_outputs(
        &project.name,
        &graph,
        &metrics,
        &communities,
        &cycles_for_report,
        proj_out,
        formats,
    );
    persist_historical_snapshot(
        &project.name,
        &graph,
        &metrics,
        &communities,
        &cycles_for_report,
        proj_out,
    );
    ProjectData {
        name: project.name.clone(),
        graph,
        metrics,
        community_count: communities.len(),
        cycles: cycles_for_report,
    }
}

fn persist_historical_snapshot(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[graphify_core::metrics::NodeMetrics],
    communities: &[graphify_core::community::Community],
    cycles: &[Cycle],
    proj_out: &Path,
) {
    let captured_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let history_dir = proj_out.join("history");
    std::fs::create_dir_all(&history_dir).expect("create history directory");

    let snapshot = build_historical_snapshot(
        project_name,
        graph,
        metrics,
        communities,
        cycles,
        captured_at,
    );
    let path = history_dir.join(format!("{captured_at}.json"));
    let payload = serde_json::to_string_pretty(&snapshot).expect("serialize history snapshot");
    std::fs::write(&path, payload).expect("write history snapshot");
}

// ---------------------------------------------------------------------------
// Quality gates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
struct CheckLimits {
    max_cycles: Option<usize>,
    max_hotspot_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectCheckSummary {
    nodes: usize,
    edges: usize,
    communities: usize,
    cycles: usize,
    max_hotspot_score: f64,
    max_hotspot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PolicyCheckSummary {
    rules_evaluated: usize,
    policy_violations: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum CheckViolation {
    #[serde(rename = "limit")]
    Limit {
        kind: String,
        actual: serde_json::Value,
        expected_max: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    #[serde(rename = "policy")]
    Policy {
        kind: String,
        rule: String,
        source_node: String,
        target_node: String,
        source_project: String,
        target_project: String,
        source_selectors: Vec<String>,
        target_selectors: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
struct ProjectCheckResult {
    name: String,
    ok: bool,
    summary: ProjectCheckSummary,
    limits: CheckLimits,
    policy_summary: PolicyCheckSummary,
    violations: Vec<CheckViolation>,
}

#[derive(Debug, Clone, Serialize)]
struct CheckReport {
    ok: bool,
    violations: usize,
    projects: Vec<ProjectCheckResult>,
}

fn evaluate_quality_gates(
    graph: &CodeGraph,
    metrics: &[graphify_core::metrics::NodeMetrics],
    communities: &[graphify_core::community::Community],
    cycles: &[Cycle],
    limits: &CheckLimits,
) -> (ProjectCheckSummary, Vec<CheckViolation>) {
    let top_hotspot = metrics.iter().max_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let summary = ProjectCheckSummary {
        nodes: graph.node_count(),
        edges: graph.edge_count(),
        communities: communities.len(),
        cycles: cycles.len(),
        max_hotspot_score: top_hotspot.map(|m| m.score).unwrap_or(0.0),
        max_hotspot_id: top_hotspot.map(|m| m.id.clone()),
    };

    let mut violations = Vec::new();

    if let Some(max_cycles) = limits.max_cycles {
        if summary.cycles > max_cycles {
            violations.push(CheckViolation::Limit {
                kind: "max_cycles".to_string(),
                actual: serde_json::json!(summary.cycles),
                expected_max: serde_json::json!(max_cycles),
                node_id: None,
            });
        }
    }

    if let Some(max_hotspot_score) = limits.max_hotspot_score {
        if summary.max_hotspot_score > max_hotspot_score {
            violations.push(CheckViolation::Limit {
                kind: "max_hotspot_score".to_string(),
                actual: serde_json::json!(summary.max_hotspot_score),
                expected_max: serde_json::json!(max_hotspot_score),
                node_id: summary.max_hotspot_id.clone(),
            });
        }
    }

    (summary, violations)
}

fn build_project_check_result(
    project_name: &str,
    summary: ProjectCheckSummary,
    limits: CheckLimits,
    policy_result: ProjectPolicyResult,
    mut violations: Vec<CheckViolation>,
) -> ProjectCheckResult {
    let policy_violations = policy_result.violations.len();
    violations.extend(policy_result.violations.into_iter().map(|violation| {
        CheckViolation::Policy {
            kind: "policy_rule".to_string(),
            rule: violation.rule,
            source_node: violation.source_node,
            target_node: violation.target_node,
            source_project: violation.source_project,
            target_project: violation.target_project,
            source_selectors: violation.source_selectors,
            target_selectors: violation.target_selectors,
        }
    }));

    ProjectCheckResult {
        name: project_name.to_string(),
        ok: violations.is_empty(),
        summary,
        limits,
        policy_summary: PolicyCheckSummary {
            rules_evaluated: policy_result.rules_evaluated,
            policy_violations,
        },
        violations,
    }
}

fn build_check_report(projects: Vec<ProjectCheckResult>) -> CheckReport {
    let violations = projects.iter().map(|p| p.violations.len()).sum();
    let ok = projects.iter().all(|p| p.ok);
    CheckReport {
        ok,
        violations,
        projects,
    }
}

fn print_check_report(report: &CheckReport) {
    for project in &report.projects {
        let status = if project.ok { "PASS" } else { "FAIL" };
        let hotspot = match &project.summary.max_hotspot_id {
            Some(node_id) => format!("{:.3} ({node_id})", project.summary.max_hotspot_score),
            None => format!("{:.3}", project.summary.max_hotspot_score),
        };
        println!(
            "[{}] {} nodes={} edges={} communities={} cycles={} max_hotspot={} policy_violations={}",
            project.name,
            status,
            project.summary.nodes,
            project.summary.edges,
            project.summary.communities,
            project.summary.cycles,
            hotspot,
            project.policy_summary.policy_violations
        );

        for violation in &project.violations {
            match violation {
                CheckViolation::Limit {
                    kind,
                    actual,
                    expected_max,
                    ..
                } if kind == "max_cycles" => {
                    println!(
                        "  - max_cycles: actual {} > expected {}",
                        actual, expected_max
                    );
                }
                CheckViolation::Limit {
                    kind,
                    actual,
                    expected_max,
                    node_id,
                } if kind == "max_hotspot_score" => {
                    if let Some(node_id) = node_id {
                        println!(
                            "  - max_hotspot_score: actual {:.3} > expected {:.3} at {}",
                            actual.as_f64().unwrap_or_default(),
                            expected_max.as_f64().unwrap_or_default(),
                            node_id
                        );
                    } else {
                        println!(
                            "  - max_hotspot_score: actual {:.3} > expected {:.3}",
                            actual.as_f64().unwrap_or_default(),
                            expected_max.as_f64().unwrap_or_default()
                        );
                    }
                }
                CheckViolation::Limit {
                    kind,
                    actual,
                    expected_max,
                    ..
                } => {
                    println!(
                        "  - {}: actual {} > expected {}",
                        kind, actual, expected_max
                    );
                }
                CheckViolation::Policy {
                    rule,
                    source_node,
                    target_node,
                    source_project,
                    target_project,
                    ..
                } => {
                    println!(
                        "  - {}: {} -> {} [{} -> {}]",
                        rule, source_node, target_node, source_project, target_project
                    );
                }
            }
        }
    }

    if report.ok {
        println!("All checks passed");
    } else {
        let failing_projects = report.projects.iter().filter(|p| !p.ok).count();
        println!(
            "Check failed: {} violation(s) across {} project(s)",
            report.violations, failing_projects
        );
    }
}

fn cmd_check(
    config_path: &Path,
    project_filter: Option<&str>,
    force: bool,
    limits: CheckLimits,
    json: bool,
) {
    let cfg = load_config(config_path);
    let projects = filter_projects(&cfg, project_filter);
    let mut analyzed_projects = Vec::new();

    for project in &projects {
        let (graph, _excludes, stats) = run_extract(project, &cfg.settings, None, force);
        print_cache_stats(&project.name, &stats);
        let (metrics, communities, cycles) = run_analyze(&graph, &ScoringWeights::default());
        analyzed_projects.push(CheckProjectData {
            name: project.name.clone(),
            graph,
            metrics,
            communities,
            cycles,
        });
    }

    let compiled_policy = if cfg.policy.is_empty() {
        None
    } else {
        Some(CompiledPolicy::compile(&cfg.policy).unwrap_or_else(|err| {
            eprintln!("Invalid policy config: {err}");
            std::process::exit(1);
        }))
    };

    let policy_results = if let Some(policy) = &compiled_policy {
        let policy_inputs: Vec<ProjectGraph<'_>> = analyzed_projects
            .iter()
            .map(|project| ProjectGraph {
                name: &project.name,
                graph: &project.graph,
            })
            .collect();
        policy.evaluate(&policy_inputs)
    } else {
        Vec::new()
    };

    let policy_by_name: HashMap<String, ProjectPolicyResult> = policy_results
        .into_iter()
        .map(|result| (result.name.clone(), result))
        .collect();

    let mut results = Vec::new();
    for project in analyzed_projects {
        let (summary, violations) = evaluate_quality_gates(
            &project.graph,
            &project.metrics,
            &project.communities,
            &project.cycles,
            &limits,
        );
        let policy_result =
            policy_by_name
                .get(&project.name)
                .cloned()
                .unwrap_or(ProjectPolicyResult {
                    name: project.name.clone(),
                    rules_evaluated: 0,
                    violations: Vec::new(),
                });
        results.push(build_project_check_result(
            &project.name,
            summary,
            limits.clone(),
            policy_result,
            violations,
        ));
    }

    let report = build_check_report(results);
    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        print_check_report(&report);
    }

    if !report.ok {
        std::process::exit(1);
    }
}

struct CheckProjectData {
    name: String,
    graph: CodeGraph,
    metrics: Vec<graphify_core::metrics::NodeMetrics>,
    communities: Vec<graphify_core::community::Community>,
    cycles: Vec<Cycle>,
}

// ---------------------------------------------------------------------------
// Query engine helpers
// ---------------------------------------------------------------------------

fn build_query_engine(project: &ProjectConfig, settings: &Settings) -> QueryEngine {
    let (graph, _, _stats) = run_extract(project, settings, None, false);
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
        "class" | "struct" => Some(graphify_core::types::NodeKind::Class),
        "method" => Some(graphify_core::types::NodeKind::Method),
        "trait" | "interface" => Some(graphify_core::types::NodeKind::Trait),
        "enum" => Some(graphify_core::types::NodeKind::Enum),
        _ => {
            eprintln!("Warning: unknown kind '{}', ignoring filter.", s);
            None
        }
    }
}

fn print_explain_report(
    report: &graphify_core::query::ExplainReport,
    project_name: &str,
    multi_project: bool,
) {
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
        println!(
            "  In cycle:    yes (with: {})",
            report.cycle_peers.join(", ")
        );
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
    println!(
        "  ── Dependencies ({}) ──",
        report.direct_dependencies.len()
    );
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
        println!(
            "  ... and {} more",
            report.direct_dependents.len() - max_show
        );
    }

    println!();
    println!("  ── Impact ──");
    println!(
        "  Transitive dependents: {} modules",
        report.transitive_dependent_count
    );
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
// Shell (REPL)
// ---------------------------------------------------------------------------

fn cmd_shell(config_path: &Path, project_filter: Option<&str>) {
    use std::io::{BufRead, Write};

    let cfg = load_config(config_path);
    let projects = filter_projects(&cfg, project_filter);

    // Build engines for each project
    let mut engines: Vec<(String, QueryEngine)> = Vec::new();
    for proj in &projects {
        eprintln!("[{}] Loading...", proj.name);
        let engine = build_query_engine(proj, &cfg.settings);
        engines.push((proj.name.clone(), engine));
    }

    println!();
    println!(
        "Graphify interactive shell ({} project(s) loaded)",
        engines.len()
    );
    println!("Type 'help' for available commands, 'exit' to quit.");
    println!();

    let stdin = std::io::stdin();
    let reader = std::io::BufReader::new(stdin.lock());

    print!("graphify> ");
    std::io::stdout().flush().ok();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            print!("graphify> ");
            std::io::stdout().flush().ok();
            continue;
        }

        let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
        let cmd = parts[0];

        match cmd {
            "exit" | "quit" => break,

            "help" => {
                println!("Commands:");
                println!("  stats                    Show graph statistics");
                println!("  query <pattern>          Search nodes by glob pattern");
                println!("  path <source> <target>   Find shortest path between nodes");
                println!("  explain <node_id>        Show detailed info about a node");
                println!("  exit / quit              Exit the shell");
                println!("  help                     Show this help");
            }

            "stats" => {
                for (name, engine) in &engines {
                    let s = engine.stats();
                    println!(
                        "[{}] {} nodes, {} edges, {} local, {} communities, {} cycles",
                        name,
                        s.node_count,
                        s.edge_count,
                        s.local_node_count,
                        s.community_count,
                        s.cycle_count
                    );
                }
            }

            "query" => {
                if parts.len() < 2 {
                    println!("Usage: query <pattern>");
                } else {
                    let pattern = parts[1];
                    let filters = SearchFilters::default();
                    for (name, engine) in &engines {
                        let results = engine.search(pattern, &filters);
                        if !results.is_empty() {
                            let multi = engines.len() > 1;
                            for r in &results {
                                if multi {
                                    println!(
                                        "  [{}] {} ({:?}) score={:.3}",
                                        name, r.node_id, r.kind, r.score
                                    );
                                } else {
                                    println!("  {} ({:?}) score={:.3}", r.node_id, r.kind, r.score);
                                }
                            }
                        }
                    }
                }
            }

            "path" => {
                if parts.len() < 3 {
                    println!("Usage: path <source> <target>");
                } else {
                    let source = parts[1];
                    let target = parts[2];
                    let mut found = false;
                    for (name, engine) in &engines {
                        if let Some(path) = engine.shortest_path(source, target) {
                            found = true;
                            if engines.len() > 1 {
                                print!("[{}] ", name);
                            }
                            print_path(&path);
                            break;
                        }
                    }
                    if !found {
                        println!("No path found from '{}' to '{}'.", source, target);
                    }
                }
            }

            "explain" => {
                if parts.len() < 2 {
                    println!("Usage: explain <node_id>");
                } else {
                    let node_id = parts[1];
                    let mut found = false;
                    for (name, engine) in &engines {
                        if let Some(report) = engine.explain(node_id) {
                            found = true;
                            print_explain_report(&report, name, engines.len() > 1);
                            break;
                        }
                    }
                    if !found {
                        println!("Node '{}' not found.", node_id);
                        for (_name, engine) in &engines {
                            let suggestions = engine.suggest(node_id);
                            if !suggestions.is_empty() {
                                println!("Did you mean?");
                                for s in &suggestions {
                                    println!("  {}", s);
                                }
                                break;
                            }
                        }
                    }
                }
            }

            _ => {
                println!(
                    "Unknown command '{}'. Type 'help' for available commands.",
                    cmd
                );
            }
        }

        print!("graphify> ");
        std::io::stdout().flush().ok();
    }

    println!();
}

// ---------------------------------------------------------------------------
// Cache stats helper
// ---------------------------------------------------------------------------

fn print_cache_stats(project_name: &str, stats: &CacheStats) {
    if stats.forced {
        eprintln!("[{}] Cache: forced full rebuild", project_name);
    } else if stats.hits > 0 || stats.evicted > 0 {
        eprintln!(
            "[{}] Cache: {} hits, {} misses, {} evicted",
            project_name, stats.hits, stats.misses, stats.evicted
        );
    }
}

// ---------------------------------------------------------------------------
// Assign community IDs back to NodeMetrics
// ---------------------------------------------------------------------------

fn assign_community_ids(
    metrics: &mut [graphify_core::metrics::NodeMetrics],
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
                write_analysis_json(
                    metrics,
                    communities,
                    cycles,
                    graph,
                    &out_dir.join("analysis.json"),
                );
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
                    graph,
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
            "neo4j" | "cypher" => {
                write_cypher(graph, &out_dir.join("graph.cypher"));
            }
            "graphml" => {
                write_graphml(graph, &out_dir.join("graph.graphml"));
            }
            "obsidian" => {
                write_obsidian_vault(
                    graph,
                    metrics,
                    communities,
                    cycles,
                    &out_dir.join("obsidian_vault"),
                );
            }
            other => {
                eprintln!("Warning: unknown format '{other}', skipping.");
            }
        }
    }
}

fn prune_stale_project_dirs(out_dir: &Path, active_projects: &[ProjectConfig]) {
    if !out_dir.exists() {
        return;
    }

    let active_names: HashSet<&str> = active_projects.iter().map(|p| p.name.as_str()).collect();
    let entries = match std::fs::read_dir(out_dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!(
                "Warning: could not inspect output directory {}: {err}",
                out_dir.display()
            );
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!(
                    "Warning: could not inspect an entry inside {}: {err}",
                    out_dir.display()
                );
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                eprintln!(
                    "Warning: could not determine entry type for {}: {err}",
                    entry.path().display()
                );
                continue;
            }
        };

        if !file_type.is_dir() {
            continue;
        }

        let dir_name = entry.file_name();
        let dir_name = dir_name.to_string_lossy();
        if active_names.contains(dir_name.as_ref()) {
            continue;
        }

        let path = entry.path();
        if is_prunable_stale_project_dir(&path) {
            if let Err(err) = std::fs::remove_dir_all(&path) {
                eprintln!(
                    "Warning: failed to prune stale Graphify output directory {}: {err}",
                    path.display()
                );
            } else {
                eprintln!("Pruned stale Graphify output directory {}", path.display());
            }
        }
    }
}

fn is_prunable_stale_project_dir(path: &Path) -> bool {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    let mut has_graphify_artifact = false;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return false,
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => return false,
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();

        let is_known_file = file_type.is_file()
            && matches!(
                name.as_ref(),
                ".graphify-cache.json"
                    | "graph.json"
                    | "analysis.json"
                    | "graph_nodes.csv"
                    | "graph_edges.csv"
                    | "architecture_report.md"
                    | "architecture_graph.html"
                    | "graph.cypher"
                    | "graph.graphml"
            );
        let is_known_dir = file_type.is_dir() && matches!(name.as_ref(), "obsidian_vault");

        if is_known_file || is_known_dir {
            has_graphify_artifact = true;
            continue;
        }

        return false;
    }

    has_graphify_artifact
}

// ---------------------------------------------------------------------------
// Cross-project summary
// ---------------------------------------------------------------------------

/// Aggregated per-project data used by the cross-project summary.
struct ProjectData {
    name: String,
    graph: CodeGraph,
    metrics: Vec<graphify_core::metrics::NodeMetrics>,
    community_count: usize,
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
            let top_hotspot = p
                .metrics
                .iter()
                .max_by(|a, b| {
                    a.score
                        .partial_cmp(&b.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "score": (m.score * 1000.0).round() / 1000.0,
                    })
                });
            serde_json::json!({
                "name": p.name,
                "nodes": node_count,
                "edges": edge_count,
                "communities": p.community_count,
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
    all_hotspots.sort_by(|a, b| {
        b.1.score
            .partial_cmp(&a.1.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
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
        .filter_map(|d| {
            d.get("edge_count")
                .and_then(|e| e.as_u64())
                .map(|n| n as usize)
        })
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

// ---------------------------------------------------------------------------
// watch command
// ---------------------------------------------------------------------------

fn cmd_watch(
    config_path: &Path,
    output_override: Option<&Path>,
    force: bool,
    format_override: Option<&str>,
) {
    use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
    use watch::{determine_affected_projects, WatchFilter};

    let cfg = load_config(config_path);
    let out_dir = resolve_output(&cfg, output_override);
    let weights = resolve_weights(&cfg, None);
    let formats = resolve_formats(&cfg, format_override);

    if cfg.project.is_empty() {
        eprintln!("Error: no projects configured in config file.");
        std::process::exit(1);
    }

    // Collect all language strings and excludes for the watch filter.
    let all_langs: Vec<String> = cfg
        .project
        .iter()
        .flat_map(|p| p.lang.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let exclude_dirs = cfg.settings.exclude.clone().unwrap_or_default();

    let canonical_out = std::fs::canonicalize(&out_dir).unwrap_or_else(|_| out_dir.clone());
    let filter = WatchFilter::new(&all_langs, &exclude_dirs, &canonical_out);

    // Collect project repo paths for affected-project detection.
    let project_repos: Vec<PathBuf> = cfg
        .project
        .iter()
        .map(|p| {
            let repo = PathBuf::from(&p.repo);
            std::fs::canonicalize(&repo).unwrap_or(repo)
        })
        .collect();

    // Run initial pipeline.
    eprintln!("=== Initial build ===");
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let _ =
            run_pipeline_for_project(project, &cfg.settings, &proj_out, &weights, &formats, force);
        eprintln!("[{}] Ready.", project.name);
    }

    // Setup file watcher.
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer =
        new_debouncer(std::time::Duration::from_millis(300), tx).expect("create file watcher");

    for repo in &project_repos {
        debouncer
            .watcher()
            .watch(repo, notify::RecursiveMode::Recursive)
            .unwrap_or_else(|e| {
                eprintln!("Error: cannot watch {:?}: {e}", repo);
                std::process::exit(1);
            });
    }

    eprintln!();
    eprintln!(
        "Watching {} project(s). Press Ctrl+C to stop.",
        cfg.project.len()
    );
    for (i, repo) in project_repos.iter().enumerate() {
        eprintln!("  [{}] {}", cfg.project[i].name, repo.display());
    }
    eprintln!();

    // Event loop.
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let changed_paths: Vec<PathBuf> = events
                    .iter()
                    .filter(|e| e.kind == DebouncedEventKind::Any)
                    .map(|e| e.path.clone())
                    .filter(|p| filter.should_rebuild(p))
                    .collect();

                if changed_paths.is_empty() {
                    continue;
                }

                let affected = determine_affected_projects(&changed_paths, &project_repos);
                if affected.is_empty() {
                    continue;
                }

                let start = std::time::Instant::now();
                eprintln!(
                    "--- Rebuild triggered ({} file(s) changed) ---",
                    changed_paths.len()
                );

                for &idx in &affected {
                    let project = &cfg.project[idx];
                    let proj_out = out_dir.join(&project.name);
                    std::fs::create_dir_all(&proj_out).expect("create output directory");
                    let _ = run_pipeline_for_project(
                        project,
                        &cfg.settings,
                        &proj_out,
                        &weights,
                        &formats,
                        false,
                    );
                    eprintln!("[{}] Rebuilt.", project.name);
                }

                let elapsed = start.elapsed();
                eprintln!("--- Done in {:.1}s ---\n", elapsed.as_secs_f64());
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {e:?}");
            }
            Err(e) => {
                eprintln!("Channel error: {e}");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{Language, Node};

    fn sample_graph() -> CodeGraph {
        let mut graph = CodeGraph::new();
        graph.add_node(Node::module("a", "a.ts", Language::TypeScript, 1, true));
        graph.add_node(Node::module("b", "b.ts", Language::TypeScript, 1, true));
        graph
    }

    fn metric(id: &str, score: f64) -> graphify_core::metrics::NodeMetrics {
        graphify_core::metrics::NodeMetrics {
            id: id.to_string(),
            betweenness: 0.0,
            pagerank: 0.0,
            in_degree: 0,
            out_degree: 0,
            in_cycle: false,
            score,
            community_id: 0,
        }
    }

    #[test]
    fn evaluate_quality_gates_without_limits_passes() {
        let graph = sample_graph();
        let (summary, violations) = evaluate_quality_gates(
            &graph,
            &[metric("a", 0.4), metric("b", 0.7)],
            &[],
            &[],
            &CheckLimits::default(),
        );

        assert!(violations.is_empty(), "expected no violations");
        assert_eq!(summary.max_hotspot_id.as_deref(), Some("b"));
        assert!((summary.max_hotspot_score - 0.7).abs() < 1e-9);
    }

    #[test]
    fn evaluate_quality_gates_selects_highest_hotspot_score() {
        let graph = sample_graph();
        let (summary, _violations) = evaluate_quality_gates(
            &graph,
            &[metric("a", 0.91), metric("b", 0.65)],
            &[],
            &[],
            &CheckLimits::default(),
        );

        assert_eq!(summary.max_hotspot_id.as_deref(), Some("a"));
        assert!((summary.max_hotspot_score - 0.91).abs() < 1e-9);
    }

    #[test]
    fn evaluate_quality_gates_accumulates_multiple_violations() {
        let graph = sample_graph();
        let (_summary, violations) = evaluate_quality_gates(
            &graph,
            &[metric("a", 0.91)],
            &[],
            &[vec!["a".to_string(), "b".to_string()]],
            &CheckLimits {
                max_cycles: Some(0),
                max_hotspot_score: Some(0.8),
            },
        );

        assert_eq!(violations.len(), 2, "expected two violations");
        assert!(matches!(
            &violations[0],
            CheckViolation::Limit { kind, .. } if kind == "max_cycles"
        ));
        assert!(matches!(
            &violations[1],
            CheckViolation::Limit { kind, .. } if kind == "max_hotspot_score"
        ));
    }
}
