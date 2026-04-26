use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use serde::Deserialize;

use graphify_core::consolidation::{ConsolidationConfig, ConsolidationConfigRaw};
use graphify_core::contract::{
    CaseRule, FieldAlias, FieldType, GlobalContractConfig, PairConfig, PrimitiveType, Severity,
};
use graphify_core::diff::{compute_diff_with_config, AnalysisSnapshot};
use graphify_core::{
    community::detect_communities,
    cycles::{find_sccs, find_sccs_excluding, find_simple_cycles, find_simple_cycles_excluding},
    graph::CodeGraph,
    history::{build_historical_snapshot, compute_trend_report, load_historical_snapshots},
    metrics::{compute_metrics_with_thresholds, HotspotThresholds, ScoringWeights},
    policy::{CompiledPolicy, PolicyConfig, ProjectGraph, ProjectPolicyResult},
    query::{QueryEngine, SearchFilters, SortField},
    types::Language,
};
use graphify_extract::{
    cache::{sha256_hex, CacheStats, ExtractionCache},
    walker::detect_local_prefix,
    ExternalStubs, ExtractionResult, GoExtractor, LanguageExtractor, PhpExtractor, PythonExtractor,
    RustExtractor, TypeScriptExtractor,
};
use graphify_report::{
    check_report::{
        CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
        ProjectCheckSummary,
    },
    write_analysis_json, write_analysis_json_with_allowlist, write_compare_json,
    write_compare_markdown, write_cypher, write_diff_json, write_diff_markdown, write_edges_csv,
    write_graph_json, write_graphml, write_html, write_nodes_csv, write_obsidian_vault,
    write_report, write_trend_json, write_trend_markdown, Cycle,
};

mod install;
mod session;
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
    #[serde(default)]
    contract: ContractConfigRaw,
    #[serde(default)]
    hotspots: HotspotsConfig,
    #[serde(default)]
    consolidation: ConsolidationConfigToml,
}

/// TOML wire format for `[consolidation]` — compiled to
/// [`graphify_core::consolidation::ConsolidationConfig`] at `load_config` time.
#[derive(Deserialize, Default)]
struct ConsolidationConfigToml {
    #[serde(default)]
    allowlist: Vec<String>,
    #[serde(default)]
    intentional_mirrors: HashMap<String, Vec<String>>,
    /// BUG-015 opt-in: when `true`, cycle detection drops cycles whose only
    /// cycle-making edges route through an allowlisted root barrel node
    /// (node id == project `local_prefix`). Default `false` preserves
    /// pre-BUG-015 behaviour for existing configs.
    #[serde(default)]
    suppress_barrel_cycles: bool,
}

impl From<ConsolidationConfigToml> for ConsolidationConfigRaw {
    fn from(v: ConsolidationConfigToml) -> Self {
        ConsolidationConfigRaw {
            allowlist: v.allowlist,
            intentional_mirrors: v.intentional_mirrors,
            suppress_barrel_cycles: v.suppress_barrel_cycles,
        }
    }
}

#[derive(Deserialize, Default)]
struct HotspotsConfig {
    hub_threshold: Option<usize>,
    bridge_ratio: Option<f64>,
}

#[derive(Deserialize, Default)]
struct ContractConfigRaw {
    #[serde(default)]
    type_map: std::collections::HashMap<String, String>,
    #[serde(default = "default_case_rule")]
    case_rule: String,
    #[serde(default = "default_unmapped_severity")]
    unmapped_type_severity: String,
    #[serde(default, rename = "pair")]
    pairs: Vec<PairConfigRaw>,
}

fn default_case_rule() -> String {
    "snake_camel".into()
}

fn default_unmapped_severity() -> String {
    "warning".into()
}

#[derive(Deserialize, Default)]
struct PairConfigRaw {
    name: String,
    orm: PairEndpointRaw,
    ts: PairEndpointRaw,
    #[serde(default)]
    field_alias: Vec<FieldAliasRaw>,
    #[serde(default)]
    relation_alias: Vec<FieldAliasRaw>,
    #[serde(default)]
    ignore: Option<IgnoreRaw>,
}

#[derive(Deserialize, Default, Clone)]
struct PairEndpointRaw {
    #[serde(default)]
    source: String, // "drizzle" (required for orm, ignored for ts)
    file: String,
    #[serde(default)]
    table: String,
    #[serde(default)]
    export: String,
}

#[derive(Deserialize, Default, Clone)]
struct FieldAliasRaw {
    orm: String,
    ts: String,
}

#[derive(Deserialize, Default, Clone)]
struct IgnoreRaw {
    #[serde(default)]
    orm: Vec<String>,
    #[serde(default)]
    ts: Vec<String>,
}

#[derive(Deserialize, Default)]
struct Settings {
    output: Option<String>,
    weights: Option<Vec<f64>>,
    exclude: Option<Vec<String>>,
    format: Option<Vec<String>>,
    /// Opt-out switch for the workspace-wide `ReExportGraph` cross-project
    /// fan-out introduced in FEAT-028 (shipped `v0.11.0`). When explicitly
    /// set to `false`, multi-project TS configs fall back to the legacy
    /// per-project fan-out path — cross-project aliases land on the raw
    /// barrel id instead of the sibling's canonical module. Default
    /// (absent or `true`) keeps the FEAT-028 behaviour. See
    /// `docs/adr/0001-workspace-reexport-graph-gate.md`.
    workspace_reexport_graph: Option<bool>,
    /// Shared `external_stubs` prefixes that merge with every project's own
    /// `external_stubs` list. Lets single-language workspaces (e.g. Rust
    /// with its prelude + std) declare the common set once instead of
    /// repeating it per `[[project]]`. See FEAT-034.
    external_stubs: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ProjectConfig {
    name: String,
    repo: String,
    lang: Vec<String>,
    local_prefix: Option<String>,
    /// Package prefixes the project declares as intentionally external.
    /// Matching edges get `ConfidenceKind::ExpectedExternal` instead of
    /// `Ambiguous`, so the ambiguity metric reflects real extraction noise.
    #[serde(default)]
    external_stubs: Vec<String>,
    /// Per-project overrides for the `graphify check` gate (issue #14).
    ///
    /// Fields mirror the CLI flags. Precedence for the resolved limit:
    /// CLI flag > `[project.check]` > None (no gate for that dimension).
    /// `#[serde(deny_unknown_fields)]` on `ProjectCheck` catches typos
    /// inside this block (e.g. `max_hoptspot_score`) so a misspelled key
    /// never silently disables a gate.
    check: Option<ProjectCheck>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ProjectCheck {
    max_cycles: Option<usize>,
    max_hotspot_score: Option<f64>,
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
    #[command(
        about = "Generate a graphify.toml template in the current directory",
        long_about = "Generate a graphify.toml template in the current directory.\n\nThis command only creates graphify.toml. It does not install AI assistant integrations such as slash commands, skills, agents, or MCP registration.",
        after_help = "Next steps:\n  graphify install-integrations --project-local\n  graphify run"
    )]
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

        /// In-degree threshold above which a node is classified as a hub (default: 50)
        #[arg(long)]
        hub_threshold: Option<usize>,

        /// betweenness/in_degree ratio above which a node is classified as a bridge (default: 3000)
        #[arg(long)]
        bridge_ratio: Option<f64>,

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

        /// In-degree threshold above which a node is classified as a hub (default: 50)
        #[arg(long)]
        hub_threshold: Option<usize>,

        /// betweenness/in_degree ratio above which a node is classified as a bridge (default: 3000)
        #[arg(long)]
        bridge_ratio: Option<f64>,

        /// Output formats: json,csv,md,html,neo4j,graphml,obsidian (comma-separated)
        #[arg(long)]
        format: Option<String>,

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,

        /// Skip the `[consolidation].allowlist` section (debug flag — run as
        /// if no allowlist were configured).
        #[arg(long, default_value_t = false)]
        ignore_allowlist: bool,
    },

    /// Run full pipeline: extract → analyze → report (alias for report)
    Run {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// In-degree threshold above which a node is classified as a hub (default: 50)
        #[arg(long)]
        hub_threshold: Option<usize>,

        /// betweenness/in_degree ratio above which a node is classified as a bridge (default: 3000)
        #[arg(long)]
        bridge_ratio: Option<f64>,

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,

        /// Skip the `[consolidation].allowlist` section (debug flag — run as
        /// if no allowlist were configured).
        #[arg(long, default_value_t = false)]
        ignore_allowlist: bool,
    },

    /// Check architectural quality gates for CI
    Check {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting) — `check-report.json`
        /// is written under `<output>/<project>/` for every project
        #[arg(long)]
        output: Option<PathBuf>,

        /// Maximum allowed cycle count
        #[arg(long)]
        max_cycles: Option<usize>,

        /// Maximum allowed hotspot score
        #[arg(long)]
        max_hotspot_score: Option<f64>,

        /// In-degree threshold above which a node is classified as a hub (default: 50)
        #[arg(long)]
        hub_threshold: Option<usize>,

        /// betweenness/in_degree ratio above which a node is classified as a bridge (default: 3000)
        #[arg(long)]
        bridge_ratio: Option<f64>,

        /// Filter to a specific project
        #[arg(long)]
        project: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,

        /// Run the contract drift gate (default: on if [[contract.pair]] is declared)
        #[arg(long, default_value_t = false, conflicts_with = "no_contracts")]
        contracts: bool,

        /// Skip the contract drift gate even when pairs are configured
        #[arg(long = "no-contracts", default_value_t = false)]
        no_contracts: bool,

        /// Treat contract warnings (UnmappedOrmType) as errors
        #[arg(long, default_value_t = false)]
        contracts_warnings_as_errors: bool,

        /// Skip the `[consolidation].allowlist` section (debug flag — run as
        /// if no allowlist were configured).
        #[arg(long, default_value_t = false)]
        ignore_allowlist: bool,
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

        /// Disable ANSI color output (auto-off when stdout is not a TTY or
        /// `NO_COLOR` is set to a non-empty value)
        #[arg(long)]
        no_color: bool,
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
    ///
    /// Requires full `analysis.json` files — not the trend-format snapshots
    /// under `report/<project>/history/` (those are only consumable by
    /// `graphify trend`). To baseline before a refactor, copy
    /// `analysis.json` to `baseline.json` and diff against it later.
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

        /// Skip the `[consolidation].allowlist` and
        /// `[consolidation.intentional_mirrors]` sections when a config is
        /// supplied (debug flag — emits un-annotated hotspot entries).
        #[arg(long, default_value_t = false)]
        ignore_allowlist: bool,
    },

    /// Compare two existing Graphify analysis outputs head-to-head
    ///
    /// Inputs may be either full `analysis.json` files or directories that
    /// contain `analysis.json`, such as `report/<project>/` folders from two
    /// PR artifacts.
    Compare {
        /// Left-hand analysis.json file or directory containing analysis.json
        left: PathBuf,

        /// Right-hand analysis.json file or directory containing analysis.json
        right: PathBuf,

        /// Label for the left-hand side in stdout and Markdown output
        #[arg(long)]
        left_label: Option<String>,

        /// Label for the right-hand side in stdout and Markdown output
        #[arg(long)]
        right_label: Option<String>,

        /// Path to graphify.toml for consolidation annotations
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output directory for compare report files (default: current directory)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Minimum score delta to report as significant (default: 0.05)
        #[arg(long, default_value = "0.05")]
        threshold: f64,

        /// Skip the `[consolidation].allowlist` and
        /// `[consolidation.intentional_mirrors]` sections when a config is
        /// supplied (debug flag — emits un-annotated hotspot entries).
        #[arg(long, default_value_t = false)]
        ignore_allowlist: bool,
    },

    /// Render a PR-ready Markdown summary from a project's Graphify output directory.
    PrSummary {
        /// Path to a single project's Graphify output directory (for example ./report/my-app).
        dir: PathBuf,

        /// Number of architectural-smell rows to surface in the summary.
        /// Pass 0 to suppress the smells section entirely. Default: 5.
        #[arg(long, default_value_t = 5)]
        top: usize,
    },

    /// Emit consolidation candidates — symbols whose leaf name is shared by
    /// multiple nodes and are therefore candidates for a consolidation
    /// refactor. Writes `consolidation-candidates.json` per project (and a
    /// cross-project aggregate when the config declares 2+ projects).
    Consolidation {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Skip the `[consolidation].allowlist` section (debug flag — emits
        /// all candidates, tagging allowlist hits with `allowlisted: true`).
        #[arg(long, default_value_t = false)]
        ignore_allowlist: bool,

        /// Minimum group size; groups with fewer members are dropped.
        #[arg(long, default_value_t = 2)]
        min_group_size: usize,

        /// Output format: `json` (default) or `md`.
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Suggest configuration additions (e.g. external_stubs) based on
    /// existing analysis output. Sub-kinds open the `suggest <kind>`
    /// namespace; `stubs` is currently the only kind.
    Suggest {
        #[command(subcommand)]
        kind: SuggestKind,
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

    /// Install Graphify AI-assistant integrations (agents, skills, commands, MCP)
    /// into ~/.claude and ~/.agents (or project-local) directories.
    /// Build / update the session-context brief consumed by Claude Code
    /// `/session-start` skills and `tn-session-dispatcher` subagents
    /// (FEAT-042). Two subcommands: `brief` consolidates `analysis.json`
    /// from every `[[project]]` into one JSON; `scope` augments that JSON
    /// with `graphify explain` output for an explicit list of files.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    InstallIntegrations {
        /// Install Claude Code artifacts (auto-detected if ~/.claude exists)
        #[arg(long)]
        claude_code: bool,
        /// Install Codex artifacts (auto-detected if ~/.agents/skills exists)
        #[arg(long)]
        codex: bool,
        /// Install to ./.claude (Codex artifacts always global)
        #[arg(long)]
        project_local: bool,
        /// Skip MCP server registration in client configs
        #[arg(long)]
        skip_mcp: bool,
        /// Show what would be done without writing
        #[arg(long)]
        dry_run: bool,
        /// Overwrite existing files with different content
        #[arg(long)]
        force: bool,
        /// Remove manifest-tracked artifacts and MCP entries
        #[arg(long)]
        uninstall: bool,
    },
}

#[derive(Subcommand)]
enum SessionAction {
    /// Consolidate `analysis.json` from each `[[project]]` into a single
    /// session-context JSON. Cache-aware: regenerates only when an
    /// `analysis.json` is newer than the existing brief.
    Brief {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Override `[settings].output` (defaults to `./report`)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Brief output path (default `.claude/session-context-gf.json`)
        #[arg(long, default_value = ".claude/session-context-gf.json")]
        out: PathBuf,

        /// Top N hotspots across all projects (default 10)
        #[arg(long, default_value_t = 10)]
        top: usize,

        /// Days threshold for marking the baseline as stale (default 7)
        #[arg(long = "stale-days", default_value_t = 7)]
        stale_days: i64,

        /// Always regenerate, even when the cache is fresh
        #[arg(long)]
        force: bool,

        /// Exit 0 if the brief is fresh, 2 if stale; never write anything
        #[arg(long)]
        check: bool,
    },

    /// Augment an existing brief with `scope_files[]` + `scope_explains[]`
    /// for an explicit list of files. Pass file paths as comma-separated
    /// `--files` — graphify never reaches into a `tn` task body itself.
    Scope {
        /// Path to graphify.toml config (needed to load each project's graph)
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Comma-separated list of file paths under apps/, packages/, scripts/
        #[arg(long, value_delimiter = ',')]
        files: Vec<String>,

        /// Cap at N files (default 5) — keeps subagent prompt budgets bounded
        #[arg(long, default_value_t = 5)]
        max: usize,

        /// Existing brief to augment (default `.claude/session-context-gf.json`)
        #[arg(long = "in", default_value = ".claude/session-context-gf.json")]
        input: PathBuf,

        /// Optional task identifier (purely informational) recorded as
        /// `scope_task` in the merged brief
        #[arg(long)]
        task: Option<String>,
    },
}

#[derive(Subcommand)]
enum SuggestKind {
    /// Suggest prefixes to add to `[settings].external_stubs` and
    /// `[[project]].external_stubs`. Consumes `graph.json` from each
    /// configured project's output directory; run `graphify run` first.
    Stubs {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output format: `md` (default), `toml`, or `json`. Mutually
        /// exclusive with `--apply`.
        #[arg(long, default_value = "md", conflicts_with = "apply")]
        format: String,

        /// Edit graphify.toml in place, merging suggested prefixes into
        /// existing arrays (preserves comments via `toml_edit`).
        #[arg(long)]
        apply: bool,

        /// Minimum per-project edge weight sum for a prefix to be
        /// suggested. Default: 2.
        #[arg(long, default_value_t = 2)]
        min_edges: u64,

        /// Limit to a single project (suggests only for that project's
        /// per-project block; settings_candidates always empty in this mode).
        #[arg(long)]
        project: Option<String>,
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
            let project_refs: Vec<&ProjectConfig> = cfg.project.iter().collect();
            let workspace = collect_workspace_reexport_graph(
                &project_refs,
                &cfg.settings,
                Some(&out_dir),
                force,
            );
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (graph, _excludes, stats) = run_extract_with_workspace(
                    project,
                    &cfg.settings,
                    Some(&proj_out),
                    force,
                    workspace.as_ref(),
                );
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
            hub_threshold,
            bridge_ratio,
            force,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            let thresholds = resolve_hotspot_thresholds(&cfg, hub_threshold, bridge_ratio);
            let consolidation = resolve_consolidation(&cfg, false);
            let project_refs: Vec<&ProjectConfig> = cfg.project.iter().collect();
            let workspace = collect_workspace_reexport_graph(
                &project_refs,
                &cfg.settings,
                Some(&out_dir),
                force,
            );
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let (graph, _, stats) = run_extract_with_workspace(
                    project,
                    &cfg.settings,
                    Some(&proj_out),
                    force,
                    workspace.as_ref(),
                );
                print_cache_stats(&project.name, &stats);
                let excluded_owned = barrel_exclusion_ids(project, &consolidation);
                let excluded: std::collections::HashSet<&str> =
                    excluded_owned.iter().copied().collect();
                let (mut metrics, communities, cycles_simple) =
                    run_analyze(&graph, &w, &thresholds, &excluded);
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
            hub_threshold,
            bridge_ratio,
            format,
            force,
            ignore_allowlist,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            let thresholds = resolve_hotspot_thresholds(&cfg, hub_threshold, bridge_ratio);
            let formats = resolve_formats(&cfg, format.as_deref());
            let consolidation = resolve_consolidation(&cfg, ignore_allowlist);
            let project_refs: Vec<&ProjectConfig> = cfg.project.iter().collect();
            let workspace = collect_workspace_reexport_graph(
                &project_refs,
                &cfg.settings,
                Some(&out_dir),
                force,
            );
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let pd = run_pipeline_for_project(
                    project,
                    &cfg.settings,
                    &proj_out,
                    &w,
                    &thresholds,
                    &formats,
                    force,
                    &consolidation,
                    workspace.as_ref(),
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
            hub_threshold,
            bridge_ratio,
            force,
            ignore_allowlist,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, None);
            let thresholds = resolve_hotspot_thresholds(&cfg, hub_threshold, bridge_ratio);
            let formats = resolve_formats(&cfg, None);
            let consolidation = resolve_consolidation(&cfg, ignore_allowlist);
            let project_refs: Vec<&ProjectConfig> = cfg.project.iter().collect();
            let workspace = collect_workspace_reexport_graph(
                &project_refs,
                &cfg.settings,
                Some(&out_dir),
                force,
            );
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let pd = run_pipeline_for_project(
                    project,
                    &cfg.settings,
                    &proj_out,
                    &w,
                    &thresholds,
                    &formats,
                    force,
                    &consolidation,
                    workspace.as_ref(),
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
            output,
            max_cycles,
            max_hotspot_score,
            hub_threshold,
            bridge_ratio,
            project,
            json,
            force,
            contracts,
            no_contracts,
            contracts_warnings_as_errors,
            ignore_allowlist,
        } => {
            cmd_check(
                &config,
                output.as_deref(),
                project.as_deref(),
                force,
                CheckLimits {
                    max_cycles,
                    max_hotspot_score,
                },
                hub_threshold,
                bridge_ratio,
                json,
                ContractsMode::from_flags(contracts, no_contracts),
                contracts_warnings_as_errors,
                ignore_allowlist,
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
            no_color,
        } => {
            let cfg = load_config(&config);
            let projects = filter_projects(&cfg, project.as_deref());
            let multi_project = cfg.project.len() > 1;
            let palette = ExplainPalette::new(no_color);
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
                        print_explain_report(&report, &proj.name, multi_project, &palette);
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
            ignore_allowlist,
        } => {
            cmd_diff(
                before.as_deref(),
                after.as_deref(),
                baseline.as_deref(),
                config.as_deref(),
                project.as_deref(),
                output.as_deref(),
                threshold,
                ignore_allowlist,
            );
        }

        Commands::Compare {
            left,
            right,
            left_label,
            right_label,
            config,
            output,
            threshold,
            ignore_allowlist,
        } => {
            cmd_compare(
                &left,
                &right,
                left_label.as_deref(),
                right_label.as_deref(),
                config.as_deref(),
                output.as_deref(),
                threshold,
                ignore_allowlist,
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

        Commands::PrSummary { dir, top } => {
            run_pr_summary(&dir, top);
        }

        Commands::Consolidation {
            config,
            output,
            ignore_allowlist,
            min_group_size,
            format,
        } => {
            cmd_consolidation(
                &config,
                output.as_deref(),
                ignore_allowlist,
                min_group_size,
                &format,
            );
        }

        Commands::Suggest { kind } => match kind {
            SuggestKind::Stubs {
                config,
                format,
                apply,
                min_edges,
                project,
            } => {
                cmd_suggest_stubs(&config, &format, apply, min_edges, project.as_deref());
            }
        },

        Commands::InstallIntegrations {
            claude_code,
            codex,
            project_local,
            skip_mcp,
            dry_run,
            force,
            uninstall,
        } => {
            cmd_install_integrations(
                claude_code,
                codex,
                project_local,
                skip_mcp,
                dry_run,
                force,
                uninstall,
            );
        }

        Commands::Session { action } => match action {
            SessionAction::Brief {
                config,
                output,
                out,
                top,
                stale_days,
                force,
                check,
            } => {
                let cfg = load_config(&config);
                let project_names: Vec<String> =
                    cfg.project.iter().map(|p| p.name.clone()).collect();
                let opts = session::BriefOpts {
                    project_names,
                    output_root: resolve_output(&cfg, output.as_deref()),
                    out_path: out,
                    top,
                    stale_days,
                    force,
                    check,
                };
                match session::run_brief(&opts) {
                    Ok(rc) => std::process::exit(rc),
                    Err(e) => {
                        eprintln!("ERROR: {e}");
                        std::process::exit(1);
                    }
                }
            }
            SessionAction::Scope {
                config,
                files,
                max,
                input,
                task,
            } => {
                let cfg = load_config(&config);
                let projects = filter_projects(&cfg, None);
                let opts = session::ScopeOpts {
                    files,
                    max,
                    in_path: input,
                    task,
                };
                let explain = |file: &str| -> Option<serde_json::Value> {
                    for proj in &projects {
                        let engine = build_query_engine(proj, &cfg.settings);
                        if let Some(report) = engine.explain(file) {
                            return serde_json::to_value(&report).ok();
                        }
                    }
                    None
                };
                match session::run_scope(&opts, explain) {
                    Ok(rc) => std::process::exit(rc),
                    Err(e) => {
                        eprintln!("ERROR: {e}");
                        std::process::exit(1);
                    }
                }
            }
        },
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
# workspace_reexport_graph = true   # multi-project TS fan-out (FEAT-028);
#                                     set to `false` to pin pre-v0.11.0 edges
# external_stubs = ["std", "serde"] # shared prelude stubs; merges with each
#                                     project's own external_stubs (FEAT-034)

[[project]]
name = "my-project"
repo = "./src"
lang = ["python"]           # Options: python, typescript, go, rust, php
local_prefix = "app"        # Leave unset for PHP — PSR-4 from composer.json
                            # provides the namespace prefix structure.

# Optional: declare packages that are intentionally external. Edges to these
# are tagged `ExpectedExternal` instead of `Ambiguous`, so the ambiguity
# metric reflects real extraction noise, not legitimate external imports.
# external_stubs = ["drizzle-orm", "zod", "@repo/types"]

# Optional per-project override for `graphify check` thresholds (issue #14).
# Precedence: `[project.check]` > CLI flag > None. Use this to acknowledge a
# legitimate facade (theme provider, i18n context, shared router hook) without
# relaxing the workspace-wide gate for every sibling project. Typos inside
# `[project.check]` fail the parse, so misspelled keys cannot silently disable
# a gate.
#
# [project.check]
# max_hotspot_score = 0.75
# max_cycles = 0
# # Rationale: pin a 1-3 line comment explaining why this project is an exception.

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

# Optional consolidation allowlist — regex patterns (anchored ^...$) matched
# against the *leaf* symbol name. Matching nodes are treated as intentional
# duplicates: they are excluded from consolidation candidates, hotspot gates,
# and drift output. Absent section = no allowlist.
#
# [consolidation]
# allowlist = [
#   "TokenUsage",
#   "(Guided|SemiGuided|Challenging)Exercise",
#   ".*(Response|Output|Dto)",
# ]
#
# [consolidation.intentional_mirrors]
# TokenUsage = ["ana-service:app.models.tokens", "pkg-types:src.tokens"]
"#;

    let dest = Path::new("graphify.toml");
    if dest.exists() {
        eprintln!("graphify.toml already exists — not overwriting.");
        std::process::exit(1);
    }
    std::fs::write(dest, template).expect("write graphify.toml");
    println!("Created graphify.toml — edit it to point at your repo.");
    println!();
    println!("Next steps:");
    println!("  Install AI assistant integrations (slash commands, agents, skills, MCP):");
    println!("    graphify install-integrations --project-local");
    println!("  Generate your first analysis:");
    println!("    graphify run");
}

// ---------------------------------------------------------------------------
// diff command
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn cmd_diff(
    before: Option<&Path>,
    after: Option<&Path>,
    baseline: Option<&Path>,
    config: Option<&Path>,
    project: Option<&str>,
    output: Option<&Path>,
    threshold: f64,
    ignore_allowlist: bool,
) {
    // Load the config once up front when supplied — it feeds both the
    // consolidation lookup (file-vs-file + baseline-vs-live modes) and
    // the live extraction (baseline-vs-live only).
    let cfg_opt = config.map(load_config);
    let consolidation = cfg_opt
        .as_ref()
        .map(|cfg| resolve_consolidation(cfg, ignore_allowlist));

    let (before_snapshot, after_snapshot) = match (before, after, baseline, cfg_opt.as_ref()) {
        // File-vs-file mode (config is optional; when present it supplies
        // the consolidation allowlist + intentional_mirrors).
        (Some(before_path), Some(after_path), None, _) => {
            let b = load_snapshot(before_path);
            let a = load_snapshot(after_path);
            (b, a)
        }
        // Baseline-vs-live mode
        (None, None, Some(baseline_path), Some(cfg)) => {
            let b = load_snapshot(baseline_path);
            let projects = filter_projects(cfg, project);
            let project_cfg = projects[0];
            let w = resolve_weights(cfg, None);
            let thresholds = resolve_hotspot_thresholds(cfg, None, None);
            let (graph, _, _stats) = run_extract(project_cfg, &cfg.settings, None, false);
            let excluded_owned: Vec<&str> = consolidation
                .as_ref()
                .map(|c| barrel_exclusion_ids(project_cfg, c))
                .unwrap_or_default();
            let excluded: std::collections::HashSet<&str> =
                excluded_owned.iter().copied().collect();
            let (mut metrics, communities, cycles_simple) =
                run_analyze(&graph, &w, &thresholds, &excluded);
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
                        hotspot_type: Some(m.hotspot_type),
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
                allowlisted_symbols: None,
                edges: vec![],
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

    let report = compute_diff_with_config(
        &before_snapshot,
        &after_snapshot,
        threshold,
        consolidation.as_ref(),
    );

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

#[allow(clippy::too_many_arguments)]
fn cmd_compare(
    left: &Path,
    right: &Path,
    left_label: Option<&str>,
    right_label: Option<&str>,
    config: Option<&Path>,
    output: Option<&Path>,
    threshold: f64,
    ignore_allowlist: bool,
) {
    let cfg_opt = config.map(load_config);
    let consolidation = cfg_opt
        .as_ref()
        .map(|cfg| resolve_consolidation(cfg, ignore_allowlist));

    let left_path = resolve_compare_analysis_input(left, "left");
    let right_path = resolve_compare_analysis_input(right, "right");
    let left_label = left_label
        .map(str::to_owned)
        .unwrap_or_else(|| default_compare_label(left, &left_path));
    let right_label = right_label
        .map(str::to_owned)
        .unwrap_or_else(|| default_compare_label(right, &right_path));

    let left_snapshot = load_snapshot(&left_path);
    let right_snapshot = load_snapshot(&right_path);
    let report = compute_diff_with_config(
        &left_snapshot,
        &right_snapshot,
        threshold,
        consolidation.as_ref(),
    );

    let out_dir = output.unwrap_or(Path::new("."));
    std::fs::create_dir_all(out_dir).expect("create output directory");
    write_compare_json(
        &report,
        &left_label,
        &right_label,
        &out_dir.join("compare-report.json"),
    );
    write_compare_markdown(
        &report,
        &left_label,
        &right_label,
        &out_dir.join("compare-report.md"),
    );

    println!("Architecture Compare Report");
    println!("  Left:        {} ({})", left_label, left_path.display());
    println!("  Right:       {} ({})", right_label, right_path.display());
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
        println!("  Right-only:  {} nodes", report.edges.added_nodes.len());
    }
    if !report.edges.removed_nodes.is_empty() {
        println!("  Left-only:   {} nodes", report.edges.removed_nodes.len());
    }
    if !report.hotspots.rising.is_empty() || !report.hotspots.falling.is_empty() {
        println!(
            "  Hotspots:    {} higher on right, {} lower on right",
            report.hotspots.rising.len(),
            report.hotspots.falling.len()
        );
    }
    println!("Written to {}", out_dir.display());
}

fn resolve_compare_analysis_input(input: &Path, side: &str) -> PathBuf {
    if input.is_dir() {
        let analysis_path = input.join("analysis.json");
        if analysis_path.exists() {
            return analysis_path;
        }
        eprintln!(
            "graphify compare: {side} directory '{}' is missing analysis.json (run 'graphify run' first or pass an analysis.json file)",
            input.display()
        );
        std::process::exit(1);
    }

    if input.exists() {
        return input.to_path_buf();
    }

    eprintln!(
        "graphify compare: {side} input '{}' not found (pass an analysis.json file or a directory containing analysis.json)",
        input.display()
    );
    std::process::exit(1);
}

fn default_compare_label(input: &Path, resolved: &Path) -> String {
    let label_path = if input.is_dir() {
        input
    } else if resolved
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "analysis.json")
    {
        resolved.parent().unwrap_or(input)
    } else {
        input
    };

    label_path
        .file_stem()
        .or_else(|| label_path.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| input.display().to_string())
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
            if graphify_core::history::is_trend_snapshot_json(&text) {
                eprintln!(
                    "Error: {} is a trend-format history snapshot, not a full analysis.",
                    path.display()
                );
                eprintln!();
                eprintln!(
                    "History snapshots under `report/<project>/history/` are consumable only by"
                );
                eprintln!("`graphify trend`. `graphify diff` requires a full `analysis.json`.");
                eprintln!();
                eprintln!(
                    "To diff before/after a refactor, copy the current analysis as a baseline"
                );
                eprintln!("before starting the refactor:");
                eprintln!();
                eprintln!("    cp report/<project>/analysis.json report/<project>/baseline.json");
                eprintln!();
                eprintln!("Then, after the refactor:");
                eprintln!();
                eprintln!("    graphify diff --before report/<project>/baseline.json \\");
                eprintln!("                  --after  report/<project>/analysis.json");
            } else {
                eprintln!("Invalid analysis JSON {:?}: {e}", path);
            }
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
    let cfg = match toml::from_str::<Config>(&text) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Invalid config {:?}: {e}", path);
            std::process::exit(1);
        }
    };
    // Fail fast on malformed consolidation regex patterns so the pipeline
    // never runs with a half-valid allowlist. The compiled value is thrown
    // away here — `resolve_consolidation` compiles again downstream. Keeping
    // a separate validation pass keeps `Config` TOML-only (no compiled state).
    let raw: ConsolidationConfigRaw = ConsolidationConfigToml {
        allowlist: cfg.consolidation.allowlist.clone(),
        intentional_mirrors: cfg.consolidation.intentional_mirrors.clone(),
        suppress_barrel_cycles: cfg.consolidation.suppress_barrel_cycles,
    }
    .into();
    if let Err(err) = ConsolidationConfig::compile(raw) {
        eprintln!("Invalid [consolidation] section in {:?}: {err}", path);
        std::process::exit(1);
    }
    // DOC-002: PSR-4 mappings from composer.json already provide the PHP
    // namespace prefix structure; resolver case 7 does not re-apply
    // `local_prefix`, so setting one for a PHP project is silently ignored.
    // Non-fatal warning — changing resolver behavior retroactively would
    // double-prefix any config that does set one.
    for project in &cfg.project {
        let is_php = project.lang.iter().any(|l| l.eq_ignore_ascii_case("php"));
        let has_prefix = project
            .local_prefix
            .as_deref()
            .is_some_and(|p| !p.is_empty());
        if is_php && has_prefix {
            eprintln!(
                "Warning: project '{}' sets local_prefix for a PHP project — \
                 PSR-4 mappings from composer.json should be used instead. \
                 Consider removing local_prefix.",
                project.name
            );
        }
    }
    cfg
}

/// Compiles the `[consolidation]` section into a live [`ConsolidationConfig`].
///
/// `ignore_allowlist`, when true, returns an empty config so downstream stages
/// run as if no allowlist were declared — useful when debugging "why is this
/// symbol missing from the report?".
fn resolve_consolidation(cfg: &Config, ignore_allowlist: bool) -> ConsolidationConfig {
    if ignore_allowlist {
        return ConsolidationConfig::default();
    }
    let raw: ConsolidationConfigRaw = ConsolidationConfigToml {
        allowlist: cfg.consolidation.allowlist.clone(),
        intentional_mirrors: cfg.consolidation.intentional_mirrors.clone(),
        suppress_barrel_cycles: cfg.consolidation.suppress_barrel_cycles,
    }
    .into();
    // Safe: validated during load_config.
    ConsolidationConfig::compile(raw).expect("consolidation config validated by load_config")
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

/// Resolves hotspot classification thresholds.
///
/// Precedence: CLI flag > `[hotspots]` config > [`HotspotThresholds::default`].
fn resolve_hotspot_thresholds(
    cfg: &Config,
    hub_override: Option<usize>,
    bridge_override: Option<f64>,
) -> HotspotThresholds {
    let defaults = HotspotThresholds::default();
    HotspotThresholds {
        hub_threshold: hub_override
            .or(cfg.hotspots.hub_threshold)
            .unwrap_or(defaults.hub_threshold),
        bridge_ratio: bridge_override
            .or(cfg.hotspots.bridge_ratio)
            .unwrap_or(defaults.bridge_ratio),
    }
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
            "php" => Some(Language::Php),
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

/// Build a workspace-wide [`graphify_extract::WorkspaceReExportGraph`] by
/// running a lightweight pre-pass over every configured project.
///
/// FEAT-028 step 5 P2b: the outer project loop collects every project's
/// `ProjectReExportContext` (+ per-project [`graphify_extract::ReExportGraph`])
/// into a single workspace aggregate BEFORE any project's fan-out runs. The
/// fan-out loop at main.rs:1953 then consults this aggregate via
/// [`graphify_extract::ModuleResolver::apply_ts_alias_workspace`] +
/// [`graphify_extract::WorkspaceReExportGraph::resolve_canonical_cross_project`]
/// so consumer-side `@repo/core` imports fan out to the canonical
/// declarations in the sibling project.
///
/// Cost: each project's files are walked twice (here + inside
/// `run_extract_with_workspace`). The extraction cache at
/// `.graphify-cache.json` absorbs the second parse, so the overhead is
/// bounded to file I/O + AST re-hydration.
///
/// Returns `None` when no workspace-aware work is possible — i.e. fewer
/// than 2 projects configured, or zero TypeScript projects. Callers fall
/// back to the single-project `run_extract` path in that case.
fn collect_workspace_reexport_graph(
    projects: &[&ProjectConfig],
    settings: &Settings,
    cache_root: Option<&Path>,
    force: bool,
) -> Option<graphify_extract::WorkspaceReExportGraph> {
    // FEAT-030: explicit opt-out. Absent / `true` keeps the FEAT-028
    // workspace-wide fan-out on; `false` forces the legacy per-project path.
    if settings.workspace_reexport_graph == Some(false) {
        return None;
    }

    if projects.len() < 2 {
        return None;
    }

    let mut has_ts = false;
    let mut ws = graphify_extract::WorkspaceReExportGraph::new();

    for project in projects {
        let languages = parse_languages(&project.lang);
        if !languages.contains(&Language::TypeScript) {
            continue;
        }
        has_ts = true;

        // Per-project cache dir (mirrors the caller's layout).
        let cache_dir = cache_root.map(|root| root.join(&project.name));
        let cache_dir_ref = cache_dir.as_deref();

        let phase = build_phase1_for_workspace(project, settings, cache_dir_ref, force);
        ws.add_project(phase.ctx);
        ws.set_project_graph(project.name.clone(), phase.reexport_graph);
    }

    if !has_ts {
        return None;
    }

    for (module_id, losing_project) in ws.module_collisions().iter().take(5) {
        eprintln!(
            "Warning: workspace module id collision on '{}' (losing project: '{}'). Cross-project \
             fan-out may resolve to the first registering project.",
            module_id, losing_project,
        );
    }

    Some(ws)
}

/// Intermediate output of the workspace pre-pass for one project.
struct WorkspacePhase1 {
    ctx: graphify_extract::ProjectReExportContext,
    reexport_graph: graphify_extract::ReExportGraph,
}

/// Run just enough of the extraction pipeline to produce a
/// [`ProjectReExportContext`] + per-project [`ReExportGraph`] for workspace
/// aggregation. Does NOT build the project's [`CodeGraph`] — that's the job
/// of the second pass inside [`run_extract_with_workspace`].
fn build_phase1_for_workspace(
    project: &ProjectConfig,
    settings: &Settings,
    cache_dir: Option<&Path>,
    force: bool,
) -> WorkspacePhase1 {
    let repo_path = PathBuf::from(&project.repo);
    let languages = parse_languages(&project.lang);

    let extra_owned: Vec<String> = settings.exclude.clone().unwrap_or_default();
    let extra_excludes: Vec<&str> = extra_owned.iter().map(|s| s.as_str()).collect();

    let (effective_local_prefix, _auto) = match project.local_prefix.as_deref() {
        Some(prefix) => (prefix.to_owned(), false),
        None => (
            detect_local_prefix(&repo_path, &languages, &extra_excludes),
            true,
        ),
    };

    let psr4_mappings: Vec<(String, String)> = if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            let mut tmp = graphify_extract::resolver::ModuleResolver::new(&repo_path);
            tmp.load_composer_json(&composer);
            tmp.psr4_mappings().to_vec()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let files = graphify_extract::discover_files_with_psr4(
        &repo_path,
        &languages,
        &effective_local_prefix,
        &extra_excludes,
        &psr4_mappings,
    );

    // Reuse the extraction cache if available — the second-pass
    // `run_extract_with_workspace` call will save an updated cache.
    let cache = match (force, cache_dir) {
        (false, Some(dir)) => {
            let cache_path = dir.join(".graphify-cache.json");
            ExtractionCache::load(&cache_path, &effective_local_prefix)
                .unwrap_or_else(|| ExtractionCache::new(&effective_local_prefix))
        }
        _ => ExtractionCache::new(&effective_local_prefix),
    };

    let python_extractor = PythonExtractor::new();
    let typescript_extractor = TypeScriptExtractor::new();
    let go_extractor = GoExtractor::new();
    let rust_extractor = RustExtractor::new();
    let php_extractor = PhpExtractor::new();

    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    resolver.set_local_prefix(&effective_local_prefix);
    for file in &files {
        resolver.register_module_path(&file.module_name, &file.path, file.is_package);
    }

    if languages.contains(&Language::TypeScript) {
        for file in files.iter().filter(|f| f.language == Language::TypeScript) {
            if let Some(tsconfig) = find_nearest_ancestor_file(&file.path, "tsconfig.json") {
                resolver.load_tsconfig_for_module(&file.module_name, &tsconfig);
            }
        }
    }

    if languages.contains(&Language::Go) {
        let go_mod = repo_path.join("go.mod");
        if go_mod.exists() {
            resolver.load_go_mod(&go_mod);
        }
    }

    if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            resolver.load_composer_json(&composer);
        }
    }

    let repo_path_ref = &repo_path;

    let extraction: Vec<ExtractionResult> = files
        .par_iter()
        .filter_map(|file| {
            let source = match std::fs::read(&file.path) {
                Ok(bytes) => bytes,
                Err(_) => return None,
            };

            let rel_path = file
                .path
                .strip_prefix(repo_path_ref)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .to_string();
            let hash = sha256_hex(&source);

            if let Some(cached) = cache.lookup(&rel_path, &hash) {
                return Some(cached.clone());
            }

            let extractor: &dyn LanguageExtractor = match file.language {
                Language::Python => &python_extractor,
                Language::TypeScript => &typescript_extractor,
                Language::Go => &go_extractor,
                Language::Rust => &rust_extractor,
                Language::Php => &php_extractor,
            };

            Some(extractor.extract_file(&file.path, &source, &file.module_name))
        })
        .collect();

    let mut all_reexports: Vec<graphify_extract::ReExportEntry> = Vec::new();
    for result in extraction {
        all_reexports.extend(result.reexports);
    }

    let package_modules_owned: std::collections::HashSet<String> = files
        .iter()
        .filter(|f| f.is_package)
        .map(|f| f.module_name.clone())
        .collect();

    let resolve_cb = |raw: &str, from_module: &str| -> (String, bool) {
        let is_package = package_modules_owned.contains(from_module);
        let (resolved, is_local, _conf) = resolver.resolve(raw, from_module, is_package);
        (resolved, is_local)
    };
    let reexport_graph = graphify_extract::ReExportGraph::build(&all_reexports, &resolve_cb);

    let ctx = build_project_reexport_context(&project.name, &repo_path, &files, all_reexports);

    WorkspacePhase1 {
        ctx,
        reexport_graph,
    }
}

/// Build a [`ProjectReExportContext`] from the inputs already computed inside
/// `run_extract`.
///
/// Extracted as a free helper (FEAT-028 step 5 P2a) so the outer project loop
/// can materialise every project's context before any project's fan-out runs.
/// Today the single-project `run_extract` call builds its own context inline
/// and doesn't consume the result; P2b will invert this so the outer loop
/// drives the collection and hands a shared `WorkspaceReExportGraph` back in.
///
/// Inputs:
/// - `project_name` — display name from `graphify.toml` (becomes `project_name`
///   on the returned context).
/// - `repo_root` — canonical project repo path (becomes `repo_root`).
/// - `files` — every [`graphify_extract::DiscoveredFile`] the walker produced
///   for this project. `module_paths` is populated by iterating files and
///   calling [`ProjectReExportContext::add_module_path`].
/// - `reexports` — the collected `export … from …` entries for this project.
///   Consumed by value because the caller has no further use for them; the
///   workspace aggregate will eventually clone if a future pass needs to
///   recompute re-export graphs.
fn build_project_reexport_context(
    project_name: &str,
    repo_root: &Path,
    files: &[graphify_extract::DiscoveredFile],
    reexports: Vec<graphify_extract::ReExportEntry>,
) -> graphify_extract::ProjectReExportContext {
    let known_modules: Vec<String> = files.iter().map(|f| f.module_name.clone()).collect();

    let mut ctx = graphify_extract::ProjectReExportContext::new(
        project_name.to_owned(),
        repo_root.to_string_lossy().into_owned(),
        known_modules,
        reexports,
    );

    for file in files {
        ctx.add_module_path(&file.module_name, &file.path, file.is_package);
    }

    ctx
}

fn run_extract(
    project: &ProjectConfig,
    settings: &Settings,
    cache_dir: Option<&Path>,
    force: bool,
) -> (CodeGraph, Vec<String>, CacheStats) {
    run_extract_with_workspace(project, settings, cache_dir, force, None)
}

/// Workspace-aware variant of [`run_extract`].
///
/// When `workspace` is `Some`, the TypeScript fan-out block consults the
/// workspace-wide [`WorkspaceReExportGraph`] for cross-project alias-through-
/// barrel resolution (FEAT-028 step 5 P2b). Non-local barrels (e.g.
/// `@repo/core`) that resolve — via the per-project [`ModuleResolver`]'s
/// tsconfig alias map — into a **sibling** workspace project are walked
/// through that sibling's [`ReExportGraph`] to the canonical declaration,
/// and the emitted `Imports` edge targets the sibling's canonical module id
/// verbatim (option-2 namespacing: stable public ids per project).
///
/// When `workspace` is `None`, behaviour is bit-for-bit identical to the
/// pre-FEAT-028 single-project path.
fn run_extract_with_workspace(
    project: &ProjectConfig,
    settings: &Settings,
    cache_dir: Option<&Path>,
    force: bool,
    workspace: Option<&graphify_extract::WorkspaceReExportGraph>,
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

    // Pre-parse composer.json if PHP is in the project's language list.
    // PSR-4 mappings must be known at walk time so module_names are computed
    // in namespace-space (not path-space).
    let psr4_mappings: Vec<(String, String)> = if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            let mut tmp = graphify_extract::resolver::ModuleResolver::new(&repo_path);
            tmp.load_composer_json(&composer);
            tmp.psr4_mappings().to_vec()
        } else {
            eprintln!(
                "Warning: PHP project at {:?} has no composer.json — PSR-4 resolution \
                 disabled, imports may not resolve to local modules",
                repo_path
            );
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Discover files (PSR-4-aware for PHP; degrades to regular discovery for other languages).
    let files = graphify_extract::discover_files_with_psr4(
        &repo_path,
        &languages,
        &effective_local_prefix,
        &extra_excludes,
        &psr4_mappings,
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
    let php_extractor = PhpExtractor::new();

    // Build resolver.
    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    resolver.set_local_prefix(&effective_local_prefix);
    for file in &files {
        resolver.register_module_path(&file.module_name, &file.path, file.is_package);
    }

    // Load the nearest tsconfig for each TypeScript source file. This handles
    // common `repo=./src` layouts where tsconfig.json lives in a parent dir.
    if languages.contains(&Language::TypeScript) {
        for file in files.iter().filter(|f| f.language == Language::TypeScript) {
            if let Some(tsconfig) = find_nearest_ancestor_file(&file.path, "tsconfig.json") {
                resolver.load_tsconfig_for_module(&file.module_name, &tsconfig);
            }
        }
    }

    // Load go.mod if Go is in the language list.
    if languages.contains(&Language::Go) {
        let go_mod = repo_path.join("go.mod");
        if go_mod.exists() {
            resolver.load_go_mod(&go_mod);
        }
    }

    // Load composer.json if PHP is in the language list.
    if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            resolver.load_composer_json(&composer);
        }
        // Warning already printed above during pre-parse; don't duplicate.
    }

    let repo_path_ref = &repo_path;

    // Extract each file in parallel: read → hash → cache check → parse on miss.
    // Tuple: `(rel_path, hash, module_name, result, was_hit)` — module_name
    // is kept so the post-extraction sequential pass can register per-file
    // artifacts (FEAT-031 `use_aliases`) against the resolver.
    let extraction_with_meta: Vec<(String, String, String, ExtractionResult, bool)> = files
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
                return Some((
                    rel_path,
                    hash,
                    file.module_name.clone(),
                    cached.clone(),
                    true,
                ));
            }

            // Cache miss: parse with tree-sitter.
            let extractor: &dyn LanguageExtractor = match file.language {
                Language::Python => &python_extractor,
                Language::TypeScript => &typescript_extractor,
                Language::Go => &go_extractor,
                Language::Rust => &rust_extractor,
                Language::Php => &php_extractor,
            };

            let result = extractor.extract_file(&file.path, &source, &file.module_name);
            Some((rel_path, hash, file.module_name.clone(), result, false))
        })
        .collect();

    // Build new cache from extraction results and count stats.
    let mut new_cache = ExtractionCache::new(&effective_local_prefix);
    let mut results: Vec<(String, ExtractionResult)> =
        Vec::with_capacity(extraction_with_meta.len());

    for (rel_path, hash, module_name, result, was_hit) in extraction_with_meta {
        if was_hit {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
        new_cache.insert(rel_path, hash, result.clone());
        results.push((module_name, result));
    }

    // Count evictions: old cache entries whose paths aren't in the current discovered file set.
    let current_paths: HashSet<String> = new_cache.paths().cloned().collect();
    stats.evicted = cache
        .paths()
        .filter(|p| !current_paths.contains(*p))
        .count();

    // Merge results sequentially into graph. FEAT-031: also register per-file
    // `use_aliases` on the resolver so case-9 fallback can rewrite scoped and
    // bare-name call targets to their canonical local ids.
    let mut all_nodes = Vec::new();
    let mut all_raw_edges: Vec<(String, String, graphify_core::types::Edge)> = Vec::new();
    let mut all_reexports: Vec<graphify_extract::ReExportEntry> = Vec::new();
    let mut all_named_imports: Vec<graphify_extract::NamedImportEntry> = Vec::new();
    for (module_name, result) in results {
        if !result.use_aliases.is_empty() {
            resolver.register_use_aliases(&module_name, &result.use_aliases);
        }
        all_nodes.extend(result.nodes);
        all_raw_edges.extend(result.edges);
        all_reexports.extend(result.reexports);
        all_named_imports.extend(result.named_imports);
    }

    // Build a set of module names that are package entry points (__init__.py,
    // index.ts), so the resolver knows not to pop the leaf for relative imports.
    let package_modules: HashSet<&str> = files
        .iter()
        .filter(|f| f.is_package)
        .map(|f| f.module_name.as_str())
        .collect();

    // -----------------------------------------------------------------------
    // FEAT-021 Part B: barrel collapse for TypeScript re-exports.
    //
    // Build a project-wide re-export graph from the collected `export ... from`
    // statements, then walk each barrel-scoped symbol node back to its
    // canonical declaration. Nodes that resolve to a canonical upstream
    // sibling are folded in: the canonical node absorbs the barrel id into
    // its `alternative_paths`, and the barrel node is dropped so it no
    // longer inflates hotspot / fan-in metrics.
    //
    // Edge-level rewrite (rewriting import-path targets to canonical symbol
    // ids) is intentionally not wired here — Imports edges key on module
    // paths, not symbol names, so the incremental win comes from the node
    // dedupe alone. Expanding to edge rewrite is tracked as a follow-up.
    //
    // FEAT-028 step 5 P2a: the per-project context is now materialised via
    // `build_project_reexport_context` so the outer call sites can collect
    // every project's context into a single workspace-wide
    // `WorkspaceReExportGraph` in P2b. Within `run_extract` the fan-out still
    // consumes only the project's own `ReExportGraph` (single-project
    // workspace), keeping v1 behaviour bit-for-bit identical.
    // -----------------------------------------------------------------------
    let mut barrel_to_canonical: HashMap<String, String> = HashMap::new();
    let mut canonical_to_alt_paths: HashMap<String, Vec<String>> = HashMap::new();
    // FEAT-026: edges synthesized from named-import specifier fan-out. We
    // emit these after the main `all_raw_edges` resolver loop, tagged so the
    // resolver pass knows the target is already resolved to a canonical
    // module id (no re-resolve needed).
    let mut named_import_edges: Vec<(String, String, graphify_core::types::Edge)> = Vec::new();
    let has_ts_reexport_work = (!all_reexports.is_empty() || !all_named_imports.is_empty())
        && languages.contains(&Language::TypeScript);
    if has_ts_reexport_work {
        let package_modules_owned: HashSet<String> =
            package_modules.iter().map(|s| (*s).to_owned()).collect();
        let reexport_resolver = &resolver;
        let resolve_cb = |raw: &str, from_module: &str| -> (String, bool) {
            let is_package = package_modules_owned.contains(from_module);
            let (resolved, is_local, _conf) =
                reexport_resolver.resolve(raw, from_module, is_package);
            (resolved, is_local)
        };
        let reexport_graph = graphify_extract::ReExportGraph::build(&all_reexports, &resolve_cb);

        // FEAT-028 step 5 P2a: surface the per-project context so the outer
        // loop can (in P2b) aggregate every project's context into a
        // workspace-wide graph. `_ctx` is built here solely to exercise the
        // helper; at the single-project scope it's unused by the fan-out.
        let _ctx = build_project_reexport_context(
            &project.name,
            &repo_path,
            &files,
            all_reexports.clone(),
        );

        let is_local_fn = |module: &str| reexport_resolver.is_local_module(module);

        for entry in &all_reexports {
            for spec in &entry.specs {
                let barrel_id = format!("{}.{}", entry.from_module, spec.local_name);
                let outcome = reexport_graph.resolve_canonical(
                    &entry.from_module,
                    &spec.local_name,
                    &is_local_fn,
                );
                match outcome {
                    graphify_extract::CanonicalResolution::Canonical {
                        canonical_id,
                        alternative_paths,
                        ..
                    } => {
                        if canonical_id == barrel_id {
                            continue;
                        }
                        // Register the barrel → canonical rewrite; accumulate
                        // every barrel-scoped id reached along the walk on
                        // the canonical's alternative_paths list.
                        barrel_to_canonical
                            .entry(barrel_id.clone())
                            .or_insert_with(|| canonical_id.clone());
                        let alts = canonical_to_alt_paths.entry(canonical_id).or_default();
                        for p in alternative_paths {
                            if !alts.contains(&p) {
                                alts.push(p);
                            }
                        }
                        if !alts.contains(&barrel_id) {
                            alts.push(barrel_id);
                        }
                    }
                    graphify_extract::CanonicalResolution::Cycle {
                        at_module, at_name, ..
                    } => {
                        eprintln!(
                            "Warning: [{}] cyclic re-export detected for symbol '{}' from '{}' (chain revisits {}.{}); leaving barrel nodes in place.",
                            project.name,
                            spec.local_name,
                            entry.from_module,
                            at_module,
                            at_name,
                        );
                    }
                    graphify_extract::CanonicalResolution::Unresolved {
                        last_module,
                        last_name,
                        ..
                    } => {
                        // Chain terminated at a non-local module (external
                        // package, missing file). Keep the barrel symbol
                        // node as-is; downstream confidence handling will
                        // take over for import edges whose target points
                        // at the unresolved external.
                        //
                        // FEAT-025: emit a stderr diagnostic for visibility.
                        // No confidence downgrade at the node level — the
                        // node itself wasn't created by the re-export walk,
                        // and edge-level confidence is already capped via
                        // the non-local rule in the resolver step below.
                        eprintln!(
                            "Info: [{}] unresolved re-export chain for symbol '{}' from '{}' (ends at {}.{}); leaving barrel node in place.",
                            project.name,
                            spec.local_name,
                            entry.from_module,
                            last_module,
                            last_name,
                        );
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // FEAT-026: fan module-level `Imports` edges out to canonical modules.
        //
        // For each captured `import { X, Y } from '…'` statement:
        //   1. Resolve the raw target path to a module id.
        //   2. For each upstream specifier name, walk the re-export graph
        //      from that module to the canonical declaration module.
        //   3. Emit one `Imports` edge per canonical module (deduping via
        //      `CodeGraph::add_edge`'s weight-increment path — N specifiers
        //      landing on the same canonical module collapse to one edge
        //      with weight N).
        //   4. `Unresolved` / `Cycle` fall back to the barrel target so we
        //      never drop an import; behaviour matches the pre-FEAT-026
        //      single-edge contract.
        // -------------------------------------------------------------------
        for entry in &all_named_imports {
            let from_is_package = package_modules_owned.contains(&entry.from_module);
            let (barrel_module, barrel_is_local, _conf) =
                reexport_resolver.resolve(&entry.raw_target, &entry.from_module, from_is_package);

            // If the barrel isn't a local module (e.g. `react`,
            // `@reduxjs/toolkit`), there's no **per-project** re-export graph
            // to walk. FEAT-028 step 5 P2b: before falling back to the raw
            // alias edge, try the workspace-wide resolver — if the tsconfig
            // alias points at a sibling `[[project]]`, fan out to each
            // specifier's canonical declaration in that sibling.
            if !barrel_is_local {
                if let Some(ws) = workspace {
                    if let Some(target) = reexport_resolver.apply_ts_alias_workspace(
                        &entry.raw_target,
                        &entry.from_module,
                        ws,
                    ) {
                        // Register the dropped alias id on every canonical
                        // node reached so `alternative_paths` carries the
                        // original import string (parity with the per-
                        // project collapse at line 1819).
                        let mut fanned_out_any = false;
                        for spec_name in &entry.specs {
                            let outcome = ws.resolve_canonical_cross_project(
                                &target.project,
                                &target.module_id,
                                spec_name,
                            );
                            match outcome {
                                graphify_extract::CrossProjectResolution::Canonical {
                                    module: canonical_module,
                                    ..
                                } => {
                                    named_import_edges.push((
                                        entry.from_module.clone(),
                                        canonical_module,
                                        graphify_core::types::Edge::imports(entry.line),
                                    ));
                                    fanned_out_any = true;
                                }
                                graphify_extract::CrossProjectResolution::Unresolved { .. }
                                | graphify_extract::CrossProjectResolution::Cycle { .. } => {
                                    // Chain terminated outside the workspace
                                    // or cycled — land a single edge at the
                                    // sibling's barrel module so the import
                                    // is still represented (mirrors the
                                    // per-project FEAT-026 fallback). Using
                                    // the sibling's `target.module_id` keeps
                                    // the edge inside the workspace graph.
                                    named_import_edges.push((
                                        entry.from_module.clone(),
                                        target.module_id.clone(),
                                        graphify_core::types::Edge::imports(entry.line),
                                    ));
                                    fanned_out_any = true;
                                }
                            }
                        }
                        if !fanned_out_any {
                            // Defensive — zero specs, shouldn't happen but
                            // keeps the statement-to-edge guarantee.
                            named_import_edges.push((
                                entry.from_module.clone(),
                                target.module_id.clone(),
                                graphify_core::types::Edge::imports(entry.line),
                            ));
                        }
                        // Accumulate alternative_paths on canonicals so the
                        // consumer's dropped `@repo/core` alias is remembered.
                        //
                        // NOTE: we add the raw alias to `canonical_to_alt_paths`
                        // keyed by each canonical module that will actually
                        // appear as a node in THIS project's graph. In
                        // practice, cross-project canonicals live in the
                        // sibling's graph (the canonical node isn't part of
                        // THIS project's node set), so the alt-paths entry
                        // here is a no-op for the current writer — it only
                        // matters for future cross-project node dedupe. Kept
                        // as a forward-compatibility hook; safe because the
                        // alt-paths fan-out loop only rewrites nodes that
                        // exist in `all_nodes`.
                        let alts = canonical_to_alt_paths
                            .entry(target.module_id.clone())
                            .or_default();
                        if !alts.contains(&entry.raw_target) {
                            alts.push(entry.raw_target.clone());
                        }
                        continue;
                    }
                }

                named_import_edges.push((
                    entry.from_module.clone(),
                    entry.raw_target.clone(),
                    graphify_core::types::Edge::imports(entry.line),
                ));
                continue;
            }

            // Local barrel: fan out per specifier.
            let mut fanned_out_any = false;
            for spec_name in &entry.specs {
                let outcome =
                    reexport_graph.resolve_canonical(&barrel_module, spec_name, &is_local_fn);
                match outcome {
                    graphify_extract::CanonicalResolution::Canonical {
                        canonical_module, ..
                    } => {
                        named_import_edges.push((
                            entry.from_module.clone(),
                            canonical_module,
                            graphify_core::types::Edge::imports(entry.line),
                        ));
                        fanned_out_any = true;
                    }
                    graphify_extract::CanonicalResolution::Unresolved { .. }
                    | graphify_extract::CanonicalResolution::Cycle { .. } => {
                        // Chain didn't complete — fall back to a single edge
                        // pointing at the barrel module. `CodeGraph`'s
                        // weight-increment path dedupes when multiple specs
                        // from the same statement all fail.
                        named_import_edges.push((
                            entry.from_module.clone(),
                            barrel_module.clone(),
                            graphify_core::types::Edge::imports(entry.line),
                        ));
                        fanned_out_any = true;
                    }
                }
            }

            // Defensive: if the statement produced no outcome at all, keep
            // the pre-FEAT-026 single barrel edge.
            if !fanned_out_any {
                named_import_edges.push((
                    entry.from_module.clone(),
                    barrel_module,
                    graphify_core::types::Edge::imports(entry.line),
                ));
            }
        }
    } else if !all_named_imports.is_empty() {
        // FEAT-026 safety net: non-TS projects don't populate
        // `all_named_imports` today, but if they ever do we still need to
        // land one edge per statement so we don't drop imports.
        for entry in &all_named_imports {
            named_import_edges.push((
                entry.from_module.clone(),
                entry.raw_target.clone(),
                graphify_core::types::Edge::imports(entry.line),
            ));
        }
    }

    // FEAT-026: hand the synthesized named-import edges off to the main
    // edge-resolution loop. Canonical targets (already dot-notation module
    // ids known to the resolver) resolve to themselves at confidence 1.0;
    // fallback barrel-target edges (when the chain was unresolvable) go
    // through the normal resolver path unchanged.
    if !named_import_edges.is_empty() {
        all_raw_edges.extend(named_import_edges);
    }

    // Collapse barrel symbol nodes into their canonical counterparts.
    // Order: first dedupe the node list (drop any barrel id that maps to a
    // canonical), then fan out `alternative_paths` onto canonical nodes.
    // Also rewrites the raw edges' source / target strings so the
    // subsequent edge-merge step sees canonical ids only.
    if !barrel_to_canonical.is_empty() {
        all_nodes.retain(|n| !barrel_to_canonical.contains_key(&n.id));
        for node in &mut all_nodes {
            if let Some(alts) = canonical_to_alt_paths.get(&node.id) {
                for p in alts {
                    if &node.id != p && !node.alternative_paths.contains(p) {
                        node.alternative_paths.push(p.clone());
                    }
                }
            }
        }
        for (src_id, raw_target, _edge) in all_raw_edges.iter_mut() {
            if let Some(canonical) = barrel_to_canonical.get(src_id) {
                *src_id = canonical.clone();
            }
            if let Some(canonical) = barrel_to_canonical.get(raw_target) {
                *raw_target = canonical.clone();
            }
        }
    }

    // BUG-018: Register symbol-level `Defines` targets as known local modules
    // so Calls edges that resolve to them (via FEAT-031's `use`-alias fallback,
    // or directly by name) keep their extractor confidence instead of being
    // downgraded to Ambiguous/0.5 by the non-local rule. Runs after barrel
    // collapse so only canonical ids are registered.
    for (_src_id, raw_target, edge) in &all_raw_edges {
        if edge.kind == graphify_core::types::EdgeKind::Defines {
            resolver.register_module(raw_target);
        }
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

    // Compile external-stub prefixes (issue #12): edges resolving to these
    // packages are tagged `ExpectedExternal` instead of `Ambiguous`.
    // FEAT-034: `[settings].external_stubs` contributes a shared list that
    // merges with the project-level array. Overlap is harmless — the matcher
    // sorts by length and dedupes identical prefixes.
    let external_stubs = ExternalStubs::new(
        settings
            .external_stubs
            .iter()
            .flatten()
            .chain(project.external_stubs.iter())
            .cloned(),
    );

    // Resolve edges and add them.
    for (src_id, raw_target, mut edge) in all_raw_edges {
        if edge.kind == graphify_core::types::EdgeKind::Defines {
            graph.add_edge(&src_id, &raw_target, edge);
            continue;
        }

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

        // Step 3: Downgrade edges to non-local targets — unless the target
        // matches an `external_stubs` prefix, in which case it's classified
        // as `ExpectedExternal` so it doesn't inflate the ambiguity metric.
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

    // Save updated cache.
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(dir).ok();
        new_cache.save(&dir.join(".graphify-cache.json"));
    }

    (graph, extra_owned, stats)
}

fn find_nearest_ancestor_file(start_path: &Path, file_name: &str) -> Option<PathBuf> {
    let mut current = if start_path.is_dir() {
        Some(start_path)
    } else {
        start_path.parent()
    };

    while let Some(dir) = current {
        let candidate = dir.join(file_name);
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }

    None
}

// ---------------------------------------------------------------------------
// Analysis pipeline
// ---------------------------------------------------------------------------

type AnalysisResult = (
    Vec<graphify_core::metrics::NodeMetrics>,
    Vec<graphify_core::community::Community>,
    Vec<Cycle>,
);

fn run_analyze(
    graph: &CodeGraph,
    weights: &ScoringWeights,
    thresholds: &HotspotThresholds,
    excluded_cycle_nodes: &std::collections::HashSet<&str>,
) -> AnalysisResult {
    let metrics = compute_metrics_with_thresholds(graph, weights, thresholds);
    let communities = detect_communities(graph);
    let sccs = if excluded_cycle_nodes.is_empty() {
        find_sccs(graph)
    } else {
        find_sccs_excluding(graph, excluded_cycle_nodes)
    };

    // Build simple cycles from SCCs (capped at 500).
    let simple_cycles = if excluded_cycle_nodes.is_empty() {
        find_simple_cycles(graph, 500)
    } else {
        find_simple_cycles_excluding(graph, 500, excluded_cycle_nodes)
    };

    // Convert to Cycle (Vec<String>) — already the right type.
    // Also include SCC node_ids as cycles for completeness when simple_cycles is empty.
    let cycles: Vec<Cycle> = if !simple_cycles.is_empty() {
        simple_cycles
    } else {
        sccs.into_iter().map(|g| g.node_ids).collect()
    };

    (metrics, communities, cycles)
}

/// Builds the set of node IDs to exclude from cycle detection for a project
/// given the project's `local_prefix` and the compiled consolidation config
/// (BUG-015 `suppress_barrel_cycles`).
///
/// Returns an empty set unless:
/// - `suppress_barrel_cycles = true` in `[consolidation]`,
/// - the project declares a `local_prefix`, AND
/// - that prefix (as a leaf symbol) is matched by the allowlist.
///
/// When all three hold, the barrel node ID (equal to `local_prefix`) is
/// returned as a single-element set. The string is owned here so the call
/// site can assemble a `HashSet<&str>` that borrows from a stable source.
fn barrel_exclusion_ids<'a>(
    project: &'a ProjectConfig,
    consolidation: &ConsolidationConfig,
) -> Vec<&'a str> {
    if !consolidation.suppress_barrel_cycles() {
        return Vec::new();
    }
    let prefix = match project.local_prefix.as_deref() {
        Some(p) if !p.is_empty() => p,
        _ => return Vec::new(),
    };
    if consolidation.matches(prefix) {
        vec![prefix]
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Single-project pipeline helper
// ---------------------------------------------------------------------------

/// Runs the full pipeline for a single project: extract → analyze → write outputs.
///
/// Returns a `ProjectData` struct for use in cross-project summaries.
#[allow(clippy::too_many_arguments)]
fn run_pipeline_for_project(
    project: &ProjectConfig,
    settings: &Settings,
    proj_out: &Path,
    weights: &ScoringWeights,
    thresholds: &HotspotThresholds,
    formats: &[String],
    force: bool,
    consolidation: &ConsolidationConfig,
    workspace: Option<&graphify_extract::WorkspaceReExportGraph>,
) -> ProjectData {
    let (graph, _, stats) =
        run_extract_with_workspace(project, settings, Some(proj_out), force, workspace);
    print_cache_stats(&project.name, &stats);
    let excluded_owned = barrel_exclusion_ids(project, consolidation);
    let excluded: std::collections::HashSet<&str> = excluded_owned.iter().copied().collect();
    let (mut metrics, communities, cycles_simple) =
        run_analyze(&graph, weights, thresholds, &excluded);
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
        consolidation,
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

#[derive(Debug, Clone, Copy)]
enum ContractsMode {
    Auto,
    On,
    Off,
}

impl ContractsMode {
    fn from_flags(contracts: bool, no_contracts: bool) -> Self {
        match (contracts, no_contracts) {
            (true, _) => ContractsMode::On,
            (_, true) => ContractsMode::Off,
            _ => ContractsMode::Auto,
        }
    }
}

/// Merges CLI-level `CheckLimits` with an optional `[project.check]` override
/// from `graphify.toml` (issue #14). Precedence per field:
/// `[project.check]` > CLI flag > None (no gate for that dimension).
///
/// The project-wins rule was chosen against the issue's literal "CLI usually
/// overrides config" text because the motivating use case (workspace CI runs
/// `graphify check --max-hotspot-score 0.70` and needs one project to pass at
/// 0.75 via its TOML override) only works when the narrower scope wins.
/// `[project.check]` is an intentional, code-reviewed exception — not a
/// transient debug toggle — so it shadows the workspace default the same way
/// `tsconfig` per-project overrides shadow a root config.
///
/// Kept as a free function so the precedence rule has exactly one test target
/// and `cmd_check` can build the per-project limits map without cloning the
/// whole project slice.
fn effective_limits(cli: &CheckLimits, project: Option<&ProjectCheck>) -> CheckLimits {
    let project = project.cloned().unwrap_or_default();
    CheckLimits {
        max_cycles: project.max_cycles.or(cli.max_cycles),
        max_hotspot_score: project.max_hotspot_score.or(cli.max_hotspot_score),
    }
}

fn evaluate_quality_gates(
    graph: &CodeGraph,
    metrics: &[graphify_core::metrics::NodeMetrics],
    communities: &[graphify_core::community::Community],
    cycles: &[Cycle],
    limits: &CheckLimits,
    consolidation: &ConsolidationConfig,
) -> (ProjectCheckSummary, Vec<CheckViolation>) {
    // Deterministic tie-break: highest score wins; ties broken by smaller id.
    // Nodes matched by the consolidation allowlist are excluded from the
    // hotspot gate — they are intentional mirrors and should not drive
    // `max_hotspot_score` failures. Cycle count is unaffected.
    let top_hotspot = metrics
        .iter()
        .filter(|m| !consolidation.matches(&m.id))
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.id.cmp(&a.id))
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

fn build_check_report(
    projects: Vec<ProjectCheckResult>,
    contracts: Option<graphify_report::ContractCheckResult>,
) -> CheckReport {
    let mut violations: usize = projects.iter().map(|p| p.violations.len()).sum();
    if let Some(c) = &contracts {
        violations += c.error_count;
    }
    let ok_projects = projects.iter().all(|p| p.ok);
    let ok_contracts = contracts.as_ref().map(|c| c.ok).unwrap_or(true);
    CheckReport {
        ok: ok_projects && ok_contracts,
        violations,
        projects,
        contracts,
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

#[allow(clippy::too_many_arguments)]
fn cmd_check(
    config_path: &Path,
    output_override: Option<&Path>,
    project_filter: Option<&str>,
    force: bool,
    limits: CheckLimits,
    hub_threshold: Option<usize>,
    bridge_ratio: Option<f64>,
    json: bool,
    contracts_mode: ContractsMode,
    contracts_warnings_as_errors: bool,
    ignore_allowlist: bool,
) {
    let cfg = load_config(config_path);
    let projects = filter_projects(&cfg, project_filter);
    let out_dir = resolve_output(&cfg, output_override);
    let thresholds = resolve_hotspot_thresholds(&cfg, hub_threshold, bridge_ratio);
    let consolidation = resolve_consolidation(&cfg, ignore_allowlist);
    let workspace =
        collect_workspace_reexport_graph(&projects, &cfg.settings, Some(&out_dir), force);
    let mut analyzed_projects = Vec::new();

    for project in &projects {
        let (graph, _excludes, stats) =
            run_extract_with_workspace(project, &cfg.settings, None, force, workspace.as_ref());
        print_cache_stats(&project.name, &stats);
        let excluded_owned = barrel_exclusion_ids(project, &consolidation);
        let excluded: std::collections::HashSet<&str> = excluded_owned.iter().copied().collect();
        let (metrics, communities, cycles) =
            run_analyze(&graph, &ScoringWeights::default(), &thresholds, &excluded);
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

    // Build per-project effective limits map. Separate from the loop so the
    // borrow on `cfg.project` is released before `analyzed_projects` is moved.
    let project_overrides: HashMap<String, CheckLimits> = cfg
        .project
        .iter()
        .map(|p| (p.name.clone(), effective_limits(&limits, p.check.as_ref())))
        .collect();

    let mut results = Vec::new();
    for project in analyzed_projects {
        let project_limits = project_overrides
            .get(&project.name)
            .cloned()
            .unwrap_or_else(|| limits.clone());
        let (summary, violations) = evaluate_quality_gates(
            &project.graph,
            &project.metrics,
            &project.communities,
            &project.cycles,
            &project_limits,
            &consolidation,
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
            project_limits,
            policy_result,
            violations,
        ));
    }

    let contracts = run_contract_gate(
        &cfg,
        config_path,
        contracts_mode,
        contracts_warnings_as_errors,
    );

    let report = build_check_report(results, contracts);

    // Write the unified check-report.json to every project's output directory
    // alongside analysis.json / drift-report.json. Each project dir receives a
    // copy of the full report so downstream consumers (e.g. `pr-summary`) can
    // read it from any single project directory. Additive: the stdout behavior
    // under --json below is preserved.
    let serialized = serde_json::to_string_pretty(&report).expect("serialize CheckReport as JSON");
    for project_result in &report.projects {
        let proj_out = out_dir.join(&project_result.name);
        if let Err(err) = std::fs::create_dir_all(&proj_out) {
            eprintln!(
                "warning: failed to create output directory {}: {}",
                proj_out.display(),
                err
            );
            continue;
        }
        let path = proj_out.join("check-report.json");
        if let Err(err) = std::fs::write(&path, &serialized) {
            eprintln!(
                "warning: failed to write check-report.json to {}: {}",
                path.display(),
                err
            );
        }
    }

    if json {
        println!("{}", serialized);
    } else {
        print_check_report(&report);
        if let Some(contracts) = &report.contracts {
            print_contract_report(contracts);
        }
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
// Contract drift gate
// ---------------------------------------------------------------------------

fn run_contract_gate(
    cfg: &Config,
    config_path: &Path,
    mode: ContractsMode,
    warnings_as_errors: bool,
) -> Option<graphify_report::ContractCheckResult> {
    let enabled = match mode {
        ContractsMode::Off => false,
        ContractsMode::On => true,
        ContractsMode::Auto => !cfg.contract.pairs.is_empty(),
    };
    if !enabled {
        return None;
    }
    if cfg.contract.pairs.is_empty() {
        eprintln!("warning: --contracts requested but no [[contract.pair]] declared; skipping");
        return None;
    }

    let global = build_global_contract_config(&cfg.contract, warnings_as_errors);
    let workspace_root = config_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let mut pair_results: Vec<graphify_report::ContractPairResult> = Vec::new();

    for pair in &cfg.contract.pairs {
        let pair_cfg = PairConfig {
            ignore_orm: pair
                .ignore
                .as_ref()
                .map(|i| i.orm.clone())
                .unwrap_or_default(),
            ignore_ts: pair
                .ignore
                .as_ref()
                .map(|i| i.ts.clone())
                .unwrap_or_default(),
            field_aliases: pair
                .field_alias
                .iter()
                .map(|a| FieldAlias {
                    orm: a.orm.clone(),
                    ts: a.ts.clone(),
                })
                .collect(),
            relation_aliases: pair
                .relation_alias
                .iter()
                .map(|a| FieldAlias {
                    orm: a.orm.clone(),
                    ts: a.ts.clone(),
                })
                .collect(),
        };

        let orm_path = workspace_root.join(&pair.orm.file);
        let ts_path = workspace_root.join(&pair.ts.file);

        let orm_source = std::fs::read_to_string(&orm_path).unwrap_or_else(|e| {
            eprintln!(
                "contract pair '{}': cannot read ORM file {:?}: {e}",
                pair.name, orm_path
            );
            std::process::exit(1);
        });
        let ts_source = std::fs::read_to_string(&ts_path).unwrap_or_else(|e| {
            eprintln!(
                "contract pair '{}': cannot read TS file {:?}: {e}",
                pair.name, ts_path
            );
            std::process::exit(1);
        });

        if pair.orm.source != "drizzle" {
            eprintln!(
                "contract pair '{}': orm.source '{}' not supported in v1 (use 'drizzle')",
                pair.name, pair.orm.source
            );
            std::process::exit(1);
        }

        let orm_contract = graphify_extract::extract_drizzle_contract_at(
            &orm_source,
            &pair.orm.table,
            orm_path.clone(),
        )
        .unwrap_or_else(|e| {
            eprintln!("contract pair '{}': Drizzle parse error: {e}", pair.name);
            std::process::exit(1);
        });

        let ts_contract =
            graphify_extract::extract_ts_contract_at(&ts_source, &pair.ts.export, ts_path.clone())
                .unwrap_or_else(|e| {
                    eprintln!("contract pair '{}': TS parse error: {e}", pair.name);
                    std::process::exit(1);
                });

        let cmp = graphify_core::contract::compare_contracts(
            &orm_contract,
            &ts_contract,
            &pair_cfg,
            &global,
        );

        let entries: Vec<graphify_report::ViolationEntry> = cmp
            .violations
            .into_iter()
            .map(|v| graphify_report::ViolationEntry {
                severity: v.severity(global.unmapped_type_severity),
                violation: v,
            })
            .collect();

        pair_results.push(graphify_report::ContractPairResult {
            name: pair.name.clone(),
            orm: graphify_report::ContractSideInfo {
                file: orm_path,
                symbol: pair.orm.table.clone(),
                line: 1, // v1 limitation: pair-level line is hardcoded
            },
            ts: graphify_report::ContractSideInfo {
                file: ts_path,
                symbol: pair.ts.export.clone(),
                line: 1,
            },
            violations: entries,
        });
    }

    Some(graphify_report::build_contract_check_result(
        pair_results,
        global.unmapped_type_severity,
    ))
}

fn build_global_contract_config(
    cfg: &ContractConfigRaw,
    warnings_as_errors: bool,
) -> GlobalContractConfig {
    let unmapped_severity = if warnings_as_errors {
        Severity::Error
    } else {
        match cfg.unmapped_type_severity.as_str() {
            "error" => Severity::Error,
            _ => Severity::Warning,
        }
    };
    let case_rule = match cfg.case_rule.as_str() {
        "exact" => CaseRule::Exact,
        _ => CaseRule::SnakeCamel,
    };
    let overrides = cfg
        .type_map
        .iter()
        .map(|(k, v)| (k.clone(), parse_type_override(v)))
        .collect();
    GlobalContractConfig {
        case_rule,
        type_map_overrides: overrides,
        unmapped_type_severity: unmapped_severity,
    }
}

fn parse_type_override(spec: &str) -> FieldType {
    match spec {
        "string" => FieldType::Primitive {
            value: PrimitiveType::String,
        },
        "number" => FieldType::Primitive {
            value: PrimitiveType::Number,
        },
        "boolean" => FieldType::Primitive {
            value: PrimitiveType::Boolean,
        },
        "date" => FieldType::Primitive {
            value: PrimitiveType::Date,
        },
        "unknown" => FieldType::Primitive {
            value: PrimitiveType::Unknown,
        },
        other => FieldType::Named {
            value: other.to_string(),
        },
    }
}

fn print_contract_report(c: &graphify_report::ContractCheckResult) {
    let status = if c.ok { "OK" } else { "FAILED" };
    println!(
        "[contracts] {} (errors={}, warnings={}, pairs={})",
        status,
        c.error_count,
        c.warning_count,
        c.pairs.len()
    );
    for pair in &c.pairs {
        if pair.violations.is_empty() {
            continue;
        }
        println!(
            "  pair: {} ({}::{} <-> {}::{})",
            pair.name,
            pair.orm.file.display(),
            pair.orm.symbol,
            pair.ts.file.display(),
            pair.ts.symbol,
        );
        for v in &pair.violations {
            let sev = match v.severity {
                Severity::Error => "error  ",
                Severity::Warning => "warning",
            };
            println!("    {sev} {:?}", v.violation);
        }
    }
}

// ---------------------------------------------------------------------------
// Query engine helpers
// ---------------------------------------------------------------------------

fn build_query_engine(project: &ProjectConfig, settings: &Settings) -> QueryEngine {
    let (graph, _, _stats) = run_extract(project, settings, None, false);
    let w = ScoringWeights::default();
    // Query engine runs without the consolidation config in scope, so barrel
    // suppression is not applied here — query commands see the raw cycle set.
    let empty: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let (mut metrics, communities, _cycles_simple) =
        run_analyze(&graph, &w, &HotspotThresholds::default(), &empty);
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

/// Maximum rows shown per edge-kind subsection before `... and N more` kicks in.
const EXPLAIN_MAX_PER_SECTION: usize = 10;

/// Color policy for `print_explain_report` output.
///
/// Honors the `NO_COLOR` environment variable (any non-empty value disables
/// color, matching https://no-color.org convention) and auto-disables when
/// stdout is not a TTY. `--no-color` forces `enabled = false` unconditionally.
pub struct ExplainPalette {
    enabled: bool,
}

impl ExplainPalette {
    pub fn new(forced_off: bool) -> Self {
        use std::io::IsTerminal;
        let enabled = !forced_off
            && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
            && std::io::stdout().is_terminal();
        Self { enabled }
    }

    /// Construct a palette with colors explicitly on or off (used by tests).
    pub fn fixed(enabled: bool) -> Self {
        Self { enabled }
    }

    fn paint(&self, style: anstyle::Style, text: impl std::fmt::Display) -> String {
        if self.enabled {
            format!("{}{}{}", style.render(), text, anstyle::Reset.render())
        } else {
            text.to_string()
        }
    }
}

fn explain_style_red() -> anstyle::Style {
    anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)))
}
fn explain_style_yellow() -> anstyle::Style {
    anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow)))
}
fn explain_style_green() -> anstyle::Style {
    anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)))
}
fn explain_style_dim() -> anstyle::Style {
    anstyle::Style::new().dimmed()
}
fn explain_style_bold() -> anstyle::Style {
    anstyle::Style::new().bold()
}

fn score_style(score: f64) -> anstyle::Style {
    if score >= 0.4 {
        explain_style_red().bold()
    } else if score >= 0.1 {
        explain_style_yellow()
    } else {
        anstyle::Style::new()
    }
}

fn confidence_tag_style(kind: &graphify_core::types::ConfidenceKind) -> anstyle::Style {
    use graphify_core::types::ConfidenceKind;
    match kind {
        ConfidenceKind::Extracted => explain_style_green(),
        ConfidenceKind::Inferred => explain_style_yellow(),
        ConfidenceKind::Ambiguous => explain_style_red(),
        ConfidenceKind::ExpectedExternal => explain_style_dim(),
    }
}

fn confidence_tag_text(kind: &graphify_core::types::ConfidenceKind, confidence: f64) -> String {
    use graphify_core::types::ConfidenceKind;
    let label = match kind {
        ConfidenceKind::Extracted => "extracted",
        ConfidenceKind::Inferred => "inferred",
        ConfidenceKind::Ambiguous => "ambiguous",
        ConfidenceKind::ExpectedExternal => "expected_external",
    };
    // Extracted edges are always 1.0 — drop the redundant number.
    match kind {
        ConfidenceKind::Extracted => format!("[{}]", label),
        _ => format!("[{} {:.2}]", label, confidence),
    }
}

/// Render `print_explain_report` to stdout.
///
/// Colors honor `palette.enabled`; pass an `ExplainPalette::fixed(false)` from
/// tests or `ExplainPalette::new(no_color_flag)` from CLI call sites.
///
/// Dependencies and dependents are grouped by [`EdgeKind`] subsection, each
/// capped at [`EXPLAIN_MAX_PER_SECTION`] rows with a trailing `... and N more`.
/// Edge-kind ordering within a section follows `EdgeKind`'s discriminant order
/// (Imports → Defines → Calls, matching the enum definition in `types.rs`).
fn print_explain_report(
    report: &graphify_core::query::ExplainReport,
    project_name: &str,
    multi_project: bool,
    palette: &ExplainPalette,
) {
    let mut stdout = std::io::stdout().lock();
    // Writing to a locked stdout is infallible in practice; if the pipe dies
    // we just stop — matching the prior `println!` behaviour.
    let _ = write_explain_report(&mut stdout, report, project_name, multi_project, palette);
}

/// Pure writer — used by `print_explain_report` and by tests that capture
/// output into a `Vec<u8>` for snapshot comparison.
pub fn write_explain_report<W: std::io::Write>(
    out: &mut W,
    report: &graphify_core::query::ExplainReport,
    project_name: &str,
    multi_project: bool,
    palette: &ExplainPalette,
) -> std::io::Result<()> {
    use graphify_core::query::ExplainEdge;
    use graphify_core::types::EdgeKind;
    use std::collections::BTreeMap;

    let dim = explain_style_dim();
    let bold = explain_style_bold();

    writeln!(out)?;
    writeln!(
        out,
        "{} {} {}",
        palette.paint(dim, "═══"),
        palette.paint(bold, &report.node_id),
        palette.paint(dim, "═══"),
    )?;
    if multi_project {
        writeln!(out, "  Project:     {}", project_name)?;
    }
    writeln!(out, "  Kind:        {:?}", report.kind)?;
    writeln!(out, "  File:        {}", report.file_path.display())?;
    writeln!(out, "  Language:    {:?}", report.language)?;
    writeln!(out, "  Community:   {}", report.community_id)?;
    if report.in_cycle {
        writeln!(
            out,
            "  In cycle:    {} (with: {})",
            palette.paint(explain_style_red().bold(), "yes"),
            report.cycle_peers.join(", ")
        )?;
    } else {
        writeln!(out, "  In cycle:    {}", palette.paint(dim, "no"))?;
    }

    writeln!(out)?;
    writeln!(out, "  {}", palette.paint(dim, "── Metrics ──"))?;
    let score = report.metrics.score;
    writeln!(
        out,
        "  Score:         {}",
        palette.paint(score_style(score), format!("{:.3}", score)),
    )?;
    writeln!(out, "  Betweenness:   {:.3}", report.metrics.betweenness)?;
    writeln!(out, "  PageRank:      {:.4}", report.metrics.pagerank)?;
    writeln!(out, "  In-degree:     {}", report.metrics.in_degree)?;
    writeln!(out, "  Out-degree:    {}", report.metrics.out_degree)?;

    // Group edges by EdgeKind, preserving insertion order within each group.
    // `BTreeMap` orders sections by `EdgeKind`'s derived `Ord` — declaration
    // order in `types.rs`: Imports → Defines → Calls.
    fn group_by_kind(edges: &[ExplainEdge]) -> BTreeMap<EdgeKind, Vec<&ExplainEdge>> {
        let mut groups: BTreeMap<EdgeKind, Vec<&ExplainEdge>> = BTreeMap::new();
        for e in edges {
            groups.entry(e.edge_kind.clone()).or_default().push(e);
        }
        groups
    }

    let write_row = |out: &mut W, arrow: &str, edge: &ExplainEdge| -> std::io::Result<()> {
        let tag_style = confidence_tag_style(&edge.confidence_kind);
        let tag_text = confidence_tag_text(&edge.confidence_kind, edge.confidence);
        writeln!(
            out,
            "      {} {} {}",
            palette.paint(dim, arrow),
            edge.target,
            palette.paint(tag_style, tag_text),
        )
    };

    let write_grouped =
        |out: &mut W, title: &str, arrow: &str, edges: &[ExplainEdge]| -> std::io::Result<()> {
            writeln!(out)?;
            writeln!(
                out,
                "  {} ({})",
                palette.paint(dim, format!("── {} ──", title)),
                edges.len(),
            )?;
            let groups = group_by_kind(edges);
            for (kind, edges) in &groups {
                writeln!(
                    out,
                    "    {} ({})",
                    palette.paint(dim, format!("── {:?} ──", kind)),
                    edges.len(),
                )?;
                for edge in edges.iter().take(EXPLAIN_MAX_PER_SECTION) {
                    write_row(out, arrow, edge)?;
                }
                if edges.len() > EXPLAIN_MAX_PER_SECTION {
                    writeln!(
                        out,
                        "    {}",
                        palette.paint(
                            dim,
                            format!("... and {} more", edges.len() - EXPLAIN_MAX_PER_SECTION),
                        ),
                    )?;
                }
            }
            Ok(())
        };

    write_grouped(out, "Dependencies", "→", &report.direct_dependencies)?;
    write_grouped(out, "Dependents", "←", &report.direct_dependents)?;

    writeln!(out)?;
    writeln!(out, "  {}", palette.paint(dim, "── Impact ──"))?;
    writeln!(
        out,
        "  Transitive dependents: {} modules",
        report.transitive_dependent_count
    )?;
    writeln!(out)?;
    Ok(())
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
                    let palette = ExplainPalette::new(false);
                    for (name, engine) in &engines {
                        if let Some(report) = engine.explain(node_id) {
                            found = true;
                            print_explain_report(&report, name, engines.len() > 1, &palette);
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

#[allow(clippy::too_many_arguments)]
fn write_all_outputs(
    project_name: &str,
    graph: &CodeGraph,
    metrics: &[graphify_core::metrics::NodeMetrics],
    communities: &[graphify_core::community::Community],
    cycles: &[Cycle],
    out_dir: &Path,
    formats: &[String],
    consolidation: &ConsolidationConfig,
) {
    // Compute allowlisted node ids once; reused across formats that consume it.
    let allowlisted: Vec<String> = if consolidation.is_empty() {
        Vec::new()
    } else {
        consolidation.allowlisted(metrics.iter().map(|m| m.id.as_str()))
    };
    // Only emit the `allowlisted_symbols` field when a section is actually
    // configured — absent section = current JSON shape.
    let allow_ref: Option<&[String]> = if consolidation.is_empty() {
        None
    } else {
        Some(allowlisted.as_slice())
    };

    for fmt in formats {
        match fmt.as_str() {
            "json" => {
                write_graph_json(graph, &out_dir.join("graph.json"));
                write_analysis_json_with_allowlist(
                    metrics,
                    communities,
                    cycles,
                    graph,
                    allow_ref,
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
    let thresholds = resolve_hotspot_thresholds(&cfg, None, None);
    let formats = resolve_formats(&cfg, format_override);
    // Watch mode always honours the configured allowlist — there is no
    // debug-flag opt-out, since `graphify watch` has no CLI surface beyond
    // existing flags.
    let consolidation = resolve_consolidation(&cfg, false);

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
    let project_refs: Vec<&ProjectConfig> = cfg.project.iter().collect();
    let workspace =
        collect_workspace_reexport_graph(&project_refs, &cfg.settings, Some(&out_dir), force);
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let _ = run_pipeline_for_project(
            project,
            &cfg.settings,
            &proj_out,
            &weights,
            &thresholds,
            &formats,
            force,
            &consolidation,
            workspace.as_ref(),
        );
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

                // Rebuild the workspace aggregate so cross-project fan-out
                // reflects the newly edited re-exports. Force=false here
                // matches the per-rebuild cache-honouring policy.
                let refreshed_workspace = collect_workspace_reexport_graph(
                    &project_refs,
                    &cfg.settings,
                    Some(&out_dir),
                    false,
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
                        &thresholds,
                        &formats,
                        false,
                        &consolidation,
                        refreshed_workspace.as_ref(),
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

fn resolve_project_name(dir: &std::path::Path) -> String {
    dir.file_name()
        .and_then(|os| os.to_str())
        .unwrap_or("unknown")
        .to_string()
}

// ---------------------------------------------------------------------------
// pr-summary command
// ---------------------------------------------------------------------------

fn run_pr_summary(dir: &std::path::Path, smells_top_n: usize) {
    use graphify_core::diff::{AnalysisSnapshot, DiffReport};
    use graphify_report::check_report::CheckReport;
    use graphify_report::pr_summary;

    if !dir.exists() {
        eprintln!(
            "graphify pr-summary: directory '{}' not found",
            dir.display()
        );
        std::process::exit(1);
    }

    let analysis_path = dir.join("analysis.json");
    if !analysis_path.exists() {
        // Detect a multi-project root: no analysis.json here but at least one subdir has its own.
        if dir.is_dir() {
            let any_child_has_analysis = std::fs::read_dir(dir)
                .ok()
                .map(|iter| {
                    iter.filter_map(Result::ok).any(|entry| {
                        entry.path().is_dir() && entry.path().join("analysis.json").exists()
                    })
                })
                .unwrap_or(false);
            if any_child_has_analysis {
                eprintln!(
                    "graphify pr-summary: '{}' is a multi-project output root — point at a single project subdirectory",
                    dir.display()
                );
                std::process::exit(1);
            }
        }
        eprintln!(
            "graphify pr-summary: missing analysis.json in '{}' (run 'graphify run' first)",
            dir.display()
        );
        std::process::exit(1);
    }
    let analysis_text = match std::fs::read_to_string(&analysis_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("graphify pr-summary: failed to read analysis.json: {}", e);
            std::process::exit(1);
        }
    };
    let analysis: AnalysisSnapshot = match serde_json::from_str(&analysis_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("graphify pr-summary: failed to parse analysis.json: {}", e);
            std::process::exit(1);
        }
    };

    let drift =
        load_optional_json::<DiffReport>(&dir.join("drift-report.json"), "drift-report.json");
    let check =
        load_optional_json::<CheckReport>(&dir.join("check-report.json"), "check-report.json");

    let project_name = resolve_project_name(dir);
    let output = pr_summary::render_with_smells(
        &project_name,
        &analysis,
        drift.as_ref(),
        check.as_ref(),
        smells_top_n,
    );
    print!("{}", output);
}

fn load_optional_json<T: for<'de> serde::Deserialize<'de>>(
    path: &std::path::Path,
    label: &str,
) -> Option<T> {
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<T>(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!(
                    "warning: failed to parse {}, skipping section: {}",
                    label, e
                );
                None
            }
        },
        Err(e) => {
            eprintln!("warning: failed to read {}, skipping section: {}", label, e);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// consolidation command
// ---------------------------------------------------------------------------

/// Emits consolidation candidates per project (and a cross-project aggregate
/// when applicable). Thin CLI wrapper around the pure renderer in
/// `graphify_report::consolidation`.
///
/// Exits with `1` on config or I/O error. Candidates are always a non-fatal
/// finding — gating is `graphify check`'s job.
fn cmd_consolidation(
    config_path: &Path,
    output_override: Option<&Path>,
    ignore_allowlist: bool,
    min_group_size: usize,
    format: &str,
) {
    use graphify_report::consolidation::{
        render, render_aggregate, render_aggregate_markdown, render_markdown, GraphSnapshot,
        ProjectInput, RenderOptions,
    };

    let format = format.to_ascii_lowercase();
    if format != "json" && format != "md" {
        eprintln!(
            "graphify consolidation: unknown --format '{}' (expected 'json' or 'md')",
            format
        );
        std::process::exit(1);
    }

    let cfg = load_config(config_path);
    let out_dir = resolve_output(&cfg, output_override);
    // Always load the real allowlist — the renderer needs it to tag
    // candidates. `ignore_allowlist` is consumed by `RenderOptions` to decide
    // whether to drop or retain hits.
    let allowlist = resolve_consolidation(&cfg, false);
    let opts = RenderOptions {
        min_group_size,
        ignore_allowlist,
    };

    if cfg.project.is_empty() {
        eprintln!(
            "graphify consolidation: no projects configured in {:?}",
            config_path
        );
        std::process::exit(1);
    }

    // Collect per-project snapshots (also used by the aggregate pass).
    let mut loaded: Vec<(String, AnalysisSnapshot, GraphSnapshot)> = Vec::new();
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        let analysis_path = proj_out.join("analysis.json");
        if !analysis_path.exists() {
            eprintln!(
                "graphify consolidation: missing {} — run `graphify run` first",
                analysis_path.display()
            );
            std::process::exit(1);
        }
        let analysis = load_snapshot(&analysis_path);

        let graph_path = proj_out.join("graph.json");
        let graph = load_graph_snapshot(&graph_path);

        loaded.push((project.name.clone(), analysis, graph));
    }

    // Per-project reports.
    for (name, analysis, graph) in &loaded {
        let report = render(name, analysis, graph, &allowlist, opts);
        let proj_out = out_dir.join(name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");

        match format.as_str() {
            "md" => {
                let md = render_markdown(&report);
                let path = proj_out.join("consolidation-candidates.md");
                std::fs::write(&path, md).expect("write consolidation-candidates.md");
                println!(
                    "[{}] {} candidate group(s) → {}",
                    name,
                    report.candidates.len(),
                    path.display()
                );
            }
            _ => {
                let json =
                    serde_json::to_string_pretty(&report).expect("serialize consolidation report");
                let path = proj_out.join("consolidation-candidates.json");
                std::fs::write(&path, json).expect("write consolidation-candidates.json");
                println!(
                    "[{}] {} candidate group(s) → {}",
                    name,
                    report.candidates.len(),
                    path.display()
                );
            }
        }
    }

    // Aggregate across projects (only when 2+).
    if loaded.len() >= 2 {
        let inputs: Vec<ProjectInput<'_>> = loaded
            .iter()
            .map(|(name, analysis, graph)| ProjectInput {
                name,
                analysis,
                graph,
            })
            .collect();
        let agg = render_aggregate(&inputs, &allowlist, opts);

        match format.as_str() {
            "md" => {
                let md = render_aggregate_markdown(&agg);
                let path = out_dir.join("consolidation-candidates.md");
                std::fs::write(&path, md).expect("write aggregate consolidation-candidates.md");
                println!(
                    "[aggregate] {} cross-project group(s) → {}",
                    agg.candidates.len(),
                    path.display()
                );
            }
            _ => {
                let json = serde_json::to_string_pretty(&agg)
                    .expect("serialize aggregate consolidation report");
                let path = out_dir.join("consolidation-candidates.json");
                std::fs::write(&path, json).expect("write aggregate consolidation-candidates.json");
                println!(
                    "[aggregate] {} cross-project group(s) → {}",
                    agg.candidates.len(),
                    path.display()
                );
            }
        }
    }
}

fn load_graph_snapshot(path: &Path) -> graphify_report::consolidation::GraphSnapshot {
    use graphify_report::consolidation::GraphSnapshot;
    if !path.exists() {
        // graph.json missing is not fatal — members just lose their kind/file
        // annotations. Warn once and continue.
        eprintln!(
            "warning: {} missing; consolidation members will be annotated without kind/file.",
            path.display()
        );
        return GraphSnapshot { nodes: vec![] };
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Cannot read {:?}: {e}", path);
            std::process::exit(1);
        }
    };
    match serde_json::from_str::<GraphSnapshot>(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid graph JSON {:?}: {e}", path);
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// install-integrations command
// ---------------------------------------------------------------------------

fn cmd_install_integrations(
    claude_code: bool,
    codex: bool,
    project_local: bool,
    skip_mcp: bool,
    dry_run: bool,
    force: bool,
    uninstall: bool,
) {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            eprintln!("graphify install-integrations: cannot determine $HOME");
            std::process::exit(1);
        }
    };
    let project_root = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("graphify install-integrations: cannot determine working directory: {e}");
            std::process::exit(1);
        }
    };

    // Auto-detect when no explicit flags
    let mut claude = claude_code;
    let mut cdx = codex;
    if !claude && !cdx {
        claude = home.join(".claude").exists();
        cdx = home.join(".agents/skills").exists();
        if !claude && !cdx {
            eprintln!(
                "graphify install-integrations: no supported AI client detected \
                 (expected ~/.claude/ or ~/.agents/skills/). \
                 Create the directory or pass --claude-code / --codex explicitly."
            );
            std::process::exit(1);
        }
    }

    let graphify_mcp_binary = which::which("graphify-mcp").unwrap_or_else(|_| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("graphify-mcp")))
            .unwrap_or_else(|| PathBuf::from("graphify-mcp"))
    });

    let opts = install::InstallOptions {
        claude_code: claude,
        codex: cdx,
        project_local,
        skip_mcp,
        dry_run,
        force,
        home,
        project_root,
        graphify_version: env!("CARGO_PKG_VERSION").to_string(),
        graphify_mcp_binary,
    };

    if opts.project_local {
        let gitignore = opts.project_root.join(".gitignore");
        let tracked = std::fs::read_to_string(&gitignore)
            .map(|s| {
                s.lines()
                    .any(|l| l.trim() == ".claude" || l.trim() == ".claude/")
            })
            .unwrap_or(false);
        if !tracked {
            eprintln!(
                "Note: .claude/ will be created in {}. Add it to .gitignore if you do not want to commit skill files.",
                opts.project_root.display()
            );
        }
    }

    if uninstall {
        match install::run_uninstall(&opts) {
            Ok(()) => println!("Uninstall complete."),
            Err(e) => {
                eprintln!("graphify install-integrations: uninstall failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let report = match install::run_install(&opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("graphify install-integrations: install failed: {e}");
            std::process::exit(1);
        }
    };

    println!(
        "Installed {} files ({} skipped as identical, {} conflicts).",
        report.manifest.files.len(),
        report.skipped_identical.len(),
        report.conflicts.len(),
    );
    if !report.conflicts.is_empty() {
        eprintln!("Conflicts (use --force to overwrite):");
        for p in &report.conflicts {
            eprintln!("  {}", p.display());
        }
    }
    if !report.mcp_changes.is_empty() {
        println!("MCP registered in:");
        for p in &report.mcp_changes {
            println!("  {}", p.display());
        }
    }
    if dry_run {
        println!("(dry-run: nothing was written)");
    }
}

// ---------------------------------------------------------------------------
// suggest stubs command (FEAT-043)
// ---------------------------------------------------------------------------

fn cmd_suggest_stubs(
    config_path: &Path,
    format: &str,
    apply: bool,
    min_edges: u64,
    project_filter: Option<&str>,
) {
    use graphify_extract::stubs::ExternalStubs;
    use graphify_report::suggest::{
        render_json, render_markdown, render_toml, score_stubs, GraphSnapshot, ProjectInput,
    };

    let format = format.to_ascii_lowercase();
    if !apply && !["md", "toml", "json"].contains(&format.as_str()) {
        eprintln!(
            "graphify suggest stubs: unknown --format '{}' (expected md, toml, or json)",
            format
        );
        std::process::exit(1);
    }

    let cfg = load_config(config_path);
    let out_dir = resolve_output(&cfg, None);

    if cfg.project.is_empty() {
        eprintln!(
            "graphify suggest stubs: no projects configured in {:?}",
            config_path
        );
        std::process::exit(1);
    }

    // If --project is specified, validate it exists.
    if let Some(name) = project_filter {
        if !cfg.project.iter().any(|p| p.name == name) {
            eprintln!(
                "graphify suggest stubs: project \"{}\" not found in {:?}",
                name, config_path
            );
            std::process::exit(1);
        }
    }

    // Settings-level shared stubs (FEAT-034 merge layer).
    let settings_stubs: Vec<String> = cfg.settings.external_stubs.clone().unwrap_or_default();

    // Load each project's graph.json + build per-project ExternalStubs.
    struct Loaded {
        name: String,
        local_prefix: String,
        stubs: ExternalStubs,
        graph: GraphSnapshot,
    }
    let mut loaded: Vec<Loaded> = Vec::new();
    let mut any_skipped = false;

    for project in &cfg.project {
        if let Some(name) = project_filter {
            if project.name != name {
                continue;
            }
        }
        let proj_out = out_dir.join(&project.name);
        let graph_path = proj_out.join("graph.json");
        if !graph_path.exists() {
            eprintln!(
                "graphify suggest stubs: project \"{}\" has no graph.json at {} — run `graphify run` first; skipping",
                project.name,
                graph_path.display()
            );
            any_skipped = true;
            continue;
        }
        let text = match std::fs::read_to_string(&graph_path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Cannot read {:?}: {e}", graph_path);
                any_skipped = true;
                continue;
            }
        };
        let graph: GraphSnapshot = match serde_json::from_str(&text) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Invalid graph.json {:?}: {e}", graph_path);
                any_skipped = true;
                continue;
            }
        };
        if graph.links.is_empty() {
            eprintln!(
                "graphify suggest stubs: project \"{}\" has empty graph.json; skipping",
                project.name
            );
            any_skipped = true;
            continue;
        }

        let local_prefix = project
            .local_prefix
            .clone()
            .unwrap_or_else(|| "src".to_string());

        let stubs = ExternalStubs::new(
            settings_stubs
                .iter()
                .chain(project.external_stubs.iter())
                .cloned(),
        );

        loaded.push(Loaded {
            name: project.name.clone(),
            local_prefix,
            stubs,
            graph,
        });
    }

    if loaded.is_empty() {
        eprintln!(
            "graphify suggest stubs: no graph.json found for any project; run `graphify run` first"
        );
        std::process::exit(1);
    }

    let inputs: Vec<ProjectInput<'_>> = loaded
        .iter()
        .map(|l| ProjectInput {
            name: l.name.as_str(),
            local_prefix: l.local_prefix.as_str(),
            current_stubs: &l.stubs,
            graph: &l.graph,
        })
        .collect();

    let report = score_stubs(&inputs, min_edges);

    if apply {
        // Implemented in Task 8.
        eprintln!("graphify suggest stubs: --apply not yet implemented (placeholder)");
        std::process::exit(1);
    }

    match format.as_str() {
        "md" => print!("{}", render_markdown(&report)),
        "toml" => print!("{}", render_toml(&report)),
        "json" => {
            let value = render_json(&report);
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        }
        _ => unreachable!(),
    }

    let _ = any_skipped; // surfaced via stderr above; exit 0 if at least one project loaded.
}

#[cfg(test)]
mod pr_summary_helper_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn resolve_project_name_uses_dir_basename() {
        assert_eq!(resolve_project_name(Path::new("./report/my-app")), "my-app");
        assert_eq!(resolve_project_name(Path::new("/abs/report/web")), "web");
    }

    #[test]
    fn resolve_project_name_returns_unknown_for_empty_path() {
        assert_eq!(resolve_project_name(Path::new("")), "unknown");
    }

    #[test]
    fn resolve_project_name_handles_trailing_slash() {
        // Path::file_name strips trailing components that are "." or root markers.
        assert_eq!(
            resolve_project_name(Path::new("./report/my-app/")),
            "my-app"
        );
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
            score,
            ..Default::default()
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
            &ConsolidationConfig::default(),
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
            &ConsolidationConfig::default(),
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
            &ConsolidationConfig::default(),
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

    #[test]
    fn evaluate_quality_gates_skips_allowlisted_hotspot() {
        use graphify_core::consolidation::{ConsolidationConfig, ConsolidationConfigRaw};
        let graph = sample_graph();
        // `a` would be the max hotspot, but the allowlist marks it as
        // intentional — so `b` should be selected instead.
        let allow = ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec!["a".into()],
            ..Default::default()
        })
        .unwrap();
        let (summary, violations) = evaluate_quality_gates(
            &graph,
            &[metric("a", 0.91), metric("b", 0.65)],
            &[],
            &[],
            &CheckLimits {
                max_cycles: None,
                max_hotspot_score: Some(0.8),
            },
            &allow,
        );

        assert_eq!(summary.max_hotspot_id.as_deref(), Some("b"));
        assert!(
            violations.is_empty(),
            "allowlisted hotspot should not trip the gate"
        );
    }

    // --- issue #14: per-project [project.check] overrides -------------------

    #[test]
    fn issue_14_effective_limits_project_check_takes_precedence_over_cli() {
        // The motivating use case: workspace CI runs with --max-hotspot-score
        // 0.70, but pageshell-native declares an override at 0.75 and expects
        // to pass at 0.738. The narrower scope wins.
        let cli = CheckLimits {
            max_cycles: Some(0),
            max_hotspot_score: Some(0.70),
        };
        let project = ProjectCheck {
            max_cycles: Some(5),
            max_hotspot_score: Some(0.75),
        };
        let merged = effective_limits(&cli, Some(&project));
        assert_eq!(merged.max_cycles, Some(5));
        assert_eq!(merged.max_hotspot_score, Some(0.75));
    }

    #[test]
    fn issue_14_effective_limits_cli_fills_when_project_check_absent() {
        // Projects without a [project.check] block inherit the CLI default.
        let cli = CheckLimits {
            max_cycles: Some(0),
            max_hotspot_score: Some(0.70),
        };
        let merged = effective_limits(&cli, None);
        assert_eq!(merged.max_cycles, Some(0));
        assert_eq!(merged.max_hotspot_score, Some(0.70));
    }

    #[test]
    fn issue_14_effective_limits_none_when_both_absent() {
        let merged = effective_limits(&CheckLimits::default(), None);
        assert_eq!(merged.max_cycles, None);
        assert_eq!(merged.max_hotspot_score, None);
    }

    #[test]
    fn issue_14_effective_limits_mixed_per_field_precedence() {
        // Project sets only max_hotspot_score; CLI sets only max_cycles.
        // Each dimension is resolved independently: project value is used
        // where present, CLI fills the gap otherwise.
        let cli = CheckLimits {
            max_cycles: Some(0),
            max_hotspot_score: Some(0.70),
        };
        let project = ProjectCheck {
            max_cycles: None,
            max_hotspot_score: Some(0.75),
        };
        let merged = effective_limits(&cli, Some(&project));
        assert_eq!(merged.max_cycles, Some(0));
        assert_eq!(merged.max_hotspot_score, Some(0.75));
    }

    #[test]
    fn issue_14_project_check_parses_from_toml_and_rejects_typos() {
        // Happy path: `[project.check]` deserializes cleanly.
        let toml_ok = r#"
            [[project]]
            name = "pageshell-native"
            repo = "./src"
            lang = ["typescript"]
            local_prefix = "@parisgroup-ai/pageshell-native"

            [project.check]
            max_hotspot_score = 0.75
        "#;
        let cfg: Config = toml::from_str(toml_ok).expect("valid project.check parses");
        let check = cfg.project[0].check.as_ref().expect("check block present");
        assert_eq!(check.max_hotspot_score, Some(0.75));
        assert_eq!(check.max_cycles, None);

        // Typo guard: deny_unknown_fields on ProjectCheck fails the parse
        // rather than silently disabling the intended gate.
        let toml_typo = r#"
            [[project]]
            name = "p"
            repo = "./src"
            lang = ["typescript"]

            [project.check]
            max_hoptspot_score = 0.75
        "#;
        let err = toml::from_str::<Config>(toml_typo)
            .err()
            .expect("typo in project.check should fail to parse");
        let msg = err.to_string();
        assert!(
            msg.contains("max_hoptspot_score") || msg.contains("unknown field"),
            "typo should surface in error: {msg}"
        );
    }

    #[test]
    fn issue_14_effective_limits_trips_hotspot_gate_per_project() {
        // End-to-end shape: a project-level override at 0.75 trips on score
        // 0.91 when CLI leaves max_hotspot_score unset.
        let graph = sample_graph();
        let cli = CheckLimits::default();
        let project = ProjectCheck {
            max_cycles: None,
            max_hotspot_score: Some(0.75),
        };
        let merged = effective_limits(&cli, Some(&project));
        let (_summary, violations) = evaluate_quality_gates(
            &graph,
            &[metric("a", 0.91), metric("b", 0.65)],
            &[],
            &[],
            &merged,
            &ConsolidationConfig::default(),
        );
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            &violations[0],
            CheckViolation::Limit { kind, .. } if kind == "max_hotspot_score"
        ));
    }

    #[test]
    fn find_nearest_ancestor_file_walks_above_repo_root_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path().join("menubar");
        let src = repo_root.join("src/components");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join("Foo.tsx");
        let tsconfig = repo_root.join("tsconfig.json");
        std::fs::write(&file, "export const Foo = 1;\n").unwrap();
        std::fs::write(&tsconfig, "{}").unwrap();

        assert_eq!(
            find_nearest_ancestor_file(&file, "tsconfig.json"),
            Some(tsconfig)
        );
    }

    #[test]
    fn find_nearest_ancestor_file_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src/components");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join("Foo.tsx");
        std::fs::write(&file, "export const Foo = 1;\n").unwrap();

        assert_eq!(find_nearest_ancestor_file(&file, "tsconfig.json"), None);
    }
}

#[cfg(test)]
mod language_parse_tests {
    use super::*;
    use graphify_core::types::Language;

    #[test]
    fn parse_languages_accepts_php() {
        let langs = parse_languages(&["php".to_string()]);
        assert_eq!(langs, vec![Language::Php]);
    }

    #[test]
    fn parse_languages_accepts_all_five() {
        let langs = parse_languages(&[
            "python".to_string(),
            "typescript".to_string(),
            "go".to_string(),
            "rust".to_string(),
            "php".to_string(),
        ]);
        assert_eq!(
            langs,
            vec![
                Language::Python,
                Language::TypeScript,
                Language::Go,
                Language::Rust,
                Language::Php,
            ]
        );
    }
}

#[cfg(test)]
mod explain_printer_tests {
    use super::*;
    use graphify_core::query::{ExplainEdge, ExplainMetrics, ExplainReport};
    use graphify_core::types::{ConfidenceKind, EdgeKind, Language, NodeKind};
    use std::path::PathBuf;

    fn dep(target: &str, kind: EdgeKind, conf: f64, ck: ConfidenceKind) -> ExplainEdge {
        ExplainEdge {
            target: target.to_string(),
            edge_kind: kind,
            confidence: conf,
            confidence_kind: ck,
        }
    }

    fn sample_report() -> ExplainReport {
        ExplainReport {
            node_id: "src.foo".to_string(),
            kind: NodeKind::Module,
            file_path: PathBuf::from("src/foo.rs"),
            language: Language::Rust,
            metrics: ExplainMetrics {
                score: 0.412,
                betweenness: 12.5,
                pagerank: 0.0321,
                in_degree: 3,
                out_degree: 5,
            },
            community_id: 2,
            in_cycle: false,
            cycle_peers: vec![],
            direct_dependencies: vec![
                dep("src.bar", EdgeKind::Imports, 1.0, ConfidenceKind::Extracted),
                dep("src.baz", EdgeKind::Calls, 0.7, ConfidenceKind::Inferred),
                dep(
                    "std::io",
                    EdgeKind::Imports,
                    0.5,
                    ConfidenceKind::ExpectedExternal,
                ),
            ],
            direct_dependents: vec![dep(
                "src.root",
                EdgeKind::Imports,
                1.0,
                ConfidenceKind::Extracted,
            )],
            transitive_dependent_count: 4,
            top_transitive_dependents: vec![],
        }
    }

    fn render(report: &ExplainReport, multi_project: bool) -> String {
        let mut buf: Vec<u8> = Vec::new();
        let palette = ExplainPalette::fixed(false);
        write_explain_report(&mut buf, report, "test-project", multi_project, &palette).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// Golden snapshot — FEAT-039 regression guard. If the printer shape
    /// changes intentionally, update this string. If it changes unintentionally,
    /// the diff will surface here.
    #[test]
    fn write_explain_report_colorless_snapshot_single_project() {
        let out = render(&sample_report(), false);
        let expected = concat!(
            "\n",
            "═══ src.foo ═══\n",
            "  Kind:        Module\n",
            "  File:        src/foo.rs\n",
            "  Language:    Rust\n",
            "  Community:   2\n",
            "  In cycle:    no\n",
            "\n",
            "  ── Metrics ──\n",
            "  Score:         0.412\n",
            "  Betweenness:   12.500\n",
            "  PageRank:      0.0321\n",
            "  In-degree:     3\n",
            "  Out-degree:    5\n",
            "\n",
            "  ── Dependencies ── (3)\n",
            "    ── Imports ── (2)\n",
            "      → src.bar [extracted]\n",
            "      → std::io [expected_external 0.50]\n",
            "    ── Calls ── (1)\n",
            "      → src.baz [inferred 0.70]\n",
            "\n",
            "  ── Dependents ── (1)\n",
            "    ── Imports ── (1)\n",
            "      ← src.root [extracted]\n",
            "\n",
            "  ── Impact ──\n",
            "  Transitive dependents: 4 modules\n",
            "\n",
        );
        assert_eq!(out, expected, "explain printer snapshot mismatch");
    }

    #[test]
    fn write_explain_report_shows_project_when_multi_project() {
        let out = render(&sample_report(), true);
        assert!(
            out.contains("Project:     test-project"),
            "multi-project mode should print project line; got:\n{}",
            out
        );
    }

    #[test]
    fn write_explain_report_caps_dependencies_at_ten_per_section() {
        let mut report = sample_report();
        // 15 Imports edges — should trigger "... and 5 more" footer.
        report.direct_dependencies = (0..15)
            .map(|i| {
                dep(
                    &format!("src.t{}", i),
                    EdgeKind::Imports,
                    1.0,
                    ConfidenceKind::Extracted,
                )
            })
            .collect();
        let out = render(&report, false);
        assert!(
            out.contains("... and 5 more"),
            "cap footer should fire for 15 edges in a section; got:\n{}",
            out
        );
        // Only 10 rows should render
        let row_count = out.matches("      → src.t").count();
        assert_eq!(row_count, EXPLAIN_MAX_PER_SECTION);
    }

    #[test]
    fn write_explain_report_in_cycle_lists_peers_uncolored() {
        let mut report = sample_report();
        report.in_cycle = true;
        report.cycle_peers = vec!["src.a".to_string(), "src.b".to_string()];
        let out = render(&report, false);
        assert!(
            out.contains("In cycle:    yes (with: src.a, src.b)"),
            "cycle peers should render inline; got:\n{}",
            out
        );
    }

    #[test]
    fn write_explain_report_emits_no_ansi_escapes_when_palette_disabled() {
        let out = render(&sample_report(), false);
        assert!(
            !out.contains('\x1b'),
            "palette=disabled output must contain no ANSI escapes; got:\n{:?}",
            out
        );
    }
}
