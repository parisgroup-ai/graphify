# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

See also: [[AGENTS.md]]

## What is Graphify

Graphify is a Rust CLI tool for architectural analysis of codebases. It extracts dependencies from Python, TypeScript, Go, Rust, and PHP source code using tree-sitter AST parsing, builds knowledge graphs with petgraph, and generates structured reports identifying architectural hotspots, circular dependencies, and community clusters.

Distributed as a standalone binary (no runtime dependencies). Targets macOS + Linux.

## Running Graphify

```bash
# Generate config
graphify init

# Full pipeline: extract → analyze → report
graphify run --config graphify.toml

# Individual stages
graphify extract --config graphify.toml    # sources → graph.json per project
graphify analyze --config graphify.toml    # metrics → analysis.json + CSV
graphify report  --config graphify.toml    # all outputs including markdown

# Watch mode: auto-rebuild on file changes
graphify watch --config graphify.toml

# Architectural drift detection
graphify diff --before report/v1/analysis.json --after report/v2/analysis.json
graphify diff --baseline report/baseline/analysis.json --config graphify.toml

# Query the graph
graphify query "app.services.*" --config graphify.toml
graphify path app.main app.services.llm --config graphify.toml
graphify explain app.services.llm --config graphify.toml
graphify shell --config graphify.toml

# Install AI integrations (Claude Code / Codex)
graphify install-integrations                   # auto-detect
graphify install-integrations --project-local   # install into ./.claude
graphify install-integrations --uninstall       # remove installed artifacts

# Build from source
cargo build --release -p graphify-cli
# Binary at target/release/graphify

# Tests
cargo test --workspace
cargo test -p graphify-extract             # single crate
```

## Configuration

Multi-project analysis via `graphify.toml`:

```toml
[settings]
output = "./report"
weights = [0.4, 0.2, 0.2, 0.2]  # betweenness, pagerank, in_degree, in_cycle
exclude = ["__pycache__", "node_modules", ".git", "dist", "tests", "__tests__", ".next"]
format = ["json", "csv", "md", "html"]  # also: neo4j, graphml, obsidian

[[project]]
name = "ana-service"
repo = "./apps/ana-service"
lang = ["python"]
local_prefix = "app"
```

## Architecture

Cargo workspace with 5 crates:

| Crate | Role | Key deps |
|---|---|---|
| `graphify-core` | Graph model, metrics, community detection, cycles | petgraph, serde, rand |
| `graphify-extract` | tree-sitter AST parsing, file discovery, module resolution | tree-sitter, tree-sitter-python, tree-sitter-typescript, rayon |
| `graphify-report` | JSON, CSV, Markdown, HTML, Neo4j Cypher, GraphML, Obsidian output generation | serde_json, csv |
| `graphify-cli` | CLI (clap), config parsing, pipeline orchestration | clap, toml, rayon |
| `graphify-mcp` | MCP server exposing graph queries to AI assistants | rmcp, tokio, clap |

### Data flow

```
graphify.toml (project definitions)
    ↓
For each [[project]]:
    Walker: discover files (.py, .ts, .tsx)
        ↓ parallel via rayon
    Extractors: tree-sitter AST → nodes + edges
        ↓
    Resolver: normalize module refs (Python relative, TS path aliases)
        ↓
    CodeGraph (petgraph DiGraph)
        ↓
    Analysis:
        ├── Betweenness centrality (Brandes, sampled k=min(200,n))
        ├── PageRank (iterative, damping=0.85)
        ├── Community detection (Louvain + Label Propagation fallback)
        ├── Cycle detection (Tarjan SCC + DFS simple cycles, cap 500)
        └── Unified scoring (configurable weights)
        ↓
    Report generation:
        ├── graph.json (node_link_data format)
        ├── analysis.json (metrics + communities + cycles)
        ├── graph_nodes.csv / graph_edges.csv
        ├── architecture_report.md
        ├── architecture_graph.html
        ├── graph.cypher (Neo4j import script)
        ├── graph.graphml (GraphML XML)
        └── obsidian_vault/ (Obsidian markdown notes)
```

### Key modules

| File | Role |
|---|---|
| `crates/graphify-core/src/types.rs` | Node, Edge, Language, NodeKind, EdgeKind, ConfidenceKind |
| `crates/graphify-core/src/graph.rs` | CodeGraph — petgraph wrapper with dedup + weight increment |
| `crates/graphify-core/src/metrics.rs` | Betweenness, PageRank, unified scoring |
| `crates/graphify-core/src/community.rs` | Louvain + Label Propagation |
| `crates/graphify-core/src/cycles.rs` | Tarjan SCC + simple cycles |
| `crates/graphify-core/src/query.rs` | QueryEngine — search, path, explain, stats |
| `crates/graphify-extract/src/python.rs` | Python extractor (imports, defs, calls) |
| `crates/graphify-extract/src/typescript.rs` | TypeScript extractor (imports, exports, require, calls) |
| `crates/graphify-extract/src/php.rs` | PHP extractor (namespace, use, class/interface/trait/enum/function, calls) |
| `crates/graphify-extract/src/resolver.rs` | Module resolver (Python relative w/ `is_package`, TS path aliases, PHP PSR-4 via composer.json) |
| `crates/graphify-extract/src/cache.rs` | ExtractionCache — SHA256-based per-file extraction cache |
| `crates/graphify-extract/src/walker.rs` | File discovery + dir exclusion + `is_package` detection + PSR-4 path translation |
| `crates/graphify-report/src/html.rs` | Interactive HTML visualization (D3.js force graph, self-contained) |
| `crates/graphify-report/src/neo4j.rs` | Neo4j Cypher import script (CREATE nodes, CREATE relationships) |
| `crates/graphify-report/src/graphml.rs` | GraphML XML export (compatible with yEd, Gephi) |
| `crates/graphify-report/src/obsidian.rs` | Obsidian vault export (one .md per node with [[wikilinks]]) |
| `crates/graphify-cli/src/main.rs` | CLI, config parsing, pipeline, watch mode |
| `crates/graphify-core/src/diff.rs` | AnalysisSnapshot deserialization, DiffReport, compute_diff() |
| `crates/graphify-report/src/diff_json.rs` | Drift report JSON output |
| `crates/graphify-report/src/diff_markdown.rs` | Drift report Markdown output |
| `crates/graphify-report/src/check_report.rs` | Public `CheckReport` / `ProjectCheckResult` / `CheckViolation` / `CheckLimits` types (moved from graphify-cli for external consumption) |
| `crates/graphify-report/src/pr_summary.rs` | Pure `render(project_name, analysis, drift, check) -> String` for `graphify pr-summary` Markdown output |
| `crates/graphify-cli/src/watch.rs` | WatchFilter (extension/exclude filtering), affected project detection |
| `crates/graphify-mcp/src/main.rs` | MCP server entry point, config parsing, extraction pipeline |
| `crates/graphify-mcp/src/server.rs` | GraphifyServer struct, 9 MCP tool handlers, ServerHandler impl |

### Graph representation

- **Nodes**: modules, functions, classes — with attributes: `id`, `kind`, `file_path`, `language`, `line`, `is_local`
- **Edge types**: `Imports` (module→module), `Defines` (module→symbol), `Calls` (module→symbol)
- **Weight tracking**: repeated calls increment `Edge.weight` instead of creating duplicate edges
- **Module naming**: file paths normalized to dot notation (`app/services/llm.py` → `app.services.llm`), `__init__.py`/`index.ts` collapsed to parent
- **Package detection**: `DiscoveredFile.is_package` tracks `__init__.py`/`index.ts` files; resolver uses this to correctly resolve relative imports from package entry points

## Conventions

- CLI uses `clap` with derive macros
- Config via `graphify.toml` (TOML format, serde Deserialize)
- Extraction parallelized with `rayon::par_iter`
- Each `extract_file` call creates a fresh tree-sitter Parser (Parser is not Send)
- Excluded directories: `__pycache__`, `node_modules`, `.git`, `dist`, `tests`, `__tests__`, `.next`, `build`, `.venv`, `venv`
- Excluded test files (built-in): `*.test.{ts,tsx,js,jsx}`, `*.spec.{ts,tsx,js,jsx}`, `*.test.py`, `*_test.py`
- Output: one subdirectory per project under the configured output path
- Graph serialization compatible with NetworkX `node_link_data` JSON format
- Cross-project summary (`graphify-summary.json`) only generated when 2+ projects configured; contains aggregate stats only (no full edge list)
- TS workspace aliases (`@repo/*` → `../../packages/*`) preserve the original import string as node ID when target path traverses outside the project
- Louvain Phase 2 merges singleton communities: connected singletons → best neighbor, isolated singletons → grouped together
- Walker warns via `eprintln!` when a project discovers ≤1 file (misconfigured `local_prefix`)
- MCP server uses `rmcp` v0.1 with `#[tool(tool_box)]` macro (not `#[tool_router]` — API differs from docs)
- MCP server config is duplicated from CLI (small, stable structs — extract if a third consumer appears)
- MCP extraction is eager on startup; all diagnostic output on stderr (stdout reserved for JSON-RPC)
- MCP server wraps `QueryEngine` in `Arc` (ServerHandler requires Clone)
- Edge confidence: `confidence: f64` (0.0–1.0) + `confidence_kind: ConfidenceKind` (Extracted/Inferred/Ambiguous)
- Bare call sites: confidence 0.7/Inferred (unqualified callee)
- Resolver confidence: direct=1.0, Python relative=0.9, TS alias=0.85, TS relative=0.9
- Non-local edge downgrade: min(confidence, 0.5) → Ambiguous
- Edge merge keeps max confidence of all observations
- Extraction cache: `.graphify-cache.json` in each project's output directory, keyed by SHA256 of file contents
- Cache is on by default; `--force` flag bypasses it (full rebuild, fresh cache saved)
- Cache invalidation: version mismatch or `local_prefix` change → full discard
- Query commands (query, explain, path, shell) don't use cache — always fresh extraction
- Watch mode: `notify` v7 + `notify-debouncer-mini` 0.5 with 300ms debounce
- Watch rebuilds only affected projects (per-project path prefix matching)
- Watch `--force` applies only to initial build; subsequent rebuilds always use cache
- Diff operates on analysis.json snapshots (not CodeGraph directly) — decoupled from internal types
- Community equivalence mapping: max-overlap matching handles unstable community IDs across runs
- Hotspot threshold default: 0.05 (configurable via --threshold)
- Drift report output: drift-report.json + drift-report.md
- `graphify check` writes `<project_out>/check-report.json` unconditionally (unified: project rules + contract violations) — introduced by FEAT-015 so `pr-summary` can consume it
- `graphify pr-summary <DIR>` — pure renderer over `analysis.json` (required) + `drift-report.json` / `check-report.json` (optional); Markdown to stdout, warnings to stderr, exit 1 on required-input errors, exit 0 otherwise (gating is `graphify check`'s job)
- CLI error-exit convention: `exit(1)` for all error paths (not exit 2) — matches `cmd_diff`/`cmd_trend` pattern; keeps graphify CLI uniform
- `graphify diff` error routing: on `AnalysisSnapshot` deserialize failure, `graphify-cli::main::load_snapshot` calls `graphify_core::history::is_trend_snapshot_json` (discriminator: requires `captured_at` + `project` at root) to emit an explanatory message with the baseline-copy recipe instead of the raw serde error — pattern: run discriminator only on the error path, so happy path stays at one read + one parse
- Tests: run `cargo test --workspace` for the authoritative current count
- PHP PSR-4 mapping loaded from `composer.json` (`autoload.psr-4` + `autoload-dev.psr-4`); longest-prefix match wins; namespaces normalized `\` → `.`
- PHP test files excluded: `*Test.php` (PHPUnit convention)
- PHP confidence: `use X\Y\Z` → 1.0 / Extracted (fully qualified); bare calls 0.7 / Inferred (same as Go/Python)
- PHP method id scheme: `{module}.{ClassName}.{method}`
- PhpExtractor never sets `is_package = true` (PHP has no package entry-point equivalent)
- `graphify_extract::walker::discover_files_with_psr4` is the PSR-4-aware discovery entry; `discover_files` remains a thin wrapper for non-PHP projects
- `[consolidation]` section in `graphify.toml` (FEAT-020 slice landed in 25eabc8): `allowlist = ["pattern", …]` — regex anchored `^…$`, matched against the **leaf symbol name** (last dot-segment of a node id), not the full id; so `TokenUsage` hits `app.models.TokenUsage` but not `app.models.TokenUsageAdapter`
- `[consolidation].suppress_barrel_cycles = true` (BUG-015 opt-in, default `false`): drops cycles whose only cycle-making edges route through the project's root barrel node, **but only** when that barrel node (`id == local_prefix`) is also matched by the `allowlist`. Counters FEAT-028 synthetic barrel cycles on consumers like `parisgroup-ai/cursos` where `src/index.ts` is the npm entry point. Implementation in `graphify-core/src/cycles.rs::{find_sccs_excluding, find_simple_cycles_excluding}` (uses `petgraph::visit::NodeFiltered` for SCC, skip-neighbor during DFS for simple cycles) plus `graphify-cli/src/main.rs::barrel_exclusion_ids`. Applied at all 4 `run_analyze` call sites (Analyze, Diff baseline-vs-live, Run pipeline helper, Check); query engine does NOT apply it (no consolidation config in scope for `query`/`explain`/`path`/`shell`). `--ignore-allowlist` debug flag disables barrel suppression too since it disables the whole consolidation config
- Consolidation regex validation is fail-fast at config load; invalid pattern aborts `load_config` with the offending pattern surfaced
- `analysis.json` gains `allowlisted_symbols: [...]` only when a `[consolidation]` section is present (absent section = legacy JSON shape, backward compatible)
- `--ignore-allowlist` debug flag on `run` / `report` / `check` bypasses the allowlist for troubleshooting
- `check`'s `max_hotspot_score` gate skips allowlisted nodes when picking candidates — intentional mirrors never trip CI on their own score
- FEAT-020 deferred subtasks (tracked as FEAT-022/023/024/DOC-001): `graphify consolidation` subcommand, `[consolidation.intentional_mirrors]` drift suppression, `pr-summary` annotation strip, README migration note
- `graphify consolidation --config <PATH>` (FEAT-022 landed in 1be5225): emits `consolidation-candidates.json` per project + top-level aggregate when 2+ projects configured; flags `--ignore-allowlist` (keeps hits tagged `allowlisted: true`), `--min-group-size N` (default 2), `--format json|md`; JSON schema `schema_version: 1` with `alternative_paths: []` always present (reserved for FEAT-021); pure renderer lives in `graphify_report::consolidation`
- `tn` feasibility-check heuristics (relevant when authoring task bodies for `/tn-plan-session`): bodies fail with `body is stub` when they contain placeholder prose (`Description here.`, empty `Subtask 1/2`), and fail with `referenced file not found` when they contain fully-qualified paths to files that don't exist in the repo — including `*.rs` paths for files this task will *create*. Phrase new files as "a new module under `crates/<crate>/src/`" (directory-only) instead; JSON examples should use placeholder strings that don't end in a recognizable source-file extension
- `tn` body-is-stub structural trap (CHORE-002): `is_stub_body_str` only counts prose between `# Title` and the first `## …` heading as "description" — so a richly authored body that nests everything under `## Description` / `## Motivation` / `## Subtasks` (leaving the strip directly under `# Title` empty) is still rejected as `body is stub`. Workaround when authoring tasks for `/tn-plan-session`: write a 1–3 line TL;DR paragraph directly under `# Title`, *before* the first `## …` heading. The tn-side fix is tracked as a follow-up in `tasknotes-cli`; until it lands, the TL;DR is mandatory for any task that will flow through the planner's feasibility gate
- TS barrel re-export collapse (FEAT-021 Part B slice, `0cf10ed`): `run_extract` aggregates per-file `ReExportEntry`s from Part A, builds a project-wide `ReExportGraph`, and walks every TS symbol back to its canonical declaration. Outcomes: `Canonical` drops the barrel symbol node and appends the dropped id + intermediates to the canonical node's `alternative_paths` (deduped, order-stable); raw edges whose src/target are collapsed ids get rewritten; `Cycle` logs to stderr and leaves participants; `Unresolved` logs an `Info:` stderr line and leaves the node as-is (no confidence downgrade — v1 policy locked by FEAT-025). All seven report writers now emit `alternative_paths` when non-empty (FEAT-025): JSON (serde skip-empty), CSV (`alternative_paths` column, pipe-joined), Markdown (`### Alternative Import Paths` subsection under hotspots), HTML (inline field on `GRAPHIFY_DATA.nodes[i]`, surfaced in the tooltip suffix), Neo4j Cypher (node property as list literal), GraphML (`alternative_paths` data key, pipe-joined string since GraphML has no array type), Obsidian (YAML sequence in the per-node frontmatter). FEAT-026 closes the module-level edge gap: TS extractor now captures named + default import specifiers (`NamedImportEntry` in `graphify-extract/src/lang.rs`, added to `ExtractionResult` with `#[serde(default)]` for cache backcompat) and the pipeline walks each specifier through the same `ReExportGraph` used for symbols, emitting one `Imports` edge per canonical module with weight dedup via `CodeGraph::add_edge`. Boundaries (v1 policy, documented in the task body): `import * as ns from '...'` stays a single barrel edge (no specifiers to fan out); type-only imports are captured with `is_type_only: true` and still contribute (weight 1, parity preserved); `Unresolved`/`Cycle` outcomes fall back to a barrel-targeted edge so no import is ever dropped. CHORE-003 reference-monorepo regression on `parisgroup-ai/cursos` @ `8ff36cc1` confirmed the combined FEAT-021/025/026 effect: −17.1% nodes, 0 edge change, top hotspot −89% (`DomainError` 0.364 → 0.039), zero new cycles, 1,923 canonical nodes carrying 2,321 alternative_paths across 14/16 projects. Report at `docs/benchmarks/2026-04-20-feat-021-025-cursos-regression.md`. FEAT-027 spike resolved the tsconfig-paths-through-barrels question with a split answer (2026-04-20, 2 integration fixtures landed in `tests/fixtures/ts_tsconfig_alias_project/` + `ts_cross_project_alias/`): **same-project aliases** (`@app/*` → `src/*` within one `[[project]]`) are fully covered post-FEAT-026 — the resolver lowers to a local module id, per-project `ReExportGraph` walker fans out normally, consumer lands on canonical (`feat_027_same_project_tsconfig_alias_fans_out_to_canonical`). **Cross-project aliases** (`@repo/*` spanning sibling `[[project]]`s) are NOT covered in v1 — `apply_ts_alias_with_context` returns the raw alias string when the target is outside the project root (`resolver.rs:307-309`), `is_local = false` (`resolver.rs:289`), FEAT-026 fan-out short-circuits and emits a single edge to the raw alias (`main.rs:1893`). The v1 contract was pinned by `feat_027_cross_project_alias_stays_at_barrel_v1_contract` — the test was the intentional tripwire for a future workspace-wide `ReExportGraph` merge; FEAT-028 (below) landed that merge and inverted the tripwire to `feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope`
- FEAT-028 workspace-wide `ReExportGraph` fan-out (2026-04-20, session `2026-04-20-1437`, 7 commits `0fe862b` → `cd760a1`): cross-project aliases (`@repo/*` spanning sibling `[[project]]`s) now fan out end-to-end, closing the FEAT-027 gap. Architecture: new `WorkspaceReExportGraph` aggregate in `crates/graphify-extract/src/workspace_reexport.rs` holding per-project `ProjectReExportContext` (known_modules, reexports, `module_paths` PathBuf→(project, module_id), first-wins `modules_to_project` index + collision log), plus parallel walker `resolve_canonical_cross_project` returning `CrossProjectResolution::{Canonical, Cycle, Unresolved}` with `CrossProjectHop` participants. `ModuleResolver` gained `apply_ts_alias_workspace(alias, &workspace) -> Option<WorkspaceAliasTarget { project, module_id }>` — mirrors `apply_ts_alias_with_context` but when the alias target path falls inside ANY registered project's root, returns the target project + module_id instead of the raw alias string. `match_alias_target` was broadened in `2904e85` to support inner-glob tsconfig forms (`"@repo/*": ["../../packages/*/src"]`) — previously trailing-`*` or exact-match only; this was the final pre-existing blocker called out in slice 4. Pipeline reshape (`a4f8972` + `60c6a85`): `run_extract` split into `build_project_reexport_context` (phase 1, collect-only) + `run_extract_with_workspace` (phase 2, fan-out against a workspace graph). The outer project loop now runs phase 1 for every project, merges contexts into a single `WorkspaceReExportGraph`, then runs phase 2 per project. 7 call sites use the workspace path (Extract/Analyze/Report/Run/Check/Watch-init/Watch-rebuild); 3 single-project sites stay on the legacy path (Diff baseline-vs-live + `build_query_engine` + in-main-extract wrapper). Workspace graph only built when ≥2 projects AND ≥1 TS project — single-project and non-TS-only configs keep the legacy fast path with zero overhead. Namespacing decision (option 2, ADR in module doc-comment): public node ids stay per-project (e.g. `src.foo`) — the workspace lookup is internal; cross-project edges reference the target's module_id verbatim since ids don't collide in the test corpus (if they ever do, `modules_to_project` first-wins + collision log surfaces it). `alternative_paths` reuses FEAT-025's writer fan-out — cross-project dropped alias ids land on the canonical node's property same as intra-project barrels. Follow-ups tracked on the FEAT-028 task body: step 6 (`graphify-summary.json` `cross_project_edges` regression on `parisgroup-ai/cursos` — **FEAT-029 benchmark confirmed the ~2,165 claim within +14.3%**: measured +2,475 cross-project edges A→B (pre-0.10.0 vs post-0.11.1 on pin `8ff36cc1`), all in `imports` kind; both redistributive (−1,622 edges across top-5 consumer-app pairs that previously terminated at barrels) and additive (+~4,000 pkg-api→`@repo/*` alias edges previously invisible). Mitigation (BUG-015 `suppress_barrel_cycles` + `src` allowlist) neutralizes all 541 synthetic cycles with zero edge impact. Report at `docs/benchmarks/2026-04-20-feat-029-cross-project-edges.md`), step 8 (feature-gate decision resolved by FEAT-030: opt-out `[settings] workspace_reexport_graph = false` short-circuits `collect_workspace_reexport_graph` before the topology check; default absent/`true` preserves the always-on ship state, no stderr notice. Rationale + alternatives in `docs/adr/0001-workspace-reexport-graph-gate.md`). Meta follow-ups from the same session on the planner/tn side (tracked as CHORE-004 + CHORE-005): tn's `tn session log` success line uses `main-context budget:` where the field is actually a snapshot per BUG-012/DOC-003 (rename to `snapshot:`), and `/tn-plan-session` Step 8 needs an explicit guard against closing sessions on `subagent_tokens_sum` approaching `budget.tokens` (it's an FEAT-019 calibration meter, not a dispatch-capacity ceiling — each `Task` subagent gets a fresh 1M model-context window)
- `resolve_ts_relative` `is_package` fix (bundled in `0cf10ed`): the TS relative-import resolver unconditionally popped the current-module leaf, so `./entities` from `src/domain/index.ts` resolved to `src.entities` instead of `src.domain.entities` — same bug shape as BUG-001 on the Python side. Now honours `DiscoveredFile.is_package` symmetric with the Python resolver. New public helper `ModuleResolver::is_local_module(id)` so the re-export walker can answer "stop at the package boundary" without reaching into `known_modules` directly
- BUG-016 Rust `crate::` resolution drops `local_prefix` (2026-04-21, fix `19f9845`, shipped in v0.11.3): the resolver's `crate::` branch at `crates/graphify-extract/src/resolver.rs:271-281` stripped the prefix and replaced `::` with `.` but never re-prepended the project-level `local_prefix`, so `crate::types::Node` from any module in a Rust crate resolved to `types.Node` instead of `src.types.Node` (auto-detected prefix), missed `known_modules`, and landed on a non-local placeholder — making intra-crate hub structure invisible across every Rust analysis with non-empty `local_prefix`. Same shape as BUG-001 (Python relative) and BUG-007/011 (TS workspace alias mangling): a language-specific resolver branch forgets to apply the project-level prefix re-prepend. Fix added `local_prefix: String` field + `set_local_prefix()` setter + private `apply_local_prefix(id)` helper to `ModuleResolver`; wired from `run_extract_with_workspace`, `build_project_reexport_context` (graphify-cli) and the MCP server's extraction pipeline. `super::`/`self::` already worked because `resolve_rust_path` walks up from `from_module` (which already carries the prefix). Discovered via dogfood: graphify v0.11.2 on its own 5-crate workspace (`graphify.toml` at repo root) showed every local hotspot with `in_degree=1, betweenness=0` — implausible for `src.graph.CodeGraph`. Post-fix: `src.types.Node` `in_degree` 0 → 3, `src.graph.CodeGraph` score invisible → 0.364 (top local hotspot in graphify-core), 0 cycles introduced. The two pre-existing `crate::` tests in `resolver.rs` had comments admitting the buggy behaviour ("the registered module might be prefixed differently"); updated in-place plus added a smoking-gun BUG-016 test and a no-prefix regression guard. Tracked follow-up: FEAT-031 (bare-name Rust call resolution) — `Node::new(...)` after `use crate::types::Node;` still bypasses the fix because the call-edge target is the bare symbol `Node`, not `crate::types::Node`; closes the remaining ~40-60% intra-crate visibility gap via per-file `use_aliases: HashMap<String, String>` on `ExtractionResult` consulted by the post-extraction resolver pass
- Self-dogfood config (`graphify.toml` at repo root, FEAT-deferred 3+ sessions, landed `3240b54`): 5 `[[project]]` blocks (one per crate), `lang = ["rust"]`, no `local_prefix` (auto-detect → `src` for every crate). `report/` is gitignored (subdir per crate); 5 stale tracked files at `report/{analysis.json,architecture_report.md,*.png}` predate the per-project subdir layout — leftovers, untouched by current runs, candidate for cleanup but not a bug. Dogfood baseline at `f4ac5e2` (graphify v0.11.3 post-BUG-016): 1,341 → 1,330 nodes total across 5 crates, 1,543 edges, 0 cycles, 5 communities sets (10/12/7/4/2). Top hotspots remain external (`std::collections::HashMap`, `Some`, `serde::Deserialize`) — `[[project.external_stubs]]` configuration for std/serde/petgraph/clap/tokio is the natural follow-up (issue #12 shipped the feature; the dogfood config doesn't use it yet). CLI ↔ MCP cross-edge pair confirms the documented "MCP server config is duplicated from CLI" debt (42 shared modules each direction)
- FEAT-034 `[settings].external_stubs` merge layer (2026-04-21, shipped in v0.11.7): `Settings` struct (CLI + MCP) gains `external_stubs: Option<Vec<String>>`. At the 2 `ExternalStubs::new(...)` call sites (`graphify-cli/src/main.rs` run_extract_with_workspace, `graphify-mcp/src/main.rs` run_extract), the settings list is chained ahead of the project list via `settings.external_stubs.iter().flatten().chain(project.external_stubs.iter()).cloned()` — concatenation, not override. `ExternalStubs::new` already sorts by descending prefix length and dedupes, so overlap between the two sources is harmless. Self-dogfood config shrank from 119 → 77 lines (35%) by lifting the 30-entry Rust prelude (`std`, `Vec`, `String`, `Some`/`None`/`Ok`/`Err`/`Self`, `format`/`writeln`/`println`/…/`vec`/`write`, `assert*`, `debug_assert*`, `panic`/`todo`/`unimplemented`/`unreachable`/`dbg`) to `[settings]`. Integration check (`--force` rebuild pre vs post): graphify-core top 10 bit-identical; other crates show only tied-score rank reordering inside large equal-score buckets (e.g. 27 nodes tied at `0.22499` in graphify-mcp — HashMap-iter non-determinism, pre-existing, not FEAT-034). Unit guard: `feat_034_chained_settings_and_project_inputs_match_identically` in `stubs.rs` asserts `ExternalStubs::new(settings ++ project)` produces identical `matches()` results to `ExternalStubs::new(merged_single_list)` across 6 target shapes. `graphify init` template now advertises the shared layer with a commented `[settings] external_stubs = ["std", "serde"]` hint
- FEAT-033 ExpectedExternal edges deprioritized in hotspot scoring (2026-04-21, shipped in v0.11.6): `compute_metrics_with_thresholds` now runs every scoring input (betweenness, PageRank, in/out-degree, cycle membership, hotspot classification) over a filtered view of the graph that excludes edges whose `confidence_kind == ConfidenceKind::ExpectedExternal`. New helper `CodeGraph::filter_edges<F>(&self, keep: F) -> CodeGraph` in `graphify-core/src/graph.rs` clones nodes verbatim and applies the predicate to edges; the original graph stays untouched and remains the data source for `query`/`explain`/report writers. Semantic contract (Choice 1 — "hotspot view = filtered view"): every field in `NodeMetrics` reflects the scoring graph, so an external stub like `std::path::PathBuf` reports `score=0.000, in_degree=0, betweenness=0.000`, but `graphify explain std::path::PathBuf` still shows its dependents via `QueryEngine` (which reads the full graph). No-op for projects without `[[project]].external_stubs` configured — the predicate returns true for every edge, the filter is cheap. Acceptance verified on graphify's own 5-crate self-dogfood: top 10 of every crate is now 100% actionable (local `src.*` + cross-crate `graphify_core::*`/`graphify_extract::*`), zero `std`/`serde`/`tree_sitter`/`rayon`/`petgraph`/`clap`/`tokio` in any top 10 (previously `std::path::PathBuf` was #1 in graphify-cli). Tests: `filter_edges_*` × 3 in `graph.rs` + `compute_metrics_{filters_expected_external,keeps_all_nodes_after_filter,mixed_confidence_only_counts_non_external}` × 3 in `metrics.rs`. Cycle detection note: `ExpectedExternal` targets are leaves with no outgoing local edges, so SCC composition is identical with/without the filter — running SCC on the scoring graph is for semantic consistency, not a behaviour change

## Build & Release

- Rust 2021 edition (check current version in root `Cargo.toml`)
- CI: GitHub Actions on tag push (`v*`), builds 4 targets (macOS Intel/ARM, Linux x86/ARM)
- Static binaries for Linux (MUSL), universal binaries for macOS
- Release binary ~3.5MB
- **CI gates** (`.github/workflows/ci.yml`, strict):
  - `cargo fmt --all -- --check` — run `cargo fmt --all` locally before every push
  - `cargo clippy --workspace -- -D warnings` — note: no `--all-targets`, so test-only lints are not gated (lib+bin only)
  - `cargo test --workspace`
- **Release workflow** (`.github/workflows/release.yml`) triggers only on `v*` tag push; builds binaries but does NOT run clippy/fmt/test — use CI (ci.yml) to catch those before tagging
- When tagging a release: `git tag vX.Y.Z <commit>` explicitly (not `HEAD`) so the tag pins to the intended version-bump commit even if later commits land

### Version bump

All crates use `version.workspace = true` — bump only `[workspace.package].version` in root `Cargo.toml`:
```bash
# Edit Cargo.toml, then:
cargo build --release -p graphify-cli  # rebuilds with new version
git add Cargo.toml Cargo.lock
git commit -m "fix: bump version to X.Y.Z"
git tag vX.Y.Z
git push origin main --tags            # triggers CI release
cargo install --path crates/graphify-cli --force  # refresh ~/.cargo/bin/graphify — CI release only builds downloadable artifacts, not the local PATH binary
```

### Current workflow

- Solo-dev mode: changes may go directly to `main`
- Releases are published from pushed tags, not from PR merges
- Keep `Cargo.lock` aligned with workspace version bumps to avoid post-release CI drift
- `cargo install --path …` after every version bump — `graphify --version` is the cheap drift check; expect 0.11.1-era binaries on PATH if you skipped this step on a prior release

## Design docs

- **Spec**: `docs/superpowers/specs/2026-04-12-graphify-rust-rewrite-design.md`
- **Plan**: `docs/superpowers/plans/2026-04-12-graphify-rust-rewrite.md`
- **BUG-001 design**: `docs/plans/2026-04-12-bug-001-python-relative-import-design.md`
- **FEAT-008 spec**: `docs/superpowers/specs/2026-04-12-feat-008-confidence-scoring-design.md`
- **FEAT-008 plan**: `docs/superpowers/plans/2026-04-12-feat-008-confidence-scoring.md`
- **FEAT-010 spec**: `docs/superpowers/specs/2026-04-13-feat-010-watch-mode-design.md`
- **FEAT-010 plan**: `docs/superpowers/plans/2026-04-13-feat-010-watch-mode.md`
- **FEAT-002 spec**: `docs/superpowers/specs/2026-04-13-feat-002-architectural-drift-detection-design.md`
- **FEAT-002 plan**: `docs/superpowers/plans/2026-04-13-feat-002-architectural-drift-detection.md`
- **FEAT-015 spec**: `docs/superpowers/specs/2026-04-14-feat-015-pr-summary-cli-design.md`
- **FEAT-015 plan**: `docs/superpowers/plans/2026-04-14-feat-015-pr-summary-cli.md`
- **FEAT-019 spec**: `docs/superpowers/specs/2026-04-15-feat-019-php-support-design.md`
- **FEAT-019 plan**: `docs/superpowers/plans/2026-04-15-feat-019-php-support.md`

## AI integrations

Source in `integrations/`; installed via `graphify install-integrations`.

- Agents: `graphify-analyst` (Opus, MCP-preferred), `graphify-ci-guardian` (Haiku, CLI-only)
- Skills: `graphify-onboarding`, `graphify-refactor-plan`, `graphify-drift-check`
- Commands: `/gf-setup`, `/gf-analyze`, `/gf-onboard`, `/gf-refactor-plan`, `/gf-drift-check`
- Spec: `docs/superpowers/specs/2026-04-15-feat-018-ai-integrations-design.md`

## Task tracking

- Sprint board: `docs/TaskNotes/Tasks/sprint.md`
- Task files: `docs/TaskNotes/Tasks/BUG-*.md` (TaskNotes format with YAML frontmatter)
- Always cross-reference task status against actual codebase — tasks may be stale
