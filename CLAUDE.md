# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

See also: [[AGENTS.md]]

## What is Graphify

Graphify is a Rust CLI tool for architectural analysis of codebases. It extracts dependencies from Python, TypeScript, Go, Rust, and PHP source code using tree-sitter AST parsing, builds knowledge graphs with petgraph, and generates structured reports identifying architectural hotspots, circular dependencies, and community clusters.

Distributed as a standalone binary (no runtime dependencies). Targets macOS + Linux.

Current version: see `[workspace.package].version` in root `Cargo.toml` (currently `0.13.0`). Per-feature version notes in this file may lag — `Cargo.toml` is the source of truth.

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
graphify compare report/main/my-app report/feature/my-app --left-label main --right-label feature

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
cargo build --release -p graphify-cli -p graphify-mcp
# Binaries at target/release/graphify and target/release/graphify-mcp

# Install both binaries to ~/.cargo/bin
cargo install --path crates/graphify-cli --force
cargo install --path crates/graphify-mcp --force

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
    Walker: discover files (.py, .ts/.tsx, .go, .rs, .php)
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
| **graphify-core** | |
| `crates/graphify-core/src/types.rs` | Node, Edge, Language, NodeKind, EdgeKind, ConfidenceKind |
| `crates/graphify-core/src/graph.rs` | CodeGraph — petgraph wrapper with dedup + weight increment + `filter_edges` |
| `crates/graphify-core/src/metrics.rs` | Betweenness, PageRank, unified scoring (filters ExpectedExternal edges) |
| `crates/graphify-core/src/community.rs` | Louvain + label propagation + Leiden refinement + greedy bisection cascade; cohesion score |
| `crates/graphify-core/src/cycles.rs` | Tarjan SCC + simple cycles + `find_*_excluding` for barrel suppression |
| `crates/graphify-core/src/query.rs` | QueryEngine — search, path, explain, stats; `ExplainEdge` enriched output |
| `crates/graphify-core/src/diff.rs` | AnalysisSnapshot, EdgeSnapshot, DiffReport, compute_diff() |
| `crates/graphify-core/src/history.rs` | Trend snapshots (HistoricalCommunity, captured_at) — distinct from analysis snapshots |
| `crates/graphify-core/src/contract.rs` | Architectural contract types (used by drizzle/ts_contract extractors) |
| `crates/graphify-core/src/policy.rs` | GlobMatcher + check policy types |
| `crates/graphify-core/src/consolidation.rs` | Consolidation candidate types + allowlist matching |
| `crates/graphify-core/src/stubs.rs` | ExternalStubs prefix matcher (settings + project lists chained, longest-prefix wins; `matching_prefix` returns the actual matched stub — BUG-021) |
| **graphify-extract** | |
| `crates/graphify-extract/src/python.rs` | Python extractor (imports, defs, calls; same-file binding filter) |
| `crates/graphify-extract/src/typescript.rs` | TypeScript extractor (imports, exports, require, calls; same-file binding filter) |
| `crates/graphify-extract/src/php.rs` | PHP extractor (namespace, use, class/interface/trait/enum/function, calls) |
| `crates/graphify-extract/src/go.rs` | Go extractor (package, imports, funcs, methods, calls) |
| `crates/graphify-extract/src/rust_lang.rs` | Rust extractor (mod, use, items, calls; emits all bare Calls) |
| `crates/graphify-extract/src/resolver.rs` | ModuleResolver — Python relative, TS path aliases, PHP PSR-4, Rust `crate::`/`super::`, case-8.5 bare-id synthesis, workspace-alias |
| `crates/graphify-extract/src/walker.rs` | File discovery + dir exclusion + `is_package` detection + PSR-4 path translation |
| `crates/graphify-extract/src/cache.rs` | ExtractionCache — SHA256-based per-file cache (key: version + local_prefix + bytes) |
| `crates/graphify-extract/src/lang.rs` | Shared lang-extraction types (NamedImportEntry, ReExportEntry, ExtractionResult) |
| `crates/graphify-extract/src/reexport_graph.rs` | Per-project ReExportGraph + canonical-resolution walker |
| `crates/graphify-extract/src/workspace_reexport.rs` | WorkspaceReExportGraph — cross-project TS alias fan-out (FEAT-028) |
| `crates/graphify-extract/src/ts_contract.rs` | TS contract extraction (architectural rules from JSDoc/comments) |
| `crates/graphify-extract/src/drizzle.rs` | Drizzle ORM schema → contract extraction |
| **graphify-report** | |
| `crates/graphify-report/src/json.rs` | analysis.json writer (nodes + communities + cycles + edges array) |
| `crates/graphify-report/src/csv.rs` | graph_nodes.csv / graph_edges.csv |
| `crates/graphify-report/src/markdown.rs` | architecture_report.md |
| `crates/graphify-report/src/html.rs` | Interactive HTML visualization (D3.js force graph, self-contained) |
| `crates/graphify-report/src/neo4j.rs` | Neo4j Cypher import script |
| `crates/graphify-report/src/graphml.rs` | GraphML XML (yEd, Gephi compatible) |
| `crates/graphify-report/src/obsidian.rs` | Obsidian vault export (one .md per node, wikilinks) |
| `crates/graphify-report/src/diff_json.rs` / `diff_markdown.rs` | Drift report output |
| `crates/graphify-report/src/check_report.rs` | Public CheckReport / ProjectCheckResult / CheckViolation / CheckLimits types |
| `crates/graphify-report/src/pr_summary.rs` | Pure `render(project, analysis, drift, check) -> String` for `graphify pr-summary` |
| `crates/graphify-report/src/smells.rs` | Pure `score_smells(analysis, drift, top_n)` — composite edge scoring for pr-summary |
| `crates/graphify-report/src/consolidation.rs` | Consolidation-candidates renderer (json + md) |
| `crates/graphify-report/src/contract_json.rs` / `contract_markdown.rs` | Contract violation reports |
| `crates/graphify-report/src/trend_json.rs` / `trend_markdown.rs` | Trend snapshots over time |
| `crates/graphify-report/src/compare_json.rs` | `graphify compare` output (left vs right analysis.json) |
| **graphify-cli** | |
| `crates/graphify-cli/src/main.rs` | CLI, config parsing, pipeline orchestration, watch mode |
| `crates/graphify-cli/src/watch.rs` | WatchFilter + affected-project detection |
| `crates/graphify-cli/src/install/` | `graphify install-integrations` — Claude Code / Codex artifact installer |
| **graphify-mcp** | |
| `crates/graphify-mcp/src/main.rs` | MCP server entry point, config parsing, extraction pipeline |
| `crates/graphify-mcp/src/server.rs` | GraphifyServer + 9 MCP tool handlers, ServerHandler impl |

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
- Defines-target seeding (BUG-018): after the barrel-collapse pass and before edge resolution, `run_extract_with_workspace` (graphify-cli) and `run_extract` (graphify-mcp) iterate `all_raw_edges` and call `resolver.register_module(raw_target)` for every `EdgeKind::Defines` — seeds `ModuleResolver::known_modules` with symbol-level ids so local Calls don't get capped to `0.5/Ambiguous`
- Resolver case 8.5 (BUG-019): bare identifiers (`no ::, no ., no \, no /, no leading dot`) with non-empty `from_module` synthesize `{from_module}.{raw}` and look it up in `known_modules` *before* the use-alias fallback. Local-first priority matches Rust shadowing semantics. Language-agnostic by design — covers Rust/Go/PHP (emit-everything extractors); Python/TS do NOT need it because they filter same-file helper calls at extraction via `collect_imported_bindings`. **This asymmetry is load-bearing — do not "harmonize" the extractors in either direction**
- Resolver case 8.6 (BUG-022): Rust-shaped scoped paths (`Foo::bar`, `Foo::Bar::baz`, …; `contains :: AND no ., \, /, leading dot`) with non-empty `from_module` synthesize `{from_module}.{raw with :: → .}` and look it up in `known_modules` — same shape as case 8.5 but for scoped input. Covers two concrete patterns FEAT-031's use-alias fallback doesn't reach: (a) same-file `Type::method` (no `use` clause for a same-file type, so `use_aliases` has no entry), and (b) sibling-mod from crate root (`pub use walker::{...};` in `lib.rs` emits scoped imports without a `crate::` prefix; `from_module == local_prefix` makes the synthesis match the canonical `{local_prefix}.{mod}.{sym}`). Ordered before case 9 to match Rust shadowing semantics — same-module symbol shadows aliased imports. The existing BUG-019 negative-guard test (`bug_019_scoped_call_skips_bare_synthesis`) still passes because its pathological plant has a literal `::` that doesn't match the normalized synthesis (`.`). Language-agnostic shape filter (`::` is Rust-specific by design)
- Rust nested grouped use-statements (BUG-023): `collect_use_paths` `scoped_use_list` arm delegates to `process_scoped_use_list(list_node, …, prefix, …)` (helper in `crates/graphify-extract/src/rust_lang.rs`). The helper handles four child kinds (`identifier|self`, `scoped_identifier`, `use_as_clause`, `scoped_use_list`) with a shared `join` closure; the `scoped_use_list` arm fetches the nested group's inner `path` field and recurses with `combined_prefix = prefix::inner_path` instead of grabbing literal text. Pre-fix, `use foo::{bar::{baz, qux}}` produced a single `foo::bar::{baz, qux}` edge with braces preserved and no `use_aliases` entries for the nested leaves. The fix only covers TOP-LEVEL nested groups; function-scoped `use` declarations (e.g. `use toml_edit::{...}` inside `apply_suggestions`) used to remain invisible — closed by BUG-025
- Function-body `use_declaration` walking (BUG-025): new helper `walk_for_uses(node, source, module_name, result)` in `crates/graphify-extract/src/rust_lang.rs` recurses through function/method bodies dispatching `use_declaration` nodes to the existing `extract_use_declaration` (which already handles all use-shapes — scoped, grouped, aliased, wildcard). Skip discipline mirrors BUG-024's `walk_for_bindings`: `function_item` and `impl_item` subtrees return without descending so a `use foo::Bar;` inside `fn inner()` does not leak `Bar`'s alias into outer-fn semantics. Wired into `extract_function_item` (after `collect_local_bindings`, before `extract_calls_recursive`) and `extract_impl_item` (per-method body loop). Approximation accepted (documented in helper docstring): aliases land in the file-wide `result.use_aliases` map, so a function-scoped use becomes visible to other fns in the same file — last-write-wins is harmless when both fns import the same path; truly per-scope alias map is a v2 refactor with no current consumer. Self-dogfood: `graphify suggest stubs` 9 → 7 (Item/Array/Value collapsed under canonical `toml_edit` once aliases populated). Out of scope: full nested `function_item` extraction (no Defines for nested fns, no Calls captured inside them) — file as separate task only when user-visible
- Closure / let-binding / nested-fn local scope (BUG-024): `extract_calls_recursive` takes a `local_bindings: &HashSet<String>` parameter, populated per-function by `collect_local_bindings(body, source)`. The helper walks the body once via `walk_for_bindings` and collects names from `let_declaration` (single-identifier `pattern` only — tuple/struct destructuring is v2) AND from nested `function_item`. Per-function scope is load-bearing: descent into nested `function_item` and `impl_item` collects the NAME then `return`s without entering the body, so a binding in fn A does not leak into fn B's set. The `identifier` arm of `call_expression` skips emission when the callee ∈ set; `scoped_identifier` arm unchanged (`Type::method()` doesn't fit the false-positive pattern). Three call sites of `extract_calls_recursive` thread the parameter: top-level fallback passes an empty `HashSet::new()`; `extract_function_item` and `extract_impl_item` pre-scan their bodies. **GREEN-cycle gotcha (worth re-living)**: dogfood predicted `sort_key` would drop after the let-binding fix; it didn't. Investigation found `sort_key` was a nested `fn` (in `crates/graphify-core/src/contract.rs::compare_violations`), not a let. Helper extended from let-only to let+fn — without that step the v1 fix would have left a misleading hole. **Stretch out of scope**: `matches!` macro stripping (lands as bare `matches` per FEAT-031, ~5 edges) and `std::env` bare references (~2 edges) are different fix shapes (macro recognizer / stdlib heuristic) and were intentionally NOT covered by BUG-024. File as `BUG-026`/`BUG-027` only when user-visible
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
- PHP projects should leave `[[project]].local_prefix` unset — PSR-4 mappings from `composer.json` already provide the namespace prefix structure. Resolver case 7 (PHP `use`-target lookup) does not re-apply `local_prefix`, so setting one is silently ignored today; `load_config` emits a non-fatal stderr warning to steer users away from the combination (DOC-002).
- `graphify_extract::walker::discover_files_with_psr4` is the PSR-4-aware discovery entry; `discover_files` remains a thin wrapper for non-PHP projects
- `[consolidation]` section in `graphify.toml` (FEAT-020 slice landed in 25eabc8): `allowlist = ["pattern", …]` — regex anchored `^…$`, matched against the **leaf symbol name** (last dot-segment of a node id), not the full id; so `TokenUsage` hits `app.models.TokenUsage` but not `app.models.TokenUsageAdapter`
- `[consolidation].suppress_barrel_cycles = true` (BUG-015 opt-in, default `false`): drops cycles whose only cycle-making edges route through the project's root barrel node, **but only** when that barrel node (`id == local_prefix`) is also matched by the `allowlist`. Counters FEAT-028 synthetic barrel cycles on consumers like `parisgroup-ai/cursos` where `src/index.ts` is the npm entry point. Implementation in `graphify-core/src/cycles.rs::{find_sccs_excluding, find_simple_cycles_excluding}` (uses `petgraph::visit::NodeFiltered` for SCC, skip-neighbor during DFS for simple cycles) plus `graphify-cli/src/main.rs::barrel_exclusion_ids`. Applied at all 4 `run_analyze` call sites (Analyze, Diff baseline-vs-live, Run pipeline helper, Check); query engine does NOT apply it (no consolidation config in scope for `query`/`explain`/`path`/`shell`). `--ignore-allowlist` debug flag disables barrel suppression too since it disables the whole consolidation config
- Consolidation regex validation is fail-fast at config load; invalid pattern aborts `load_config` with the offending pattern surfaced
- `analysis.json` gains `allowlisted_symbols: [...]` only when a `[consolidation]` section is present (absent section = legacy JSON shape, backward compatible)
- `--ignore-allowlist` debug flag on `run` / `report` / `check` bypasses the allowlist for troubleshooting
- `check`'s `max_hotspot_score` gate skips allowlisted nodes when picking candidates — intentional mirrors never trip CI on their own score
- `graphify consolidation --config <PATH>`: emits `consolidation-candidates.json` per project + top-level aggregate when 2+ projects; flags `--ignore-allowlist`, `--min-group-size N` (default 2), `--format json|md`; pure renderer in `graphify_report::consolidation`
- `tn` feasibility-check heuristics (relevant when authoring task bodies for `/tn-plan-session`): bodies fail with `body is stub` when they contain placeholder prose (`Description here.`, empty `Subtask 1/2`), and fail with `referenced file not found` when they contain fully-qualified paths to files that don't exist in the repo — including `*.rs` paths for files this task will *create*. Phrase new files as "a new module under `crates/<crate>/src/`" (directory-only) instead; JSON examples should use placeholder strings that don't end in a recognizable source-file extension
- `tn` frontmatter strict-enum trap: `ai.uncertainty` accepts only `low|med|high`. Setting `medium` (or any other value) makes `tn list`/`tn sprint summary` silently skip the file with a `yaml parse error` warning to stderr — task disappears from sprint counts and table output, but the file still exists in `docs/TaskNotes/Tasks/`. Symptom: sprint shows "Open: N" but a follow-up `tn list --status open` shows N+1 entries (or vice versa, depending on which counter the operator looks at first). Surfaced 2026-04-26 by `/session-start` when BUG-024 was missing from the open-tasks table — CLAUDE-side fix is to validate the enum before saving any frontmatter edit; `tn`-side fix would be to surface the parse failure more loudly (not just stderr warning). Same shape applies to other `ai.*` enum fields if/when added
- `tn` body-is-stub structural trap (CHORE-002): `is_stub_body_str` only counts prose between `# Title` and the first `## …` heading as "description" — so a richly authored body that nests everything under `## Description` / `## Motivation` / `## Subtasks` (leaving the strip directly under `# Title` empty) is still rejected as `body is stub`. Workaround when authoring tasks for `/tn-plan-session`: write a 1–3 line TL;DR paragraph directly under `# Title`, *before* the first `## …` heading. The tn-side fix is tracked as a follow-up in `tasknotes-cli`; until it lands, the TL;DR is mandatory for any task that will flow through the planner's feasibility gate
- TS barrel re-export collapse (FEAT-021/025/026): `run_extract` aggregates per-file `ReExportEntry`s into a project-wide `ReExportGraph` and walks every TS symbol to its canonical declaration. Outcomes — `Canonical`: drop barrel symbol nodes, append dropped + intermediate ids to canonical's `alternative_paths` (deduped, order-stable); `Cycle`: stderr log, leave participants; `Unresolved`: `Info:` stderr log, leave node as-is (no confidence downgrade — v1 policy). All 7 report writers emit `alternative_paths` when non-empty. TS extractor captures named + default import specifiers (`NamedImportEntry`) and fans out per specifier; `import * as ns` stays a single barrel edge; type-only imports captured with `is_type_only: true` and contribute weight 1; `Unresolved`/`Cycle` fall back to a barrel-targeted edge (no import dropped). Same-project tsconfig aliases (`@app/*`) covered post-FEAT-026; cross-project covered by FEAT-028
- Cross-project TS aliases (FEAT-028): `@repo/*` spanning sibling `[[project]]`s fan out via `WorkspaceReExportGraph` (`crates/graphify-extract/src/workspace_reexport.rs`). Built only when ≥2 projects AND ≥1 TS project — single-project / non-TS configs keep the legacy fast path. Pipeline split: `build_project_reexport_context` (phase 1, collect) + `run_extract_with_workspace` (phase 2, fan-out against merged workspace graph). Public node ids stay per-project; cross-project edges reference target's `module_id` verbatim (first-wins via `modules_to_project` index, collisions logged). `match_alias_target` supports inner-glob tsconfig forms (`"@repo/*": ["../../packages/*/src"]`). Opt-out: `[settings] workspace_reexport_graph = false` (ADR: `docs/adr/0001-workspace-reexport-graph-gate.md`)
- Rust intra-crate `pub use` collapse (FEAT-045/046/047, design: `docs/superpowers/specs/2026-04-26-feat-044-rust-reexport-collapse-design.md`): mirrors the TS FEAT-021/025/026 architecture for Rust. **Asymmetry vs TS** — Rust `pub use foo::Bar;` does NOT create a barrel symbol node (extractor emits an `Imports` edge, not a node), so the TS Part B "drop barrel symbol node + append `alternative_paths`" step is a NO-OP for Rust; the load-bearing piece is the consumer-side edge-target rewrite. **FEAT-045** (`crates/graphify-extract/src/rust_lang.rs`): `extract_use_declaration` checks `visibility_modifier` (first named child of `use_declaration`) — any text starting with `pub` (covers `pub`, `pub(crate)`, `pub(super)`, `pub(in path)`) emits a `ReExportEntry` on `ExtractionResult.reexports` in addition to the existing `Imports` + `use_aliases` registration; mixed groups bucket by combined prefix (one `ReExportEntry` per bucket). Wildcards (`pub use foo::*;`) and single-segment `pub use foo;` are intentional v1 no-ops. **FEAT-046** (`crates/graphify-cli/src/main.rs::run_extract_with_workspace`): the existing `has_ts_reexport_work` gate was renamed to `has_reexport_work` and widened to `TypeScript || Rust`; a Rust resolver callback applies `local_prefix` + `known_modules` lookup the same way TS does, walks `resolve_canonical` per `ReExportEntry`, accumulates `barrel_to_canonical` + `canonical_to_alt_paths`, and a post-`resolver.resolve()` rewrite step repoints edge targets at canonical declarations using BOTH exact-match AND prefix-match shapes (the prefix-match path — `src.Bar.new` → `src.foo.Bar.new` — is required because Rust raw_targets aren't module-shaped pre-resolution; helper `rewrite_via_barrel_prefix`). **FEAT-047** (`crates/graphify-extract/src/resolver.rs::ModuleResolver::rewrite_use_alias_targets(barrel_to_canonical, is_package)` — public; helper `canonical_to_crate_path` private): after `barrel_to_canonical` is built and BEFORE the resolver pass that consumes `use_aliases` (case 9 fallback in `resolve_with_depth`), iterates `use_aliases_by_module` and rewrites alias TARGETS that match a `barrel_to_canonical` key. Per-file `use_aliases` is per-file-not-per-scope (BUG-025 trade-off documented inline at the rewrite site); a file with both `use crate::Bar;` (re-export consumer) and `use other::Bar;` (shadowing import) misroutes one of them under last-write-wins
- Cross-crate Rust `pub use` workspace fan-out (FEAT-048) is deferred via gate. ADR `docs/adr/0002-cargo-workspace-reexport-graph-gate.md` documents the threshold (≥5 cross-crate misclassifications in `graphify suggest stubs`) and re-open criteria. As of 2026-04-27 the workspace shows 1 hit (`graphify-report`'s `pub use graphify_core::community::Community;`), below threshold. Plumbing for cross-crate ReExportGraph would mirror TS FEAT-028's `WorkspaceReExportGraph` + `build_project_reexport_context` two-phase split. **Re-open trigger**: any consumer project hitting ≥5 cross-crate `pub use` candidates, OR a single high-edge cross-crate hit (~50+ edges) becoming user-visible
- Rust `static_item`, `const_item`, and enum variants — local-symbol registration via `Defines` edge (BUG-027, post-v0.13.5). Top-level match in `extract_file` (`crates/graphify-extract/src/rust_lang.rs`) gained `"static_item" | "const_item"` arms delegating to a new `extract_value_item` helper (same shape as FEAT-049's `extract_type_item` — `extract_named_type` with `NodeKind::Class`). `extract_enum_item` extended to walk the `body` field's `enum_variant` children after the existing enum-name Defines, emitting one Defines edge per variant at `{module}.{Enum}.{Variant}` so resolver case 8.6 (BUG-022) finds the synthesized id in `known_modules` for bare `Selector::Group(...)` callsites. Visibility never gates Defines emission (matches struct/enum/trait/type behavior). Two dogfood misclassifications closed: `pub static INTEGRATIONS` (graphify-cli, qualified via `use_aliases` case 9 but unseeded) and `enum Selector { Project, Group }` (graphify-core, synthesized via case 8.6 but unseeded). Self-dogfood: `graphify suggest stubs` candidate count 4 → 2 — only `env` (BUG-026, separate stdlib-prefix-stripping shape) and `src.Community` (FEAT-048 deferred gate) remain. **Out of scope**: tuple-struct field emission (`struct Foo(u32)` accessing `.0`), full nested `function_item` extraction, cross-crate enum variant resolution (FEAT-048 territory). Existing fixture `full_rust_file` test bumped to 8 nodes / 7 Defines (was 6 / 5) to reflect the 2 new variant edges
- Rust `pub type X = Y;` (and rare non-pub `type X = Y;`) — local-symbol registration via `Defines` edge (FEAT-049, v0.13.5). Top-level match in `extract_file` (`crates/graphify-extract/src/rust_lang.rs`) gained a `"type_item"` arm delegating to `extract_type_item`, which calls the existing `extract_named_type` helper with `NodeKind::Class`. Same shape as struct/enum/trait — emits a `Defines` edge from the module to `{module}.{alias_name}` so BUG-018's seeding pass registers the alias in `known_modules`. **Design pivot from task body**: FEAT-049's design doc proposed an Option 1 (treat `pub type X = Y;` as a `ReExportEntry`) mirroring FEAT-045/046, but the actual dogfood case (`pub type Cycle = Vec<String>;` in `crates/graphify-report/src/lib.rs`) has a `generic_type` RHS that doesn't fit the canonical-collapse model — `Vec<String>` isn't a path to a re-exportable symbol. The structural Defines fix is sufficient and ~10 LOC vs an Option 1 implementation that would need RHS path-extraction across `generic_type`, `scoped_type_identifier`, `reference_type`, `tuple_type`. RHS canonical-collapse (mapping `Foo` → `mod::Bar` for path-shaped RHS) is intentionally out of scope; if a path-only `pub type` use case appears, FEAT-046's plumbing is reusable. **NodeKind reuse**: `Class` is the closest semantic fit; adding a `TypeAlias` variant would cascade through every report writer + match arm (FEAT-049 close).  Self-dogfood: `graphify suggest stubs` candidate count 7 → 6, `src.Cycle` (4 edges in graphify-report) collapsed onto canonical local symbol
- TS relative-import resolver honours `DiscoveredFile.is_package` (parity with Python BUG-001 fix) — `./entities` from `src/domain/index.ts` resolves to `src.domain.entities`, not `src.entities`. Public helper `ModuleResolver::is_local_module(id)` lets the re-export walker check package boundaries without touching `known_modules`
- Resolver re-applies `local_prefix` on every language-specific branch (BUG-016, BUG-001, BUG-007/011 share this shape). `ModuleResolver` carries `local_prefix: String` + `set_local_prefix()` + private `apply_local_prefix(id)` helper, wired from `run_extract_with_workspace`, `build_project_reexport_context`, and the MCP extraction pipeline. Rust `crate::types::Node` resolves to `{prefix}.types.Node`; `super::`/`self::` already worked because `resolve_rust_path` walks up from `from_module` (which carries the prefix). FEAT-031 closes the bare-name gap (`Node::new(...)` after `use crate::types::Node;`) via per-file `use_aliases: HashMap<String, String>` on `ExtractionResult` consulted by the post-extraction resolver pass
- Self-dogfood: `graphify.toml` at repo root with 5 `[[project]]` blocks (one per crate), `lang = ["rust"]`, no `local_prefix` (auto-detect → `src`). `report/` is gitignored (subdir per crate). `[settings].external_stubs` lifts the Rust prelude (std, Vec, Some/None, format/writeln, assert*, etc.) so per-project lists stay focused on cross-crate references. CLI ↔ MCP cross-edge pair (42 shared modules each direction) confirms the documented "MCP server config is duplicated from CLI" debt
- `[settings].external_stubs: Vec<String>` is concatenated ahead of `[[project]].external_stubs` (not override) at both `ExternalStubs::new(...)` call sites. `ExternalStubs::new` sorts by descending prefix length and dedupes — overlap between sources is harmless. `graphify init` template advertises the shared layer
- `graphify suggest stubs` (FEAT-043): post-analysis subcommand consuming each project's `graph.json` (NOT `analysis.json` — needs `is_local` per node and `weight` per link, neither in analysis.json). Pure scorer + 3 renderers in `graphify_report::suggest`; CLI orchestration + `--apply` (`toml_edit::DocumentMut` + `tempfile` atomic rename, idempotent) in `graphify_cli::main::cmd_suggest_stubs`. Auto-classify: prefix surviving `--min-edges` per-project in ≥2 projects → `[settings]` candidate; in 1 project → that project's per-project block. Language-aware prefix extraction keys on the `language` string from `GraphNode` (Debug-formatted enum: `"Rust"`, `"Python"`, `"TypeScript"`, `"Php"`, `"Go"`); rules: Rust first `::`-segment, Python/PHP first `.`-segment, TS scoped `@scope/name` (2 segments) else first `/`-segment, Go path-style (3 segments if seg-0 contains `.`) else first `.`-segment. Shadowing safety pre-builds a global set of `local_prefix`'s + top-segments of every `is_local=true` node id; matches there are recorded in `shadowed_prefixes` and never suggested. Threshold (`--min-edges`, default 2) applied per-project BEFORE cross-project aggregation — prefix with weight 1×N projects ≠ promotion. `--apply` rejects via `clap conflicts_with` when paired with `--format`. New workspace dep: `toml_edit = "0.22"`. CHANGELOG entry under `## [Unreleased]`. **Layer-crossing debt**: FEAT-043 added `graphify-report → graphify-extract` solely to access `ExternalStubs` — tracked as CHORE-011 (move `ExternalStubs` to `graphify-core`)
- `ExternalStubs` lives in `graphify-core` (CHORE-011, post-FEAT-043 cleanup): pure prefix matcher, no extractor coupling. Public path `graphify_core::ExternalStubs` (re-exported from `crates/graphify-core/src/stubs.rs`). The old `graphify_extract::stubs::ExternalStubs` and `graphify_extract::ExternalStubs` re-exports are GONE — no shim, no deprecation alias. `graphify-report` no longer depends on `graphify-extract`, restoring report = pure renderer over core types. **Heuristic gotcha**: `graphify-summary.json` still surfaces a `graphify-report → graphify-extract` cross-project edge count (~80 edges) — this is name-based module overlap, not Cargo-dep direction. The architectural win is real even when the heuristic still surfaces nominal edges; verify via `grep -rn "graphify_extract" crates/graphify-report/` returning zero or `cargo build -p graphify-report` succeeding without the dep
- `ExternalStubs::matching_prefix(&self, target) -> Option<&str>` (BUG-021): companion to `matches()` that returns the actual stub that matched, not just a boolean. Implementation: `self.prefixes.iter().find(|p| prefix_matches(p, target)).map(String::as_str)` — leverages the existing longest-first sort in `new()` so the first hit IS the longest match (`tokio::runtime` wins over `tokio` for `tokio::runtime::Builder`). Used by `graphify-report::suggest::score_stubs` to record the precise stub in `already_covered_prefixes` instead of `extract_prefix(target, lang)` — the latter falsely surfaced `tokio` as covered when only `tokio::runtime` was registered. Defensive against sub-namespace stubs; behavior unchanged for top-level-only stub configs (the dogfood case)
- **Self-dogfood UX rule (FEAT-043 close, refined post-BUG-022)**: when running `graphify suggest stubs` on this repo and the candidate list contains internal symbols misclassified as external — **do NOT add them to `external_stubs` to silence the symptom**. Adding them masks underlying graphify bugs. Add only legitimate externals (real workspace siblings, real third-party deps, Rust macros). The 5 dogfood adds applied at FEAT-043 close (`include_str`, `graphify_core/extract/report`, `anstyle`) are the template. BUG-022 (resolver case 8.6) brought the dogfood candidate count 35 → 18 by collapsing same-module `Type::method` and sibling-mod-from-crate-root scoped paths. The remaining 18 candidates trace to 3 distinct non-resolver bugs (BUG-023 nested grouped-use parsing, BUG-024 closures emitted as Calls, FEAT-044 Rust re-export collapse) — same rule applies: fix the root, don't paper over with stubs
- **Workspace siblings as external_stubs (FEAT-043 dogfood)**: `[settings].external_stubs` now includes `graphify_core`, `graphify_extract`, `graphify_report` because each crate sees its sibling crates as external (not local to that crate's analysis). This is consistent with the existing pattern of declaring known third-party deps and stops sibling-crate references from inflating hotspot Ambiguity scores. Pattern generalizes to any Cargo workspace analyzed crate-by-crate — each `[[project]]` should declare its sibling crates if they're consumed cross-crate
- Communities carry a `cohesion: f64` score (intra_edges / max_possible_pairs; singletons = 1.0 by convention; surfaced in `analysis.json` and Markdown report). **Two serialization surfaces** must stay in sync: `HistoricalCommunity` (history.rs) AND `CommunityRecord` (graphify-report/src/json.rs). Tests must exercise both (`write_analysis_json_includes_cohesion_per_community` is the regression guard)
- Oversized communities (n > max(10, round(total*0.25))) split via cascade in `split_oversized` (called after `merge_singletons` in both `detect_communities` and `label_propagation`): Louvain local moves → label propagation → Leiden refinement (strictly-positive gain gate, well-connectedness gate, singleton init) → greedy modularity bisection (deterministic 2-way split, lowest-degree seed, frontier growth until A's degree sum ≥ half total). Each stage returns early on ≥2 distinct sub-labels. Communities with zero intra-edges stay untouched (no edge structure to split along — cohesion 0.000 is truthful, not a bug). Recursion depth = 1 by construction. Sub-label allocation deterministic: first sub-label keeps the original global label, fresh labels start at `max(community) + 1`. `analysis.json` `communities` section is deterministic; `metrics` list still shows tied-score reordering (HashMap-iter, pre-existing)
- **`replace_all` test-name collision trap**: renaming a helper across a Rust test module can collapse multiple test fn names into duplicates. Audit fn identifiers before any `replace_all` rename — Rust's compiler catches it but the fix is annoying retroactively
- `graphify pr-summary` emits `#### Architectural smells` from `analysis.json`'s `edges` array. Composite scoring: confidence_bonus (Ambiguous=3, Inferred=2, Extracted=1, ExpectedExternal=0) + cross_community + in_cycle + peripheral→hub (`min_deg ≤ 2 AND max_deg ≥ 5`) + hotspot_adjacent (top-10). Floor=3, tie-break drift-new first then lexicographic. CLI: `--top N` (default 5, `--top 0` suppresses). `EdgeSnapshot` in `AnalysisSnapshot` has `#[serde(default)]` for legacy compat; `graphify diff` live-mode populates `edges: vec![]` (drift doesn't use per-edge data). When both endpoints land in top-10 hotspots, "touches hotspot" names the **target** (leaf-coupling-into-hub shape)
- **Test fixture trap on scoring formulas**: any fixture with ≤10 nodes makes every node implicitly a "top-10 hotspot", so the hotspot-adjacent bonus always fires. Assert on the `reasons` list (which bonus fired) rather than exact score (which other bonuses ALSO fired in the small-fixture accident)
- **External-stub classification is a post-resolution relabel and never enters the cache** (cache key: version + local_prefix + per-file SHA256 only). Editing `external_stubs` and re-running without `--force` correctly reclassifies; if a consumer reports otherwise, suspect a stale binary on PATH (pre-FEAT-034 ignored `[settings].external_stubs`) or a wrong `analysis.json`. BUG-020 evidence checklist in `docs/TaskNotes/Tasks/BUG-020-*.md`. **General lesson**: any feature that relabels post-resolution is automatically cache-safe — the cache key is narrow by design (source bytes only)
- `graphify explain` output is colored + sectioned by `EdgeKind` (Imports → Defines → Calls), per-subsection cap of 10 (`EXPLAIN_MAX_PER_SECTION`, `... and N more` footer), inline confidence tags + score color thresholds + `in_cycle` rendering. `ExplainEdge { target, edge_kind, confidence, confidence_kind }` carries the data; `ExplainPalette::new(forced_off)` honors `--no-color`, `NO_COLOR` env, and `is_terminal()`. **MCP `graphify_explain` returns richer per-edge JSON objects (additive break vs <=v0.12.0)** — re-evaluate if an external consumer surfaces
- **Multi-line golden-string trap in Rust tests**: `\`-continuation strips leading whitespace from the next line, silently corrupting indented snapshots. Use `concat!("line1\n", "  line2\n", …)` instead — each fragment is a separate literal with no continuation semantics
- Hotspot scoring (`compute_metrics_with_thresholds`) runs over a `CodeGraph::filter_edges` view that excludes `ConfidenceKind::ExpectedExternal` edges. Original graph stays untouched and remains the source for `query`/`explain`/report writers. Semantic contract: hotspot view = filtered view, but `graphify explain` still shows external dependents (FEAT-033)

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

- **Changelog**: `CHANGELOG.md` (release-by-release history; the source of truth for "what changed when")
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

- Sprint board: `docs/TaskNotes/sprint.md`
- Task files: `docs/TaskNotes/Tasks/BUG-*.md` (TaskNotes format with YAML frontmatter)
- Always cross-reference task status against actual codebase — tasks may be stale
