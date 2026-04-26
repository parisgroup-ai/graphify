# Changelog

All notable changes to Graphify will be documented in this file.

## [Unreleased]

### Changed
- chore(deps): `ExternalStubs` moved from `graphify-extract` to `graphify-core` and `graphify-report`'s dependency on `graphify-extract` was dropped (CHORE-011). Closes the layer-crossing FEAT-043 introduced when `graphify-report::suggest` started consuming the prefix matcher. The matcher had no extractor coupling — pure `Vec<String>` prefix matching — so it was a clean move with no behavior change. Public import path: `graphify_extract::stubs::ExternalStubs` and `graphify_extract::ExternalStubs` → `graphify_core::ExternalStubs` (both old paths removed; no shim, no deprecation alias). Internal workspace API; no published SDK to preserve. 856 workspace tests still pass; `graphify check` PASS on all 5 crates with 0 cycles; `graphify suggest stubs` candidate count holds at 7 (zero regression). Architecture deltas: graphify-core +6 nodes / +9 edges, graphify-extract -7 nodes / -9 edges (the moved file's contribution).

### Fixed
- fix(extract): function-scoped and impl-method-scoped `use` declarations now register their aliases and emit `Imports` edges (BUG-025). Previously the Rust extractor's `extract_file` only walked `tree.root_node().children(...)`, so a `use foo::Bar;` inside a function body left `use_aliases` unpopulated and downstream bare-name calls like `Bar::new()` were misclassified as external. New helper `walk_for_uses(node, source, module_name, result)` in `crates/graphify-extract/src/rust_lang.rs` recurses through function/method bodies dispatching `use_declaration` nodes to the existing `extract_use_declaration`, with the same per-scope skip discipline as BUG-024's `walk_for_bindings` (returns at nested `function_item` / `impl_item` so a `use` inside a nested fn does not leak into the outer scope's file-wide alias map). Wired into `extract_function_item` and `extract_impl_item`. Self-dogfood: `graphify suggest stubs` candidate count 9 → 7 — `Item`, `Array`, `Value` no longer appear as bare external prefixes; they collapsed under `toml_edit` (13 edges, now showing canonical `toml_edit::Array` etc.) which is correctly suggested as a per-project external for graphify-cli. Out of scope (separate task only if user-visible): full nested `function_item` extraction (no `Defines` for nested fns, no Calls captured inside them).

## [0.13.2] - 2026-04-26

### Added
- feat(cli): `graphify suggest stubs` — post-analysis subcommand that scans each project's `graph.json`, groups external references by language-aware prefix, and recommends additions to `[settings].external_stubs` (cross-project) or `[[project]].external_stubs` (single-project). Auto-classifies cross-project hits via a per-project `--min-edges` threshold (default 2) before promotion; skips prefixes already covered or shadowing a `local_prefix`. Output formats: `md` (default), `toml`, `json`. `--apply` edits `graphify.toml` in place via `toml_edit`, preserving comments and ordering. Idempotent — re-running `--apply` is a no-op. FEAT-043.

### Fixed
- fix(extract): closure bindings, let-bound function pointers, and nested `fn` items inside a function body no longer emit bogus bare-call `Calls` edges (BUG-024). The Rust extractor now pre-scans each function/method body via `collect_local_bindings(body)` (new helper in `crates/graphify-extract/src/rust_lang.rs`) and threads a per-function `local_bindings: &HashSet<String>` into `extract_calls_recursive`. The `identifier` arm of `call_expression` skips emission when the callee is in the set. Per-function scope is honored: `walk_for_bindings` collects names from `let_declaration` (single-identifier patterns) and from `function_item` (the name only — descent stops there), then returns without entering nested fn/impl bodies. `scoped_identifier` calls (`Type::method()`) are unchanged. Self-dogfood: removed `pct`, `write_grouped`, `join`, `threshold`, `sort_key`, `find_sccs`, `sha256_hex` from `graphify suggest stubs` (~16+ edges reclassified). Out of scope (per BUG-024's "Stretch" section, no follow-up filed unless user-visible): `matches!` macro stripping (lands as bare `matches`), `std::env` bare reference. Combined with BUG-023, session-cumulative drop is 18 → 9 candidates (50%).
- fix(extract): nested grouped use-statements (`use foo::{bar::{baz, qux}}`) decompose into per-leaf imports + use-aliases instead of emitting a single edge with literal braces in the target (BUG-023). The Rust extractor's `collect_use_paths` `scoped_use_list` arm previously captured nested `scoped_use_list` children via `utf8_text()`, producing nodes like `foo::bar::{baz, qux}` and registering no aliases for `baz`/`qux`. Refactored to delegate the inner loop to a new `process_scoped_use_list(list_node, …, prefix, …)` helper that recurses with a combined prefix on each nested group. Self-dogfood: `graphify suggest stubs` candidate count 18 → 14, with `ExtractionCache` (was 7 edges) fully eliminated. Function-scoped use-statements still leak `Item`/`Array`/`Value` because `extract_file` only walks `tree.root_node().children(...)` — split out as BUG-025 (Cat 2 of the original BUG-022 finding).
- fix(resolver): scoped same-module and sibling-mod-from-crate-root paths now resolve to local qualified ids instead of falling through as external (BUG-022). Two patterns the FEAT-031 use-alias fallback didn't reach were surfaced by the FEAT-043 dogfood: `PolicyError::new(...)` from inside `policy.rs` (no `use` clause for a same-file type, so case 9's alias map has no entry), and `pub use walker::{...};` blocks in `lib.rs` (tree-sitter emits scoped imports without a `crate::` prefix, and no alias is registered for the leading sibling-mod segment). New case 8.6 in `resolve_with_depth` synthesizes `{from_module}.{raw with :: → .}` and looks it up against `known_modules`; on hit, returns the qualified id with `is_local=true, confidence=1.0`. Ordered before case 9 so a same-module symbol shadows aliased imports (matches Rust resolution semantics). Self-dogfood: `graphify suggest stubs` candidate list 35 → 18 (49% drop) — 17 prefixes (`PolicyError`, `GlobMatcher`, `ExplainPalette`, `walker::*`, `lang::*`, `reexport_graph::*`, `ts_contract::*`, `workspace_reexport::*`, `drizzle::*`, `check_report::*`, `contract_json::*`, `json::*`, `diff_markdown::*`, `install::*`, `manifest::*`, `session::*`, `codex_bridge::*`) collapsed onto canonical local ids. Remaining 18 candidates are 3 distinct non-resolver bugs filed separately (BUG-023 nested grouped-use parsing, BUG-024 closures emitted as Calls, FEAT-044 Rust re-export collapse).

## [0.13.1] - 2026-04-26

### Added
- feat(session): `graphify session brief` consolidates `report/<project>/analysis.json` from every `[[project]]` in `graphify.toml` into a single `.claude/session-context-gf.json` consumed by Claude Code `/session-start` skills and `tn-session-dispatcher` subagents. Cache-aware: regenerates only when an `analysis.json` is newer than the existing brief; `--force` bypasses, `--check` exits 2 on stale (CI-friendly). Top-N hotspots are score-sorted globally across projects (default `--top 10`); cycles surface tagged with project name; baseline staleness warns above `--stale-days` (default 7). FEAT-042.
- feat(session): `graphify session scope --files <a,b,c>` augments an existing brief with structured `graphify explain` output per file. Caps at `--max 5` to keep subagent prompt budgets bounded. The caller passes paths explicitly, so graphify carries no dependency on tasknotes-cli or any consumer task-tracker. Optional `--task <ID>` records the originating task identifier in the merged brief. FEAT-042.

### Notes
- Brief schema is `schema_version: 2`. This is the first native version of the contract — prior bash wrappers in consumers (e.g. `apps/cursos/scripts/gf-context-{brief,scope}.sh`) emitted `schema_version: 1` with two extras the native form drops on purpose: `frozen[]` (consumer-specific list mirroring each project's `CLAUDE.md` frozen-modules section) and a 40-line text blob in `scope_explains[].explain` (now a structured JSON object). Consumers wanting the legacy text or the frozen list can post-process with `jq` or call `graphify explain` directly.

## [0.13.0] - 2026-04-24

### Added
- feat(cli): add `graphify compare <left> <right>` for head-to-head comparison of two existing Graphify analysis outputs. Inputs may be `analysis.json` files or directories containing `analysis.json`; optional labels make PR-vs-PR or branch-vs-branch reports readable.
- feat(report): add compare-oriented outputs, `compare-report.json` and `compare-report.md`, reusing the existing architecture diff engine while preserving `graphify diff` behavior.
- docs(readme): add a compare recipe for branch snapshots and CI/PR artifact directories.

### Changed
- chore(tasks): move the TaskNotes sprint file outside the task list directory so sprint metadata no longer appears as a task document.

## [0.11.10] - 2026-04-21

### Added
- docs(php): `load_config` now emits a non-fatal stderr warning when a `[[project]]` declares `lang = ["php"]` AND a non-empty `local_prefix` — PSR-4 mappings from `composer.json` already provide the namespace prefix structure and resolver case 7 silently ignores `local_prefix` for PHP. `graphify init` template gains an inline comment steering PHP users away from the combination. CLAUDE.md documents the landmine under the PHP conventions cluster. Surfaced by CHORE-007's resolver-branch audit as the only "latent landmine" across all 10 branches; not a bug today, pure preventative documentation so future refactors to case 7 don't retroactively double-prefix existing configs (DOC-002).

## [0.11.9] - 2026-04-21

### Fixed
- fix(extract): bare same-module Calls edges (e.g. `build_communities()` inside `src.community`, where the function is defined in the same file) were being returned as non-local by `ModuleResolver::resolve` — the extractor emits just the bare leaf `build_communities` and no `use` statement triggers a rewrite, so the direct `known_modules.contains_key("build_communities")` lookup missed. New case 8.5 in `resolve_with_depth` synthesizes `{from_module}.{raw}` for bare-identifier input and looks it up against the `known_modules` map seeded by BUG-018's Defines-target pass. On hit, returns the qualified id with `is_local=true, confidence=1.0`. Placement before case 9 matches Rust shadowing semantics: a local `fn foo` shadows `use …foo`, so local-first is correct. Self-dogfood delta on graphify's 5-crate workspace: Ambiguous Calls 394 → 144 (−63%), Inferred 13 → 263 (20×); +250 promotions. Graph node counts dropped 5–15% per crate as bare-identifier placeholders collapsed into canonical symbols. Top hotspots now surface real hub modules (`src.policy`, `src.resolver`, `src.consolidation`, `src.install.codex_bridge`, `src.server`) instead of uniformly-capped facades (BUG-019).

## [0.11.8] - 2026-04-21

### Fixed
- fix(extract): local Calls edges that FEAT-031's `use`-alias fallback rewrote to canonical local symbol ids (e.g. `src.types.Node.module`) were still being capped to `0.5/Ambiguous` because `ModuleResolver::known_modules` only contained module-level ids. Now iterate `all_raw_edges` after barrel collapse and register each `EdgeKind::Defines` target as a known local module, so the subsequent `is_local` check succeeds and the edge keeps its extractor confidence (`0.7/Inferred` for scoped calls). Self-dogfood: 13 local Calls promoted `Ambiguous → Inferred` across 5 crates (BUG-018).

## [0.11.7] - 2026-04-21

### Added
- feat(config): `[settings].external_stubs` merge layer — the shared list is chained ahead of each `[[project]].external_stubs` at the two `ExternalStubs::new` call sites, concatenation not override. Lets monorepos lift a shared language prelude (e.g. Rust `std` + `Vec`/`String`/`Option`/… + `format`/`println`/… + `assert*`) to the top level instead of duplicating it per project. `ExternalStubs::new` sorts by descending prefix length and dedupes, so overlap between settings and project lists is harmless. Self-dogfood `graphify.toml` shrank 119 → 77 lines (35%). Unit guard asserts chained input matches single-list input (FEAT-034).

## [0.11.6] - 2026-04-21

### Changed
- feat(core): all scoring inputs (betweenness, PageRank, in/out-degree, cycle membership, hotspot classification) now run over a filtered view of the graph that excludes `ConfidenceKind::ExpectedExternal` edges. New helper `CodeGraph::filter_edges<F>` clones nodes and applies the predicate to edges; the original graph stays untouched for `query`/`explain`/report writers. Semantic contract: "hotspot view = filtered view" — every `NodeMetrics` field reflects the scoring graph, so an external stub like `std::path::PathBuf` reports `score=0.000, in_degree=0`, but `graphify explain` still shows its full-graph dependents via `QueryEngine`. Cycle detection uses the filtered graph for consistency (behavioural no-op since `ExpectedExternal` targets are leaves). Self-dogfood: top 10 hotspots of every crate now 100% actionable — zero `std`/`serde`/`tree_sitter`/`rayon`/`petgraph`/`clap`/`tokio` in any top 10 (FEAT-033).

## [0.11.5] - 2026-04-21

### Fixed
- fix(extract): cap the FEAT-031 `use`-alias rewrite recursion depth at 4 iterations. Pathological self-referential aliases of the shape `("X", "X::Y")` (common in Rust `pub use` re-exports — e.g. `use crate::types::Node` inside `crate::types`) would recurse with an ever-growing rewritten string, burning ~17 GB RSS in the first 10 seconds of the graphify-cli dogfood extraction before OS-level OOM kill. Depth cap terminates the rewrite with a non-local result, preserving the legitimate one-hop case (`Node::module` → `crate::types::Node::module` → canonical local id) while preventing the runaway (BUG-017).
- fix(stubs): extend the external-stubs prefix matcher to accept Rust `::` path separators in addition to the `.` dot-notation (`std::path::PathBuf` now matches the `std` prefix) (FEAT-032).

## [0.11.4] - 2026-04-21 — **BROKEN, DO NOT USE**

> **Skip this release.** Extracting any Rust project that uses a re-export pattern of the shape `use crate::types::Node;` in `crate::types` hits an unbounded alias-rewrite recursion and consumes all available memory within seconds. The fix landed in v0.11.5 (BUG-017). If you installed 0.11.4, upgrade immediately.

### Added
- feat(extract): FEAT-031 — Rust scoped-identifier calls (`Node::module()`) and bare-name calls (`validate()` after `use crate::validator::validate;`) now resolve to their canonical local symbol ids via a per-source-module `use`-alias fallback in `ModuleResolver::resolve`. Closes the intra-crate visibility gap that left ~40–60% of Rust call edges invisible post-BUG-016.

## [0.11.3] - 2026-04-20

### Fixed
- fix(extract): Rust `crate::` resolution now re-prepends the project-level `local_prefix` when stripping the `crate::` root. Previously `crate::types::Node` from any module in a Rust crate resolved to `types.Node` instead of `src.types.Node` (auto-detected prefix), missed `known_modules`, and landed on a non-local placeholder — making intra-crate hub structure invisible across every Rust analysis with a non-empty `local_prefix`. Same shape as BUG-001 (Python relative) and BUG-007/011 (TS workspace alias): a language-specific resolver branch forgetting to apply the project-level prefix re-prepend. Discovered via self-dogfood of v0.11.2 on graphify's own workspace (BUG-016).

### Added
- chore(dogfood): ship a self-analysis `graphify.toml` at the repo root with one `[[project]]` per crate. Enables `cargo build && ./target/release/graphify run` as a fast feedback loop for resolver/metric changes.

## [0.11.2] - 2026-04-20

### Added
- feat(settings): `workspace_reexport_graph` opt-out flag (default `true`) gates the FEAT-028 workspace-wide cross-project fan-out. Set to `false` in `[settings]` to pin pre-`v0.11.0` edge counts — useful for reproducing historical reports or bisecting whether an edge is a real dependency vs. a workspace-graph artifact. Rationale in `docs/adr/0001-workspace-reexport-graph-gate.md` (FEAT-030).

### Changed
- chore(repo): untrack `target/` build artifacts so `git status` stays clean after `cargo build` (CHORE-006). Historical tags (`v0.11.0`, `v0.11.1`) remain valid; blob weight reclaimed via future `gc`.

## [0.11.1] - 2026-04-20

### Fixed
- fix(cycles): `[consolidation].suppress_barrel_cycles = true` (opt-in, default `false`) drops cycles whose only cycle-making edges route through the project's root barrel node, but only when that barrel node (`id == local_prefix`) is also matched by the `[consolidation].allowlist`. Counters the synthetic barrel cycles FEAT-028 surfaced on consumers like `parisgroup-ai/cursos` where `src/index.ts` is the npm entry point. Applied at all 4 `run_analyze` call sites (Analyze, Diff baseline-vs-live, Run pipeline helper, Check); query engine skips it since consolidation config isn't in scope for `query`/`explain`/`path`/`shell`. `--ignore-allowlist` debug flag disables barrel suppression too (BUG-015).

## [0.11.0] - 2026-04-20

### Added
- feat(extract): FEAT-028 — workspace-wide `ReExportGraph` fan-out closes the cross-project alias gap left by FEAT-027. New `WorkspaceReExportGraph` aggregate in `crates/graphify-extract/src/workspace_reexport.rs` merges per-project `ReExportGraph`s via a first-wins `modules_to_project` index plus a collision log. `ModuleResolver::apply_ts_alias_workspace` returns a `WorkspaceAliasTarget { project, module_id }` when an alias target path falls inside a sibling `[[project]]` root. `run_extract` split into `build_project_reexport_context` (phase 1, collect-only) + `run_extract_with_workspace` (phase 2, fan-out against a workspace graph). Before v0.11.0, cross-project aliases (`@repo/*` spanning sibling `[[project]]`s) terminated at the raw alias string with `is_local=false` — consumer app → package barrel only, no fan-out into the package's internal structure. Workspace graph only built when ≥2 projects AND ≥1 TS project; single-project and non-TS-only configs keep the legacy fast path with zero overhead. Namespacing decision (option 2, ADR in module doc-comment): public node ids stay per-project; the workspace lookup is internal. Self-dogfood on `parisgroup-ai/cursos @ 8ff36cc1`: +2,475 cross-project edges A→B (pre-0.10.0 vs post-0.11.1 pin), both redistributive (−1,622 edges across top-5 consumer-app pairs that previously terminated at barrels) and additive (+~4,000 pkg-api→`@repo/*` alias edges previously invisible). Benchmark report at `docs/benchmarks/2026-04-20-feat-029-cross-project-edges.md`.
- feat(extract): `match_alias_target` supports inner-glob tsconfig forms (`"@repo/*": ["../../packages/*/src"]`), not just trailing-`*` or exact-match. Final pre-existing blocker called out in FEAT-028 slice 4.

### Deprecated
- The cross-project barrel-termination behaviour (`is_local=false` on `@repo/*` alias targets in multi-project configs) was the intentional tripwire pinned by FEAT-027's `feat_027_cross_project_alias_stays_at_barrel_v1_contract` test. That test is now inverted to `feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope`.

## [0.8.2] - 2026-04-16

### Fixed
- fix(extract): resolve TypeScript path aliases from the nearest `tsconfig.json`, including layouts where the configured repo points at `src/`
- fix(extract): model Go modules as package-scoped nodes so local import resolution and graph metrics no longer fragment across file-scoped IDs

## [0.8.1] - 2026-04-15

### Fixed
- fix(extract): resolve Python `from X import Y as Z` aliases to the canonical imported symbol so alias calls no longer create placeholder graph nodes

### Changed
- chore(release): make workspace crates publishable by inheriting versioned internal dependencies and crate metadata from the workspace

## [0.8.0] - 2026-04-15

### Added
- feat(php): first-class PHP support across extraction, resolution, and CLI analysis, including PSR-4 autoload mapping support and `.php` project discovery (FEAT-019)
- feat(report): surface low-confidence extraction findings directly in `architecture_report.md`
- feat(integrations): add `/gf-setup` and ship the Obsidian documentation vault alongside AI integrations

### Fixed
- fix(extract): restrict `Calls` edges to imported callees to reduce false positives in call graph output
- fix(extract): strip `.ts`/`.js` extensions in `resolve_ts_relative` so relative imports normalize consistently
- fix(report): improve Obsidian wiki-links and self-contained overview rendering
- fix(cli): remove the misleading `local_prefix` existence check that rejected valid layouts

## [0.7.0] - 2026-04-15

### Added
- feat(cli): add `graphify install-integrations` for manifest-driven install/uninstall of Graphify assets into Claude Code and Codex
- feat(integrations): ship bundled Graphify AI integrations: slash commands, skills, agents, and MCP configuration templates for Claude Code and Codex (FEAT-018)
- feat(core): classify top hotspots as `hub`, `bridge`, or `mixed` in report output (FEAT-017)

### Fixed
- fix(install): make uninstall respect `--dry-run`, track inline wrapper files, and warn when project-local install risks untracked artifacts

## [0.6.0] - 2026-04-14

### Added
- feat(cli): `graphify pr-summary` command to render PR-ready Markdown summaries from project analysis artifacts (FEAT-015)
- feat(cli): write `check-report.json` to each project output directory so PR summarization and downstream tooling can read a single project folder directly (FEAT-015)
- feat(report): PR summary sections for hotspots, cycles, community shifts, rules violations, and contract drift (FEAT-015)
- feat(core): deserialize diff and contract check report types for report/CLI composition (FEAT-015)

### Fixed
- fix(install): point `install.sh` to the correct GitHub repository owner (`parisgroup-ai/graphify`) so release downloads resolve correctly

## 0.5.0 — 2026-04-13

- feat(cli): FEAT-016 contract drift detection between Drizzle ORM schemas and TS interface/type declarations, integrated into `graphify check`

## [0.4.1] - 2026-04-13

### Fixed
- fix(core): make community detection deterministic on symmetric graphs so historical trend churn does not fluctuate across identical runs

## [0.4.0] - 2026-04-13

### Added
- feat(cli): `graphify trend` command for historical architecture trend aggregation across stored snapshots (FEAT-014)
- feat(core): historical snapshot and trend aggregation model with hotspot movement and community churn (FEAT-014)
- feat(report): trend report output in JSON and Markdown formats plus automatic per-run snapshot persistence (FEAT-014)
- feat(cli): policy-driven architecture rules for `graphify check` (FEAT-013)

## [0.3.0] - 2026-04-13

### Added
- feat(cli): `graphify diff` command — file-vs-file and baseline-vs-live architectural drift detection (FEAT-002)
- feat(core): `compute_diff` engine — 5-dimension drift detection with community equivalence mapping (FEAT-002)
- feat(report): drift report output in JSON and Markdown formats (FEAT-002)
- feat(cli): `graphify watch` — file watcher with debounced rebuild, per-project affected detection (FEAT-010)
- feat(core): `QueryEngine` with search, explain, path-finding, dependents/dependencies, suggest (FEAT-006)
- feat(cli): `graphify query`, `explain`, `path`, `shell` subcommands for interactive graph exploration (FEAT-006)
- feat(mcp): `graphify-mcp` crate — MCP server exposing 9 graph query tools to AI assistants (FEAT-007)
- feat(extract): `ExtractionCache` — SHA256-based per-file incremental builds with version-aware invalidation (FEAT-005)
- feat(cli): `--force` flag to bypass extraction cache for full rebuilds (FEAT-005)
- feat(core): `ConfidenceKind` enum and confidence scoring on edges (Extracted/Inferred/Ambiguous) (FEAT-008)
- feat(extract): resolver returns confidence per resolution path (direct=1.0, relative=0.9, alias=0.85) (FEAT-008)
- feat(mcp): `min_confidence` parameter on `graphify_search` tool (FEAT-008)
- feat(report): Neo4j Cypher, GraphML, and Obsidian vault export formats (FEAT-009)
- feat: CI quality gates workflow via GitHub Actions (FEAT-004)
- feat(cli): `graphify check` subcommand for CI quality gate enforcement

### Fixed
- fix(core): remove unnecessary `Serialize` bound on `Delta<T>`, fix community tie-breaking (FEAT-002)
- fix(mcp): spec compliance — `local_only`, `max_depth` default, extraction error handling
- fix: align formatting with Rust 1.94 rustfmt

### Changed
- refactor(cli): extract `run_pipeline_for_project` helper for reuse in watch mode (FEAT-010)
- refactor(mcp): code quality fixes from review

## [0.2.0] - 2026-04-12

Initial public release with Python and TypeScript extraction, petgraph-based analysis, and multi-format reporting.
