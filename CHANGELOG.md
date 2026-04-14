# Changelog

All notable changes to Graphify will be documented in this file.

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
