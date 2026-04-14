# FEAT-015 — PR Summary CLI (`graphify pr-summary`)

**Status:** Design approved (2026-04-14)
**Task:** [[FEAT-015-pr-and-editor-integration]]
**Related:** FEAT-002 (drift detection), FEAT-004 (CI quality gates), FEAT-013 (policy rules), FEAT-016 (contract drift), FEAT-007 (MCP server)

---

## 1. Scope

Add a new CLI subcommand `graphify pr-summary <PROJECT_OUTPUT_DIR>` that renders a concise Markdown summary of architectural change for a single project, suitable for:

1. **Pre-push self-review** — an LLM (or developer) running the command locally before asking for human approval
2. **Human handoff** — the solo developer skimming what an LLM-produced change did to the architecture before merging
3. **CI integration** — any CI platform appending the output to its job summary (GitHub `$GITHUB_STEP_SUMMARY`, GitLab, Buildkite, …)

The command is a **pure renderer** over existing JSON artifacts. It does not re-extract, re-analyze, or re-compute drift.

### In scope (v1)

- New subcommand `graphify pr-summary <DIR>`
- Input: a single project's Graphify output directory
- Reads `analysis.json` (required) plus `drift-report.json` and `check-report.json` (each optional)
- Output: Markdown to stdout; warnings to stderr
- Exit 0 on successful render (regardless of findings); exit 1 on usage or required-input errors (chosen to match existing `cmd_diff`/`cmd_trend` convention throughout the CLI; deviates from the Unix "exit 2 for usage" norm but keeps graphify's CLI exit codes uniform)
- Graceful degradation when optional inputs are missing or malformed
- Fixed section order, fixed row caps (no configurability in v1)
- Inline next-step CLI commands next to each finding (investigation shortcuts)
- README recipe for GitHub Actions `$GITHUB_STEP_SUMMARY` integration

#### Included ecosystem changes (required to make v1 possible)

- **Move `CheckReport`, `ProjectCheckResult`, `CheckViolation`, and related types** from the private scope in `crates/graphify-cli/src/main.rs` to a new public module `crates/graphify-report/src/check_report.rs`. These types must be `pub` + `Serialize`/`Deserialize` so the renderer can consume them.
- **Make `graphify check` always write `<project_out>/check-report.json`** alongside existing artifacts (unified shape: project check results plus contract results, mirroring the current in-memory `CheckReport` struct). Non-breaking; additive to current behavior.

These are part of FEAT-015 scope because they are prerequisites for a clean `pr-summary` implementation. The type-move is required for external deserialization regardless of path; the write-to-disk step aligns the ecosystem (every signal-producing command writes its JSON to the conventional output dir).

### Out of scope (v1) — rejected alternatives

| Rejected | Reason |
|---|---|
| Companion GitHub Action (separate repo) | Adds a second release surface; five lines of bash in a workflow achieve the same |
| SARIF output | Security-flavored by convention; architectural findings fit awkwardly |
| PR comment bot that auto-posts to GitHub | Requires a token model and GitHub App identity; `gh pr comment --body-file` suffices |
| Extending MCP (FEAT-007) with `summarize_for_pr` | MCP is editor-facing; PR summary is CI-facing |
| `[pr_summary]` TOML config for section visibility | Defaults should work for typical users; add config when asked |
| HTML output | Markdown covers both reader surfaces; no second render path needed |
| Multi-project aggregated summary | One directory = one project; monorepo users loop in their CI |
| Re-extraction / orchestration inside `pr-summary` | `graphify run` and `graphify diff` already orchestrate; this command is pure rendering |
| v0.6.0 release tag | Ships in a follow-up session after FEAT-015 lands |

---

## 2. User-facing surface

### Synopsis

```bash
graphify pr-summary <PROJECT_OUTPUT_DIR>
```

### Arguments

- `<PROJECT_OUTPUT_DIR>` (positional, required) — path to a single project's Graphify output directory (for example `./report/my-app`). Must contain at least `analysis.json`.

### Flags

None in v1. (Determinism: same inputs → same output.)

### Typical workflows

**Local pre-push self-check (solo developer or LLM):**

```bash
# 1. Generate the primary report artifacts (analysis.json, graph.json, markdown report, ...)
graphify run --config graphify.toml

# 2. Drift vs baseline (writes drift-report.json + drift-report.md)
graphify diff --baseline ./report-main/my-app/analysis.json \
              --config graphify.toml --project my-app

# 3. Rules + contract check (writes check-report.json via the FEAT-015 ecosystem change; also exits non-zero on violations)
graphify check --config graphify.toml || true

# 4. Render the PR summary
graphify pr-summary ./report/my-app
```

Steps 2 and 3 are optional. Without step 2 the summary skips the "Drift in this PR" section with a hint. Without step 3 the summary skips the "Outstanding issues" section entirely.

**GitHub Actions — append to the job summary:**

```yaml
- run: graphify run --config graphify.toml
- run: graphify diff --baseline ./baseline/analysis.json --config graphify.toml --project my-app
- run: graphify check --config graphify.toml || true   # exit code ignored; gate separately if desired
- run: graphify pr-summary ./report/my-app >> "$GITHUB_STEP_SUMMARY"
```

**Produce a Markdown file to post as a PR comment:**

```bash
graphify pr-summary ./report/my-app > pr-summary.md
gh pr comment --body-file pr-summary.md
```

---

## 3. Architecture

```
┌──────────────────────────────────────────────────┐
│ graphify-cli/src/main.rs                         │
│   Commands::PrSummary { dir }                    │
│   ↓ validate dir, detect multi-project layout    │
│   ↓ load analysis.json  (required; exit 2 on err)│
│   ↓ load drift-report.json  (optional; warn+skip)│
│   ↓ load check-report.json  (optional; warn+skip)│
│   ↓ graphify_report::pr_summary::render(...)     │
│   ↓ println!(string)                             │
└──────────────────────────────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────────┐
│ graphify-report/src/pr_summary.rs                │
│   pub fn render(                                 │
│     project_name: &str,                          │
│     analysis: &AnalysisSnapshot,                 │
│     drift: Option<&DiffReport>,                  │
│     check: Option<&CheckReport>,                 │
│   ) -> String                                    │
│                                                  │
│   Pure function. No I/O. Produces Markdown.      │
│   Graceful per-section degradation.              │
│   Contract-drift rows come from check.contracts. │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│ graphify-report/src/check_report.rs (NEW)        │
│   pub struct CheckReport { ok, violations,       │
│                            projects, contracts } │
│   pub struct ProjectCheckResult { ... }          │
│   pub enum CheckViolation { Limit, Policy }      │
│   pub struct LimitSummary { ... }                │
│   #[derive(Serialize, Deserialize)]              │
│                                                  │
│   Types moved from graphify-cli/src/main.rs.     │
│   Both the CLI (producer) and pr_summary         │
│   (consumer) depend on this module.              │
└──────────────────────────────────────────────────┘
```

### Module placement

The renderer lives in `graphify-report` alongside `diff_markdown.rs`, `contract_markdown.rs`, and `trend_markdown.rs`, matching the per-signal module convention. The CLI glue (argument parsing, file I/O, error handling) lives in `graphify-cli/src/main.rs` alongside other subcommand handlers.

### Dependencies

No new external crates. Uses existing `serde`, `serde_json`, `anyhow`, and `clap` already in the workspace.

---

## 4. Input contract

### Required

**`<DIR>/analysis.json`** — produced by `graphify run` or `graphify analyze`. Already serialized via existing `AnalysisSnapshot` type (see `graphify-core/src/diff.rs`). Used for:
- Project name (from the `project` field; falls back to `<DIR>` basename if absent)
- Current node count, edge count, language count (for the summary stats line)

### Optional

| File | Source | Renders |
|---|---|---|
| `<DIR>/drift-report.json` | `graphify diff --baseline ... --config ...` | "Drift in this PR" section — new/broken cycles, escalated/new hotspots, community shifts |
| `<DIR>/check-report.json` | `graphify check --config ...` (written unconditionally under FEAT-015) | "Outstanding issues" section — **both** rules violations (from `projects[*].violations`) **and** contract drift (from `contracts`) live in this single unified file |

`drift-report.json` is an existing artifact from FEAT-002. `check-report.json` is a **new on-disk artifact** introduced by FEAT-015 (see "Included ecosystem changes" in Section 1). Its shape mirrors the current in-memory `CheckReport` struct printed to stdout today by `graphify check --json`, so the schema is already stable — this change only moves the artifact from stdout to disk and makes its types public.

### Multi-project root detection

If `<DIR>` contains no `analysis.json` of its own but contains subdirectories that each have their own `analysis.json`, the command errors with a usage hint pointing at a subdirectory. This catches the common mistake of passing `./report/` instead of `./report/<project>/`.

---

## 5. Output contract — Markdown layout

### Exact shape

```markdown
### Graphify — Architecture Delta for `<project>`

<old_nodes> → <new_nodes> nodes (<±diff>) · <old_edges> → <new_edges> edges (<±diff>)

#### Drift in this PR

- **New cycle** — `<a> ↔ <b>`
  `→ graphify path <a> <b>`
- **Broken cycle** — `<a> ↔ <b>`
- **Escalated hotspots (<N>)**
  - `<node>` (<old_score> → <new_score>)  `→ graphify explain <node>`
  …up to 5 rows; then `_…and <M> more (see drift-report.md)_`
- **New hotspots (<N>)**
  - `<node>` (score <new_score>)  `→ graphify explain <node>`
  …same 5-row cap
- **Community shift** — `<module_prefix>` split across <N> communities (was <M>)

#### Outstanding issues
<!-- Rendered only when check-report.json is present AND has at least one project violation or contract violation. Subheadings are omitted individually when their respective list is empty. Both subheadings are populated from the same unified check-report.json file. -->

**Rules violations (<N>)** — `graphify check --config graphify.toml`
- `<rule_id>` — `<source> → <target>`
  …5-row cap

**Contract drift (<N>)** — `graphify check --config graphify.toml`
- `<orm_table>` (<source>) ↔ `<ts_type>` (TS): <violation_summary>
  …5-row cap

<sub>Graphify v<version> · `graphify pr-summary <dir>` to regenerate</sub>
```

### Rendering rules

1. **Header and stats line are always present** (given `analysis.json` is valid).
2. **Zero-finding subsections are omitted entirely.** If no new cycles exist, the "New cycle" lines do not appear. No `(0)` rows.
3. **Whole sections are omitted** when no rows remain: if drift-report has no changes anywhere, the "Drift in this PR" header is replaced by `_No architectural changes vs baseline._`; if the drift-report file is missing entirely, the hint line `_No drift baseline — run `graphify diff --baseline <path> --config graphify.toml` to populate._` appears instead.
4. **Per-list hard cap at 5 rows.** Beyond 5, a `_…and <M> more_` line points at the full `drift-report.md` or `check-report.json` for the complete list.
5. **Next-step commands are inline**, in backticks, on the same or immediately following line. Identifiers stay in backticks everywhere to aid both human scan and LLM pattern-match.
6. **Emoji count is zero in v1.** Section scan relies on headings and bold prefixes, not decoration.
7. **Footer is always present** and names the Graphify version (from `env!("CARGO_PKG_VERSION")`) plus the regeneration command.

### Project name resolution

1. `analysis.json["project"]` field, if present and a non-empty string
2. Otherwise, the basename of `<DIR>`

This matches how `graphify run` writes project output today.

---

## 6. Graceful degradation — required vs optional behavior

| Input state | Behavior | Exit |
|---|---|---|
| `<DIR>` does not exist | stderr: `graphify pr-summary: directory '<dir>' not found` | 1 |
| `<DIR>/analysis.json` missing | stderr: `graphify pr-summary: missing analysis.json in '<dir>' (run 'graphify run' first)` | 1 |
| `<DIR>/analysis.json` malformed | stderr: `graphify pr-summary: failed to parse analysis.json: <reason>` | 1 |
| `<DIR>` is a multi-project root | stderr: `graphify pr-summary: '<dir>' is a multi-project output root — point at a single project subdirectory` | 1 |
| `drift-report.json` missing | Render header + stats line + hint line to run `graphify diff` | 0 |
| `drift-report.json` present, all deltas empty | Render header + stats line + `_No architectural changes vs baseline._` | 0 |
| `drift-report.json` malformed | stderr warning `(warning: failed to parse drift-report.json, skipping section)`; render as if missing | 0 |
| `check-report.json` missing | Silently omit entire "Outstanding issues" section | 0 |
| `check-report.json` present, no project violations AND no contract violations | Silently omit entire "Outstanding issues" section | 0 |
| `check-report.json` present, project violations only (no contract) | Render "Rules violations" subsection; omit "Contract drift" subsection | 0 |
| `check-report.json` present, contract violations only (no rules) | Render "Contract drift" subsection; omit "Rules violations" subsection | 0 |
| `check-report.json` malformed | stderr warning `(warning: failed to parse check-report.json, skipping section)`; render as if missing | 0 |

### Principles

1. Only `analysis.json` is required; everything else is optional.
2. A malformed optional file warns to stderr and is treated as missing. A broken sub-report must not prevent the rest of the summary from rendering.
3. Expected-but-missing artifacts produce hints, not errors. A fresh project has no drift baseline on its first run; the summary should guide the user.
4. Gating (exit non-zero on findings) is not this command's job. `graphify check` already gates. `pr-summary` reports.
5. Summary goes to stdout; warnings go to stderr. Clean pipe semantics for `>> "$GITHUB_STEP_SUMMARY"`.

---

## 7. Testing strategy

Three layers, matching the per-signal module convention.

### Layer 1 — renderer unit tests in `graphify-report/src/pr_summary.rs`

Golden-file style with inline `const` JSON fixtures and substring/section assertions on the rendered Markdown. Pure function; no I/O; fast.

Representative coverage:

- `renders_header_and_stats_from_analysis_only`
- `renders_drift_section_with_new_cycle`
- `renders_drift_section_with_escalated_hotspots`
- `omits_drift_section_when_drift_report_missing`
- `renders_hint_when_drift_report_missing`
- `renders_no_changes_line_when_drift_is_empty`
- `caps_list_at_5_rows_with_more_hint`
- `renders_rules_violations_subsection_from_check_report_projects`
- `renders_contract_drift_subsection_from_check_report_contracts`
- `renders_both_subsections_when_check_report_has_project_and_contract_violations`
- `omits_outstanding_section_when_check_report_missing`
- `omits_outstanding_section_when_no_violations`
- `omits_only_contract_subsection_when_no_contract_violations`
- `omits_only_rules_subsection_when_no_project_violations`
- `renders_next_step_commands_inline`
- `uses_project_name_from_analysis_json`
- `falls_back_to_dir_basename_when_project_field_absent`

Approximately 16 unit tests.

### Layer 2 — CLI glue tests in `graphify-cli`

`tempfile::TempDir` based. Covers the I/O surface the renderer deliberately does not know about.

- `errors_when_directory_missing`
- `errors_when_analysis_json_missing`
- `errors_when_analysis_json_malformed`
- `errors_on_multi_project_root_layout`
- `warns_and_continues_on_malformed_drift_report`
- `warns_and_continues_on_malformed_check_report`
- `succeeds_with_only_analysis_json_present`

Approximately 7 tests.

### Layer 3 — end-to-end integration

One happy-path and one error-path test in `crates/graphify-cli/tests/`, reusing the OnceLock binary-build harness from FEAT-013.

- `pr_summary_end_to_end_against_sample_fixture`
- `pr_summary_exits_2_on_missing_dir`

2 integration tests.

### Fixture strategy

- Inline `const` JSON strings for unit tests (layer 1); isolates the renderer from the filesystem.
- A realistic multi-file fixture directory (`crates/graphify-cli/tests/fixtures/pr_summary/`) with the three JSON artifacts (`analysis.json`, `drift-report.json`, `check-report.json`) for the integration test. Small (~1-2 KB each).
- Builder helpers to construct `DiffReport` and `CheckReport` (which embeds `ContractCheckResult`) programmatically (mirrors `ContractBuilder` from FEAT-016).

### Explicitly not tested

- Full-Markdown byte-for-byte snapshots (too brittle; substring and presence asserts suffice)
- Drift / check / contract generation logic (covered by FEAT-002, FEAT-004, FEAT-013, FEAT-016)
- GitHub Actions `$GITHUB_STEP_SUMMARY` integration (user-wired)

---

## 8. File touches

### New files

| Path | Purpose |
|---|---|
| `crates/graphify-report/src/check_report.rs` | Public `CheckReport` / `ProjectCheckResult` / `CheckViolation` / `LimitSummary` / `PolicyCheckSummary` types with `Serialize` + `Deserialize` (moved from `graphify-cli/src/main.rs`); roundtrip unit tests |
| `crates/graphify-report/src/pr_summary.rs` | Pure renderer plus unit tests |
| `crates/graphify-cli/tests/pr_summary_integration.rs` | End-to-end integration tests (may fold into an existing integration file at implementation time) |
| `crates/graphify-cli/tests/fixtures/pr_summary/analysis.json` | Fixture |
| `crates/graphify-cli/tests/fixtures/pr_summary/drift-report.json` | Fixture |
| `crates/graphify-cli/tests/fixtures/pr_summary/check-report.json` | Unified fixture covering both project rule violations and contract drift |

### Modified files

| Path | Change |
|---|---|
| `crates/graphify-report/src/lib.rs` | `pub mod check_report;` and `pub mod pr_summary;` |
| `crates/graphify-cli/src/main.rs` | (1) Remove private `CheckReport` / `ProjectCheckResult` / `CheckViolation` / `LimitSummary` / `PolicyCheckSummary` types; re-import from `graphify_report::check_report`. (2) Make `graphify check` always write `<project_out>/check-report.json` alongside other artifacts (unconditional; additive; the existing stdout output under `--json` remains). (3) New `Commands::PrSummary { dir: PathBuf }` variant plus dispatch arm with file-loading glue. |
| `README.md` | New "PR Summary" subsection under CI integration, with a GitHub Actions recipe; note that `graphify check` now also writes `check-report.json` to the project output dir |
| `docs/TaskNotes/Tasks/FEAT-015-pr-and-editor-integration.md` | At ship time: status to `done`, subtasks checked, Verification appended |
| `docs/TaskNotes/Tasks/sprint.md` | At ship time: FEAT-015 row to `**done**`, Done section entry |

### LOC estimate

- `check_report.rs` (new module; type-move): ~200 LOC (types + serde derives + ~6 roundtrip unit tests)
- `pr_summary.rs`: ~250 LOC rendering + ~320 LOC unit tests
- CLI changes in `main.rs`: ~30 LOC for `graphify check` write-to-disk + ~60 LOC for new `pr-summary` dispatch
- Integration test: ~60 LOC
- Fixture JSONs: ~150 lines total
- README recipe and check-report note: ~40 lines

Approximately 1100 LOC total. Still fits in a single implementation session.

---

## 9. Future extensions (non-binding)

These are acknowledged as possibilities if real usage demands them, but are **not commitments** and are not part of FEAT-015 v1.

- **Companion GitHub Action** wrapping `pr-summary` for fleet-wide adoption (`parisgroup-ai/graphify-action`).
- **PR comment auto-post** via `gh pr comment --body-file` as a Graphify subcommand rather than user-wired bash.
- **MCP tool** `summarize_for_pr` on top of the same renderer, for assistant-driven review workflows.
- **SARIF output path** if architectural findings gain a security-adjacent use case.
- **Configurable section visibility** (`[pr_summary]` TOML block) if users request it.
- **Multi-project aggregation** (`graphify pr-summary --config graphify.toml`) for monorepo CI workflows.
- **Baseline-aware outstanding issues** — compare the current `check-report.json` against a baseline copy to distinguish "new in this PR" from "pre-existing". Adds a second input file and new compare logic; worth a dedicated feature.

---

## 10. Done criteria

FEAT-015 v1 ships when:

1. `graphify pr-summary <DIR>` is a working subcommand on `main`.
2. `graphify check` writes `<project_out>/check-report.json` unconditionally, alongside existing artifacts.
3. `CheckReport` and related types are `pub` in `graphify-report::check_report` and consumed by both the CLI producer and the `pr_summary` renderer.
4. All three testing layers pass (`cargo test --workspace` is green), including roundtrip tests for the moved `CheckReport` types.
5. Clippy is clean on touched crates (`cargo clippy --workspace -- -D warnings`).
6. The README has a documented GitHub Actions recipe and notes the new `check-report.json` artifact.
7. Graceful-degradation behavior matches Section 6's table.
8. `docs/TaskNotes/Tasks/FEAT-015-pr-and-editor-integration.md` has status `done`, subtasks checked, and a Verification section.
9. `docs/TaskNotes/Tasks/sprint.md` has FEAT-015 in the Done section.

A separate follow-up session handles the v0.6.0 bump and release tag once FEAT-015 is feature-complete on `main`.
