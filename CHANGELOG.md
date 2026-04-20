# Changelog

All notable changes to Graphify will be documented in this file.

## [Unreleased]

## [0.11.2] - 2026-04-20

### Added
- feat(settings): `workspace_reexport_graph` opt-out flag (default `true`) gates the FEAT-028 workspace-wide cross-project fan-out. Set to `false` in `[settings]` to pin pre-`v0.11.0` edge counts — useful for reproducing historical reports or bisecting whether an edge is a real dependency vs. a workspace-graph artifact. Rationale in `docs/adr/0001-workspace-reexport-graph-gate.md` (FEAT-030).

### Changed
- chore(repo): untrack `target/` build artifacts so `git status` stays clean after `cargo build` (CHORE-006). Historical tags (`v0.11.0`, `v0.11.1`) remain valid; blob weight reclaimed via future `gc`.

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
