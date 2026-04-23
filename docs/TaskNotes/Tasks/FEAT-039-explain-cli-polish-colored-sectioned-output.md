---
uid: feat-039
status: done
priority: normal
scheduled: 2026-04-23
completed: 2026-04-23
timeEstimate: 60
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  hintsInferred: true
---

# FEAT: explain CLI polish — colored + sectioned output

Polish `graphify explain` CLI output with ANSI colors, edge-kind-grouped sections, confidence annotations, and a dependency cap. Ships as v0.12.1. CLI-only scope — no TUI, no MCP prompt surface, no shell REPL changes. `anstyle` is already transitively in-tree via clap, so no new dependencies.

## Motivation

Today's `graphify explain <node>` output is a flat wall of `→ target_id` lines. For hubs like `src.server` (47 dependencies), it's a 60-line unbroken list with no signal about edge kind (`Imports` vs `Calls` vs `Defines`), no confidence annotation, and no visual hierarchy. Reviewers have to grep the output mentally to find the one `Calls` edge they care about among 30 `Imports`.

## Design

### Data enrichment (graphify-core)

`ExplainReport.direct_dependencies: Vec<String>` and `direct_dependents: Vec<String>` replaced with `Vec<ExplainEdge>`:

```rust
pub struct ExplainEdge {
    pub target: String,
    pub edge_kind: EdgeKind,
    pub confidence: f64,
    pub confidence_kind: ConfidenceKind,
}
```

Source data is already available — `QueryEngine::dependents()` / `dependencies()` already return `(String, EdgeKind, f64, ConfidenceKind)` tuples; `explain()` was dropping three of four fields at construction. Preserve existing sort order (by edge weight desc, via the underlying `dependents`/`dependencies` calls).

### Printer polish (graphify-cli)

- **Sections**: group `direct_dependencies` / `direct_dependents` by `EdgeKind`; render as subsections with headers like `── Imports (N) ──`, `── Calls (N) ──`, `── Defines (N) ──`. Skip sections with zero entries.
- **Cap + footer**: top 10 per section by weight; `... and N more` footer when exceeded. Applies to both dependencies (currently uncapped) and dependents (currently capped at 5 → bump to 10 for parity).
- **ANSI colors** via `anstyle` (already in-tree from clap):
  - `in_cycle: yes (with: …)` → red bold; `no` → dim
  - Hotspot score → `>= 0.4` red, `>= 0.1` yellow, else default
  - Confidence inline tag: `[extracted]` green, `[inferred]` yellow, `[ambiguous]` red, `[expected_external]` dim
  - Arrows (`→`, `←`) dim
  - Section-header separators dim
- **Color opt-out**: auto-disable when stdout is not a TTY; honor `NO_COLOR=1` env var (standard); add `--no-color` flag to the `explain` subcommand for explicit override.

### Tests

- Snapshot-style test on the printer with colors disabled (deterministic golden string).
- Update existing 3 `explain_*` tests in `query.rs` for the `Vec<ExplainEdge>` field shape.

## Out of scope

TUI / interactive node navigation / MCP prompt surface / shell REPL color polish / pagination beyond the top-10 cap.

## Acceptance

- `cargo fmt --all -- --check` clean, `cargo clippy --workspace -- -D warnings` clean, `cargo test --workspace` passes.
- `graphify explain src.server --config graphify.toml` on self-dogfood produces grouped, colored, capped output.
- `NO_COLOR=1 graphify explain ...` produces plain text (for CI / grep friendliness).
- `graphify --version` reports `0.12.1` after release cycle.

## Subtasks

- [ ] Slice 1 — enrich `ExplainReport` + update 3 query-engine tests
- [ ] Slice 2 — printer polish (sections, colors, caps) + TTY/NO_COLOR detect + `--no-color` flag
- [ ] Slice 3 — snapshot test on colorless printer output
- [ ] Slice 4 — bump Cargo.toml to 0.12.1, commit, tag, push, `cargo install --path crates/graphify-cli --force`
- [ ] Bonus — fix `docs/TaskNotes/Tasks/sprint.md` missing `uid:` frontmatter (silences recurring `tn` warning); commit separately

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
