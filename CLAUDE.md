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
- Consolidation regex validation is fail-fast at config load; invalid pattern aborts `load_config` with the offending pattern surfaced
- `analysis.json` gains `allowlisted_symbols: [...]` only when a `[consolidation]` section is present (absent section = legacy JSON shape, backward compatible)
- `--ignore-allowlist` debug flag on `run` / `report` / `check` bypasses the allowlist for troubleshooting
- `check`'s `max_hotspot_score` gate skips allowlisted nodes when picking candidates — intentional mirrors never trip CI on their own score
- FEAT-020 deferred subtasks (tracked as FEAT-022/023/024/DOC-001): `graphify consolidation` subcommand, `[consolidation.intentional_mirrors]` drift suppression, `pr-summary` annotation strip, README migration note
- `graphify consolidation --config <PATH>` (FEAT-022 landed in 1be5225): emits `consolidation-candidates.json` per project + top-level aggregate when 2+ projects configured; flags `--ignore-allowlist` (keeps hits tagged `allowlisted: true`), `--min-group-size N` (default 2), `--format json|md`; JSON schema `schema_version: 1` with `alternative_paths: []` always present (reserved for FEAT-021); pure renderer lives in `graphify_report::consolidation`
- `tn` feasibility-check heuristics (relevant when authoring task bodies for `/tn-plan-session`): bodies fail with `body is stub` when they contain placeholder prose (`Description here.`, empty `Subtask 1/2`), and fail with `referenced file not found` when they contain fully-qualified paths to files that don't exist in the repo — including `*.rs` paths for files this task will *create*. Phrase new files as "a new module under `crates/<crate>/src/`" (directory-only) instead; JSON examples should use placeholder strings that don't end in a recognizable source-file extension
- `tn` body-is-stub structural trap (CHORE-002): `is_stub_body_str` only counts prose between `# Title` and the first `## …` heading as "description" — so a richly authored body that nests everything under `## Description` / `## Motivation` / `## Subtasks` (leaving the strip directly under `# Title` empty) is still rejected as `body is stub`. Workaround when authoring tasks for `/tn-plan-session`: write a 1–3 line TL;DR paragraph directly under `# Title`, *before* the first `## …` heading. The tn-side fix is tracked as a follow-up in `tasknotes-cli`; until it lands, the TL;DR is mandatory for any task that will flow through the planner's feasibility gate
- TS barrel re-export collapse (FEAT-021 Part B slice, `0cf10ed`): `run_extract` aggregates per-file `ReExportEntry`s from Part A, builds a project-wide `ReExportGraph`, and walks every TS symbol back to its canonical declaration. Outcomes: `Canonical` drops the barrel symbol node and appends the dropped id + intermediates to the canonical node's `alternative_paths` (deduped, order-stable); raw edges whose src/target are collapsed ids get rewritten; `Cycle` logs to stderr and leaves participants; `Unresolved` logs an `Info:` stderr line and leaves the node as-is (no confidence downgrade — v1 policy locked by FEAT-025). All seven report writers now emit `alternative_paths` when non-empty (FEAT-025): JSON (serde skip-empty), CSV (`alternative_paths` column, pipe-joined), Markdown (`### Alternative Import Paths` subsection under hotspots), HTML (inline field on `GRAPHIFY_DATA.nodes[i]`, surfaced in the tooltip suffix), Neo4j Cypher (node property as list literal), GraphML (`alternative_paths` data key, pipe-joined string since GraphML has no array type), Obsidian (YAML sequence in the per-node frontmatter). Still deferred on FEAT-025 follow-ups: **module-level `Imports` edges** still point at barrel modules (TS extractor doesn't capture named imports yet), regression on the reference monorepo, and the tsconfig-paths-through-barrels open question
- `resolve_ts_relative` `is_package` fix (bundled in `0cf10ed`): the TS relative-import resolver unconditionally popped the current-module leaf, so `./entities` from `src/domain/index.ts` resolved to `src.entities` instead of `src.domain.entities` — same bug shape as BUG-001 on the Python side. Now honours `DiscoveredFile.is_package` symmetric with the Python resolver. New public helper `ModuleResolver::is_local_module(id)` so the re-export walker can answer "stop at the package boundary" without reaching into `known_modules` directly

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
```

### Current workflow

- Solo-dev mode: changes may go directly to `main`
- Releases are published from pushed tags, not from PR merges
- Keep `Cargo.lock` aligned with workspace version bumps to avoid post-release CI drift

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
