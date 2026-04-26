# FEAT-043 — `graphify suggest stubs` Design

**Status:** Draft
**Date:** 2026-04-26
**Author:** brainstorming session (Cleiton + Claude)
**Depends on:** FEAT-034 (`[settings].external_stubs` merge layer), FEAT-015 (analysis.json edges array)

## Motivation

Polish the ergonomics of `[settings].external_stubs` / `[[project]].external_stubs` introduced by FEAT-032/034. Today users must manually inspect noisy `Ambiguous`-confidence edges or hotspot reports to figure out which prefixes belong in `external_stubs`. This is a repeated friction during:

- **Bootstrap:** right after `graphify init`, the `external_stubs` list is empty (or only the prelude defaults), and the first `graphify run` produces noisy hotspots inflated by external dependencies that look like first-class architectural concerns.
- **Diagnostic:** during ongoing maintenance, new third-party packages enter the codebase and silently inflate hotspot scores until someone notices and updates the config by hand.

`graphify suggest stubs` automates the discovery loop: scan the existing analysis output, group external references by language-aware prefix, classify them as cross-project or per-project, and emit either a copy-paste-ready snippet or an in-place edit of `graphify.toml`.

## Non-goals

- **Not** a real-time hot loop — runs on demand, not in `watch` mode.
- **Not** a graph re-extractor — consumes existing `analysis.json` (Approach A from brainstorming). If analysis is stale, the user re-runs `graphify run` first.
- **Not** a stub *remover* — only suggests additions. Stub removal is out of scope.
- **Not** a generic suggester framework — the `suggest <kind>` namespace is opened deliberately, but only `stubs` ships in this FEAT. Future kinds (`cycles`, `hotspots`) get separate FEATs.

## CLI surface

```
graphify suggest stubs [OPTIONS]

Options:
  --config <PATH>        Path to graphify.toml (default: ./graphify.toml)
  --format <FMT>         Output format: md, toml, json [default: md]
  --apply                Edit graphify.toml in place. Mutually exclusive with --format.
  --min-edges <N>        Minimum edge weight sum to suggest a prefix [default: 2]
  --project <NAME>       Limit to a single project (suggests only for that project)
  -h, --help             Print help
```

`graphify suggest` without a `<kind>` argument prints help listing available kinds.

## Architecture

### Module placement

| Crate | New module | Purpose |
|---|---|---|
| `graphify-report` | `src/suggest.rs` | Pure scoring + rendering. No I/O, no extractor deps. |
| `graphify-cli` | `src/main.rs` (new `cmd_suggest_stubs`) | Orchestration: load config + analysis.json files, dispatch to renderer, optional `--apply`. |

This mirrors the established split for `pr_summary` and `consolidation`: pure renderer in `graphify-report`, thin orchestration in `graphify-cli`.

### Public API surface (graphify-report)

```rust
// crates/graphify-report/src/suggest.rs

pub struct ProjectInput<'a> {
    pub name: &'a str,
    pub local_prefix: &'a str,
    pub current_stubs: &'a ExternalStubs,   // already-merged settings + project
    pub analysis: &'a AnalysisSnapshot,
}

pub struct StubCandidate {
    pub prefix: String,
    pub language: Language,
    pub edge_weight: u64,                   // sum of edge.weight pointing into prefix
    pub node_count: usize,                  // distinct external nodes under prefix
    pub projects: Vec<String>,              // sorted; len() >= 1
    pub example_nodes: Vec<String>,         // first 3 distinct node ids, sorted
}

pub struct SuggestReport {
    pub min_edges: u64,
    pub settings_candidates: Vec<StubCandidate>,         // appearing in >= 2 projects
    pub per_project_candidates: BTreeMap<String, Vec<StubCandidate>>,  // appearing in 1 project
    pub already_covered_prefixes: Vec<String>,           // observed but already in current_stubs
    pub shadowed_prefixes: Vec<String>,                  // skipped: collide with local modules
}

pub fn score_stubs(projects: &[ProjectInput<'_>], min_edges: u64) -> SuggestReport;

pub fn render_markdown(report: &SuggestReport) -> String;
pub fn render_toml(report: &SuggestReport) -> String;
pub fn render_json(report: &SuggestReport) -> serde_json::Value;
```

### Data flow

```
graphify.toml
     ↓ load_config (existing)
N projects with output_dir
     ↓
For each project:
  - Read <output_dir>/<project>/analysis.json into AnalysisSnapshot
  - Build current ExternalStubs (settings.external_stubs ++ project.external_stubs,
    sorted by descending prefix length)
     ↓
Filter candidate nodes per project:
  - keep nodes with is_local = false
  - skip if matched by current ExternalStubs (already covered → recorded in already_covered_prefixes)
  - skip if the candidate prefix collides with any first-party scope (recorded in
    shadowed_prefixes; never suggested even if signal is strong). A prefix is
    considered shadowing if it equals (a) the local_prefix of any project in the
    config, or (b) the top-segment of any node id with is_local = true across
    all projects. (a) protects against cross-project FEAT-028 fan-out
    misclassification; (b) protects against synthetic nodes that share a
    namespace root with first-party modules.
     ↓
Group by language-aware prefix (see Prefix Extraction below)
     ↓
Aggregate per (project, prefix):
  - edge_weight: sum of weights of edges where target is_local=false AND target node belongs to prefix bucket
  - node_count: distinct node ids in bucket
  - example_nodes: first 3 distinct node ids by lexicographic order
     ↓
Per-project filter: drop entries with edge_weight < min_edges
     ↓
Auto-classify across projects:
  - prefix in >= 2 projects (post-filter) → settings_candidates (sum weights, union projects, merge examples)
  - prefix in 1 project → per_project_candidates[<project>]
     ↓
Sort each bucket by edge_weight descending, then prefix ascending (stable tie-break)
     ↓
Render md / toml / json, OR mutate graphify.toml (--apply)
```

**Threshold semantics:** `--min-edges` applies **per-project** before aggregation. A prefix with weight 1 in five projects (total 5) does **not** promote to `settings_candidates` — it has weight 1 < threshold in each project individually and is dropped. This prevents long-tail noise from being promoted.

**Edge weight, not node count, is the ranking signal.** A single external node referenced 100× is more relevant than 50 nodes referenced once each. Implementation: iterate `analysis.edges`, accumulate `edge.weight` per `(project, prefix-of-target)` pair, where target's `is_local = false` and target's prefix passes filters.

## Prefix extraction (language-aware)

| Language | Rule | `node_id` → prefix |
|---|---|---|
| `Rust` | first segment before `::` | `tokio::sync::mpsc::Sender` → `tokio` |
| `Python` | first segment before `.` | `numpy.linalg.norm` → `numpy` |
| `TypeScript` (scoped, starts with `@`) | two segments split by `/` | `@anthropic-ai/sdk/messages` → `@anthropic-ai/sdk` |
| `TypeScript` (unscoped) | first segment before `/` | `react/jsx-runtime` → `react` |
| `Php` | first segment before `.` (PSR-4 already normalized `\` → `.`) | `Symfony.Component.HttpFoundation.Request` → `Symfony` |
| `Go` (path-style — segment 0 contains `.`) | first 3 segments joined by `/` | `github.com/spf13/cobra/cmd` → `github.com/spf13/cobra` |
| `Go` (non-path) | first segment before `.` | `fmt.Println` → `fmt` |

Implementation: pure function `extract_prefix(node_id: &str, lang: Language) -> Option<String>`. Returns `None` only if `node_id` is empty after trim — otherwise always emits at least the input itself (cases like `Some` in Rust just return `Some` as prefix).

The language hint comes from the `language` field on each `Node` in `AnalysisSnapshot` — no heuristic guessing.

## Output formats

### Markdown (default)

```markdown
# Stub Suggestions

5 projects analyzed · 8 candidates above threshold (--min-edges=2)

## Promote to [settings].external_stubs (cross-project)

| Prefix       | Edges | Projects                                    | Example                    |
|--------------|-------|---------------------------------------------|----------------------------|
| `tokio`      |   142 | 4 (graphify-cli, -extract, -mcp, -report)   | tokio::sync::mpsc::Sender  |
| `serde`      |    89 | 5 (all)                                     | serde::Deserialize         |

## Per-project candidates

### graphify-mcp
| Prefix | Edges | Example             |
|--------|-------|---------------------|
| `rmcp` |    23 | rmcp::ServerHandler |

## Already covered (skipped)

3 prefixes already in current external_stubs: `std`, `format`, `writeln`

## Skipped — shadowing local modules

1 prefix matching local_prefix or known module: `src`
```

### TOML

```toml
# Generated by `graphify suggest stubs` 2026-04-26T20:00:00Z
# Min edges per project: 2

# Append to [settings] block:
# [settings]
# external_stubs = ["tokio", "serde"]

# Append to per-project blocks:
# [[project]]
# name = "graphify-mcp"
# external_stubs = ["rmcp"]
```

Snippet is **commented-out** by default — user must uncomment to apply manually. This avoids accidental "paste & break" if the user pipes output through tooling. The `--apply` path bypasses this entirely.

### JSON

Schema follows `SuggestReport` struct verbatim (serde_json default derive). Stable for tooling/CI consumption.

```json
{
  "min_edges": 2,
  "settings_candidates": [
    {
      "prefix": "tokio",
      "language": "Rust",
      "edge_weight": 142,
      "node_count": 18,
      "projects": ["graphify-cli", "graphify-extract", "graphify-mcp", "graphify-report"],
      "example_nodes": ["tokio::runtime::Handle", "tokio::spawn", "tokio::sync::mpsc::Sender"]
    }
  ],
  "per_project_candidates": {
    "graphify-mcp": [...]
  },
  "already_covered_prefixes": ["std", "format", "writeln"],
  "shadowed_prefixes": ["src"]
}
```

## `--apply` mode

**New workspace dependency:** `toml_edit = "0.22"` (preserves comments + ordering during round-trip; ~80 KB build cost; dual-licensed MIT/Apache-2.0).

### Algorithm

1. Parse `graphify.toml` with `toml_edit::DocumentMut::from_str`
2. For each `settings_candidate`:
   - Locate or create `[settings]` table
   - Locate or create `external_stubs` array within it
   - Append candidate prefix if not already present (string equality)
3. For each `(project_name, candidates)` in `per_project_candidates`:
   - Iterate `[[project]]` array-of-tables
   - Match by `name` field
   - Locate or create `external_stubs` array within matched block
   - Append candidate prefixes if not already present
4. If no project matches a `project_name`, abort with error (config drift; suggester saw a project name not in current toml).
5. Atomic write: serialize `DocumentMut` to a tempfile in the same directory, then `fs::rename` to target path.

### Output of `--apply`

Stdout summary of mutations:

```
Applied stub suggestions to graphify.toml:
  + [settings]               2 prefixes: tokio, serde
  + [[project]] graphify-mcp  1 prefix:  rmcp

  Total: 3 prefixes added across 2 blocks.
```

### Mutex with `--format`

`clap` argument group: `--apply` and `--format` are mutually exclusive. `--apply` always writes TOML; specifying `--format toml --apply` is rejected by clap with a clear error.

### Error handling

| Failure | Behavior |
|---|---|
| `graphify.toml` parse error | exit 1, surface line/column from `toml_edit` |
| Project name in suggestions not in toml | exit 1, message: `project "<name>" not found in graphify.toml — config may have drifted since analysis.json was generated` |
| Tempfile write fails (disk full, permission) | exit 1, original `graphify.toml` untouched |
| `fs::rename` fails (cross-FS, permission) | exit 1, log tempfile path so user can inspect / recover |

### Idempotence

Running `--apply` twice is a no-op on the second run: dedup-on-append ensures no duplicate entries. Verified by integration test.

## Edge cases

| Case | Behavior |
|---|---|
| `analysis.json` missing for a project | Warn on stderr (`Project X has no analysis.json — run `graphify run` first`), skip project, continue |
| All projects missing `analysis.json` | exit 1, message: `no analysis.json found for any project; run `graphify run` first` |
| `analysis.json` exists but has empty `edges` array (e.g., from `diff --live` mode) | Warn on stderr, skip project (no edges to score) |
| Single-project config | `settings_candidates` is always empty (auto-classify needs ≥2 projects); all suggestions land in `per_project_candidates` |
| All candidates filtered out by threshold + shadowing | Render still emits a valid report with empty buckets and a one-line "no suggestions" note |
| `--project <NAME>` with name not in config | exit 1, message: `project "<name>" not found in graphify.toml` |
| `--min-edges 0` | Allowed; effectively disables the threshold (every observed external prefix is suggested) |
| Node id with no separator (`Some`, `console`) | Prefix is the whole id; surfaces in the report. User can filter via `--min-edges`. |
| TS scoped node id with only one segment (`@anthropic-ai`, no `/`) | Returns `@anthropic-ai` as prefix (degenerate but harmless) |
| `--apply` on read-only filesystem | Tempfile creation fails first → exit 1 with permission error before any mutation attempt |
| Config has `[[project]]` blocks without `output_dir` (relying on default) | Use the same default path resolution as `cmd_run` (`<settings.output>/<project.name>/analysis.json`) |

## Testing strategy

### Unit tests — `graphify-report::suggest`

| Test | Covers |
|---|---|
| `extract_prefix_per_language` | Table from "Prefix extraction" — one case per language + degenerate cases (no separator, single-segment scoped TS, Go path vs Go simple) |
| `score_stubs_groups_by_prefix` | 2 projects, prefix `tokio` in both → `settings_candidates` |
| `score_stubs_threshold_per_project` | Threshold=5; weight 4 in project A AND weight 4 in project B → both dropped → settings empty |
| `score_stubs_skip_already_covered` | `current_stubs` contains `tokio` → `tokio` appears in `already_covered_prefixes`, not in `settings_candidates` |
| `score_stubs_skip_shadowing` | Local prefix `src` matches external node id `src::utils` from another resolver path → recorded in `shadowed_prefixes`, not suggested |
| `score_stubs_ranks_by_edge_weight` | Two prefixes; A has 50 edges across 1 node, B has 5 edges across 5 nodes → A ranked first |
| `score_stubs_stable_tiebreak` | Two prefixes with equal weight → sorted by prefix ascending |
| `render_markdown_includes_all_sections` | All 5 sections present (header, settings, per-project, already-covered, shadowed) |
| `render_markdown_handles_empty_buckets` | Empty inputs → "no suggestions" message, valid markdown |
| `render_toml_emits_commented_snippet` | Snippet round-trips through toml parser; lines start with `#` (commented) |
| `render_json_schema_stable` | JSON output deserializes back into `SuggestReport` |

### Integration test — `graphify-cli`

Reuses pattern from existing `pr-summary` integration test:

- Fixture: `crates/graphify-cli/tests/fixtures/suggest/` with a tiny `graphify.toml` referencing 2 mock project directories, each containing a hand-authored `analysis.json` with known external prefixes
- Assert: `graphify suggest stubs --config <fixture>/graphify.toml` exits 0, stdout contains expected prefix names, exits 0 even when `--min-edges 100` (just emits empty report)
- Assert: `graphify suggest stubs --apply --config <fixture-copy>/graphify.toml` mutates the toml file, golden-compare result
- Assert: second `--apply` invocation is a no-op (idempotent)
- Assert: `--format toml --apply` is rejected by clap before any I/O

### Dogfood acceptance test

After implementation lands:

```bash
graphify suggest stubs --config graphify.toml
```

Expected on this repo: `already_covered_prefixes` includes the Rust prelude entries currently in `[settings].external_stubs` (std, format, writeln, assert*, Vec, etc.). New candidates should be empty or near-empty (the codebase is mature and stubs are well-curated). If unexpected candidates surface, that's signal — file follow-up tasks.

### Non-tests (intentional)

- No "fresh extract" path (Approach A consumes existing analysis.json — that path simply doesn't exist).
- No regression test on absolute scoring numerics — counts are deterministic by construction (edge weights summed, no sampling).
- No multi-language fixture in unit tests — language behavior is covered by `extract_prefix_per_language` table; integration test uses a single-language fixture for simplicity.

## Open questions

None as of close of brainstorming. Q1–Q4 + the data-source choice are all resolved.

## Effort estimate

~2-3 hours total:

- ~250 LoC across `graphify-report::suggest` + CLI orchestration + tests
- 1 new workspace dep (`toml_edit`)
- 1 fixture directory (`tests/fixtures/suggest/`)
- README section + `graphify init` template comment update advertising the suggester
- CHANGELOG entry

## Implementation order (proposed for writing-plans)

1. Add `toml_edit` workspace dep, sanity-build
2. `graphify-report::suggest` — types + `score_stubs` + per-language `extract_prefix` + unit tests
3. `graphify-report::suggest` — markdown / toml / json renderers + unit tests
4. `graphify-cli::cmd_suggest_stubs` — orchestration (parse args, load configs, load analysis.json files, dispatch render)
5. `graphify-cli::cmd_suggest_stubs` — `--apply` path with `toml_edit` + atomic rename
6. Integration test fixture + tests
7. Dogfood run + capture output, decide if any unexpected candidates need follow-up tasks
8. README + `graphify init` template + CHANGELOG
