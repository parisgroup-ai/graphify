---
uid: bug-016
status: done
priority: normal
scheduled: 2026-04-20
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- bug
- rust
- extractor
- discovered-via-dogfood
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# BUG: Rust extractor `crate::` resolution drops local_prefix, intra-crate edges land on non-local placeholder nodes

The Rust resolver path for `use crate::foo::Bar` strips `crate::`, replaces `::` with `.`, and looks up `foo.Bar` in `known_modules` — but local modules are stored with the auto-detected `local_prefix` (e.g. `src.foo`), so the lookup misses, the edge target becomes a non-local placeholder (`foo.Bar`), and intra-crate hub structure is invisible. Discovered while dogfooding graphify on its own 5-crate workspace; same bug shape as BUG-001 (Python relative imports) and BUG-007/011 (TS workspace alias mangling).

## Repro

```bash
# In graphify repo root with the dogfood graphify.toml:
graphify run --config graphify.toml
graphify explain "src.graph" --config graphify.toml
```

The `graphify-core` `src.graph` module (`crates/graphify-core/src/graph.rs`) reports `out_degree = 29`. Among its outgoing edges:

```
→ types.Node       ← should be src.types.Node
```

Reverse confirmation:

```bash
graphify explain "src.types.Node" --config graphify.toml
# In-degree: 0    ← should be ~5+ (referenced from graph.rs, metrics.rs, etc.)
```

Every local hotspot across all 5 crates reports `in_degree = 1`, `betweenness = 0` — implausible given the actual call graph (e.g. `src.graph.CodeGraph` is referenced from ~10+ call sites across `metrics.rs`, `cycles.rs`, `query.rs`, `community.rs`).

## Root cause

`crates/graphify-extract/src/resolver.rs:271-276`:

```rust
// 6. Rust `crate::`, `super::`, `self::` imports.
if let Some(rest) = raw.strip_prefix("crate::") {
    let resolved = rest.replace("::", ".");                     // ← bug
    let is_local = self.known_modules.contains_key(&resolved);  // ← misses
    return (resolved, is_local, 0.9);
}
```

Walker auto-detects `local_prefix = "src"` for every Rust crate (FEAT-011), so known modules are stored as `src.types`, `src.graph`, etc. The `crate::types::Node` strip yields `types.Node`, which does not start with `src.`, so the lookup fails and the resolver returns a non-local id.

Compare to the immediately-following `super::` / `self::` branch (lines 277-281), which delegates to `resolve_rust_path(raw, from_module, ...)` — that path correctly walks up from `from_module` (which already has the `local_prefix`), so `super::types::Node` from `src.community` resolves to `src.types.Node` correctly. Only `crate::` is broken.

## Why it survived FEAT-003 + the existing test suite

`crates/graphify-extract/src/resolver.rs:1485-1502` tests `crate::handler` resolution but uses `make_rust_resolver()` which seeds known_modules **without** a `local_prefix` (modules are `handler`, `models.user`). The test passes because the bug only manifests when `local_prefix` is non-empty, and the test fixture sets it to "".

Every real Rust crate auto-detects `local_prefix = "src"` via the FEAT-011 walker logic (since all files live under `src/`). The test fixture should add a `local_prefix = "src"` variant.

## Fix sketch

In `resolver.rs:272-275`, prepend `local_prefix` when non-empty and not already present:

```rust
if let Some(rest) = raw.strip_prefix("crate::") {
    let stripped = rest.replace("::", ".");
    let resolved = if self.local_prefix.is_empty() || stripped.starts_with(&self.local_prefix) {
        stripped
    } else {
        format!("{}.{}", self.local_prefix, stripped)
    };
    let is_local = self.known_modules.contains_key(&resolved);
    return (resolved, is_local, 0.9);
}
```

Confirm `ModuleResolver` already carries `local_prefix` in its struct (it does — referenced elsewhere in the file). If the prepend logic is duplicated in 2+ resolver branches after this fix, factor to a helper `apply_local_prefix(&self, id: String) -> String`.

## Test plan

1. Extend the existing `make_rust_resolver()` fixture or add a sibling `make_rust_resolver_with_prefix("src")` that seeds known_modules as `src.handler`, `src.models.user`.
2. Add tests covering:
   - `resolve("crate::handler", "src.services.db", false)` → `("src.handler", true, 0.9)`
   - `resolve("crate::models::user", "src.handler", false)` → `("src.models.user", true, 0.9)`
   - `resolve("crate::types::Node", "src.graph", false)` → `("src.types.Node", true, 0.9)` (the smoking-gun case from this bug)
3. Keep the existing prefix-empty tests green.
4. Re-run `graphify run --config graphify.toml` (the dogfood config in repo root). Expect `src.types.Node` `in_degree` to jump from 0 to ~5+, and local hotspots in `graphify-core` to show realistic in-degrees and non-zero betweenness.

## Impact / blast radius

- **Severity**: medium. Doesn't crash, doesn't produce wrong cycles, doesn't break drift detection. But intra-crate hub/bridge structure is invisible in every Rust analysis with non-empty `local_prefix` — which is essentially every real Rust project.
- **Affected reports**: `analysis.json`, `architecture_report.md`, HTML visualization — all show artificially low in-degree/betweenness for local Rust nodes. Cross-crate edges via `graphify_core::types::Node` (the path other crates use) are unaffected — those don't go through the `crate::` branch.
- **Drift detection**: any prior baseline taken with this bug would lock in artificially low scores; a fix would surface as a "false positive" hotspot growth. Worth annotating any existing Rust baselines as pre-BUG-016.
- **No change to confidence semantics**: still 0.9 / Extracted, just the id is now correct.

## Out of scope

- Bare-name Rust call resolution (`CodeGraph::new()` without `crate::` qualifier). That's a separate, harder problem analogous to bare-call resolution in Python/TS — emits Calls edges to the bare symbol name (`CodeGraph`) which never matches a local module id. Could be addressed via symbol-aware fallback lookup but is its own design discussion. **Leave for a follow-up FEAT** if intra-crate visibility is still insufficient after this fix.
- Adding `external_stubs` config for std/serde/petgraph/clap to silence external hotspots in the dogfood report. Tracked separately as a follow-up to issue #12 (`external_stubs` shipped). Not blocking this bug.

## Discovered context

- Discovered 2026-04-20 while running graphify on its own 5-crate workspace (suggestion #1 from the prior session brief, deferred 3+ sessions). Dogfood config landed in `graphify.toml` at repo root.
- Symptoms in dogfood run: every local hotspot across all 5 crates had `in_degree = 1` and `betweenness = 0`. Top-of-summary hotspots were 100% external (`std::collections::HashMap`, `Some`, `serde::Deserialize`).
- Same bug-shape family as BUG-001 (Python relative `from .` resolution) and BUG-007/011 (TS workspace alias mangling). Pattern: resolver branch handles a language-specific prefix but forgets to apply the project-level `local_prefix` re-prepend.
