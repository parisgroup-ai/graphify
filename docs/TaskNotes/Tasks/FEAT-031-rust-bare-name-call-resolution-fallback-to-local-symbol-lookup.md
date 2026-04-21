---
uid: feat-031
status: open
priority: normal
scheduled: 2026-04-21
pomodoros: 0
tags:
- task
- feat
- rust
- extractor
- bug-016-followup
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: medium
  hintsInferred: true
---

# FEAT: Rust bare-name call resolution — fallback to local-symbol lookup for unqualified calls

After BUG-016 fixed `crate::` resolution, `src.types.Node` in_degree jumped 0 → 3 in the graphify self-dogfood. Real call count is closer to ~10+ — the gap is bare-name calls like `Node::new(...)` after a top-of-file `use crate::types::Node;`. The extractor emits a Calls edge to the bare symbol `Node`, the resolver's bare-name fallback at `crates/graphify-extract/src/resolver.rs:290-292` looks it up in `known_modules` (which contains module ids like `src.types.Node` from FEAT-019's symbol-as-node behaviour for Rust), and the lookup misses — so the edge lands on a non-local placeholder. This explicitly out-of-scope follow-up from BUG-016 is what closes the remaining ~40-60% of the intra-crate visibility gap.

## Repro

After BUG-016 (commit `19f9845`):

```bash
graphify run --config graphify.toml
graphify explain "src.types.Node" --config graphify.toml
# In-degree: 3   ← post-BUG-016, was 0 before
```

But `src/graph.rs` actually creates Node values via `Node::new(...)` and `Node::symbol(...)` from ~7+ call sites across `graph.rs`, `metrics.rs`, `community.rs`, etc. Each of those is captured by the Rust extractor's `extract_calls_recursive` as a Calls edge with target `Node` (the bare type name, after a `use crate::types::Node;` at the top of the file). The bare-name resolver fallback doesn't know to look these up against the local symbol pool, so they land on a non-local placeholder.

## Why this isn't a copy of BUG-016

BUG-016 was about `crate::`-qualified imports — a single-line `use crate::types::Node;` resolves correctly post-fix. But the *call sites* in the function bodies are unqualified (`Node::new(...)`, `EdgeKind::Imports`, `CodeGraph::default()`, etc.). The resolver's call-edge handling has no symbol-aware lookup; it treats every bare name as either a known *module* or an external reference.

This is a known design limitation called out in CLAUDE.md (`Bare call sites: confidence 0.7/Inferred (unqualified callee)`) and was left intentional in the BUG-016 spec under "Out of scope". This task addresses it explicitly.

## Design

Add a bare-name fallback that, when the resolver's bare-name path returns non-local, attempts a secondary lookup against a per-resolver index of **`use`-imported short names → canonical module/symbol id**.

Pipeline:

1. The Rust extractor already records every `use` declaration's path. Currently it emits an Imports edge from the file module to the imported path. **Extend** the extractor to also produce a `bare_name → resolved_id` mapping per file: e.g. `use crate::types::Node;` in `src/graph.rs` produces `("Node", "crate::types::Node")` for that file's scope.
2. Plumb this mapping through `ExtractionResult` (new field `use_aliases: HashMap<String, String>`) so the post-extraction resolver pass can consult it.
3. In the post-extraction `resolve_edges` step in graphify-cli `run_extract_with_workspace`, when an edge target is bare and the bare-name resolver returns non-local, try the per-source-module `use_aliases` map. If a hit, re-resolve the aliased path through the existing `crate::` branch (which now correctly applies `local_prefix` post-BUG-016).
4. Confidence: stays at 0.7/Inferred. Even with the alias-assist, we're not 100% certain `Node` in a function body is the imported `Node` (could be shadowed). The lookup is a hint, not a proof.

This keeps language-specific knowledge in the extractor (`use_aliases` is computed during AST walk), not the resolver. The resolver stays language-agnostic.

## Boundaries (v1)

- Only `use foo::bar::Baz;` short-name aliases. `use foo::bar::*;` (wildcard) and `use foo::bar::{Baz, Qux};` (group import) are explicitly out of scope for v1 — wildcards lose information at parse time, group imports need an extractor extension to track per-name. Track as v2.
- No shadowing detection. If a function body declares `let Node = …;` then calls `Node::method(…)`, we'd misattribute. Acceptable false-positive rate per the existing 0.7 confidence.
- No method-call resolution: `foo.bar()` where `foo` is a local variable. Only `Type::method()`-style associated function calls and module-qualified calls.

## Test plan

1. Unit test: extend an existing Rust extractor fixture to include a `use crate::types::Node;` at file top + `Node::new(...)` in a function body. Assert that the new `use_aliases` field on `ExtractionResult` contains `("Node", "crate::types::Node")`.
2. Integration test: build a 2-file Rust crate fixture with one type defined in `types.rs` and used via `Node::new()` in `graph.rs`. After full pipeline, assert that `src.types.Node` has `in_degree >= 1` from the `src.graph` module.
3. Regression: re-run graphify on its own 5-crate workspace. Expected: `src.types.Node` `in_degree` jumps from 3 (current post-BUG-016) to ~10+. `src.graph.CodeGraph` score should also climb measurably.
4. Negative test: a bare name that's NOT in any `use` declaration (e.g. `Vec::new()`) stays as external — must not get spuriously promoted to local.

## Acceptance criteria

- `cargo test --workspace` passes
- `cargo clippy --workspace -- -D warnings` clean
- `cargo fmt --all -- --check` clean
- Re-running graphify on its own workspace shows measurably higher in_degree for the canonical types (`src.types.Node`, `src.graph.CodeGraph`, `src.lang.LanguageExtractor`) compared to the post-BUG-016 baseline at commit `f4ac5e2`
- Confidence semantics unchanged: bare-name calls still 0.7/Inferred even when alias-assisted

## Out of scope

- Wildcard imports (`use foo::*;`) — track for v2
- Group imports (`use foo::{a, b};`) — track for v2
- Method-call resolution on local variables
- TypeScript / Python equivalent — those languages have similar bare-name gaps but different aliasing semantics; address per-language

## Discovered context

Discovered 2026-04-21 immediately after shipping BUG-016 (graphify v0.11.3). The dogfood report at `f4ac5e2` showed `src.types.Node` `in_degree=3` against an actual call count of ~10+. The gap is consistent with bare-name calls bypassing the now-fixed `crate::` resolution. Same pattern likely affects `src.graph.CodeGraph` (post-fix score 0.364, in_deg=7; actual usage closer to 12+) and `src.lang.LanguageExtractor` (post-fix in_deg=6; trait implemented by 5 extractors + called from ~3 sites).
