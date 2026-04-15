# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Graphify

Graphify is a Rust CLI tool for architectural analysis of codebases. It extracts dependencies from Python and TypeScript source code using tree-sitter AST parsing, builds knowledge graphs with petgraph, and generates structured reports identifying architectural hotspots, circular dependencies, and community clusters.

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
cargo test --workspace                     # all 269 tests
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
| `crates/graphify-extract/src/resolver.rs` | Module resolver (Python relative w/ `is_package`, TS path aliases) |
| `crates/graphify-extract/src/cache.rs` | ExtractionCache — SHA256-based per-file extraction cache |
| `crates/graphify-extract/src/walker.rs` | File discovery + dir exclusion + `is_package` detection |
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
- Tests: 493 unit + integration tests (`cargo test --workspace`)

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

## AI integrations

Source in `integrations/`; installed via `graphify install-integrations`.

- Agents: `graphify-analyst` (Opus, MCP-preferred), `graphify-ci-guardian` (Haiku, CLI-only)
- Skills: `graphify-onboarding`, `graphify-refactor-plan`, `graphify-drift-check`
- Commands: `/gf-analyze`, `/gf-onboard`, `/gf-refactor-plan`, `/gf-drift-check`
- Spec: `docs/superpowers/specs/2026-04-15-feat-018-ai-integrations-design.md`

## Task tracking

- Sprint board: `docs/TaskNotes/Tasks/sprint.md`
- Task files: `docs/TaskNotes/Tasks/BUG-*.md` (TaskNotes format with YAML frontmatter)
- Always cross-reference task status against actual codebase — tasks may be stale
