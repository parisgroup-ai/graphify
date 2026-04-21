---
uid: bug-019
status: open
priority: normal
scheduled: 2026-04-21
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- bug
- resolver
- confidence
- bug-018-followup
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# bare same-module Calls edges stay Ambiguous after BUG-018

BUG-018 (v0.11.8) fixed the FEAT-031 scoped-call path: `src.graph → src.types.Node.module` now arrives at `0.7/Inferred` instead of `0.5/Ambiguous`. But the bulk of local Calls on graphify's self-dogfood are still Ambiguous — e.g. `src.community → build_communities`, `src.consolidation → anchor`, `src.contract → project_fields`. These are **bare same-module helper calls**: the call is `build_communities()` inside `src.community`, targeting a function defined in the same module.

## Why BUG-018's fix doesn't cover these

BUG-018 registers symbol-level `Defines` targets as known local modules, so `src.community.build_communities` IS in `known_modules` now. But the Rust extractor emits the raw Calls target as the bare leaf `build_communities`, not the qualified `src.community.build_communities`. Resolver case 9 (`use`-alias fallback) doesn't rewrite it because there's no `use` statement — the function is defined in the same file. Case 8 (direct lookup) sees `known_modules.contains_key("build_communities")` → false, so resolve returns `(build_communities, false, 1.0)` → non-local downgrade → 0.5/Ambiguous.

Dogfood shape (post-BUG-018, graphify on itself): 394 Ambiguous Calls across 5 crates, most of which fit this "bare leaf targeting a same-file helper" pattern.

## Fix options

1. **Resolver-side: synthesize `{from_module}.{raw}` lookup.** Before case 9, if `raw` is a bare identifier (no `::`, no `.`, no leading dot), try `known_modules.contains_key(&format!("{}.{}", from_module, raw))`. On hit, return `(synthesized_id, true, 1.0)`. Cheap, one extra HashMap lookup per Calls edge, purely additive. Downside: over-matches when a module and a symbol both exist with the same leaf name (e.g. module `foo` and function `foo` in the same namespace); a `from_module.raw` lookup for a call `foo()` could wrongly land on the module rather than the function. In practice these collisions are rare because the extractor emits `Defines` edges for symbols but modules are registered via `register_module_path`; the symbol side always wins. Still — needs a test covering the collision shape.
2. **Extractor-side: emit qualified Calls targets.** Change `rust_lang.rs::extract_calls_recursive` to emit `{module_name}.{callee}` instead of just `{callee}` when the call is a bare identifier and no `use {callee}` is registered. Shifts the burden to extraction. Cleaner contract (extractor knows the module), but touches every bare-identifier code path across 5 extractors (Rust, Python, TS, Go, PHP) and the cache format (`ExtractionResult::edges` entries now carry qualified strings, so cache-v1 entries need eviction).
3. **Post-resolve fallback: try `{from_module}.{raw}` only after the non-local downgrade fires.** Minimal footprint — wraps the downgrade site in graphify-cli + graphify-mcp, doesn't touch the resolver. Downside: dirty two-phase design (same anti-pattern BUG-018's brief rejected option 3 for).

**Recommend option 1.** One extra `known_modules` lookup inserted between case 8 and case 9 in `ModuleResolver::resolve_with_depth`. Resolver-internal, no pipeline changes needed.

## Acceptance

- `graphify explain src.community.build_communities --config graphify.toml` shows incoming Calls edges with `confidence_kind != Ambiguous` (likely `Inferred` at 0.7 from the bare-call extractor confidence).
- Self-dogfood: Ambiguous Calls share drops meaningfully (target: > 200 of the 394 current Ambiguous Calls promote to Inferred).
- Regression guard: a resolver unit test asserting `resolve("foo", "src.community", false)` returns `("src.community.foo", true, 1.0)` when `register_module("src.community.foo")` was called.
- Negative test: `resolve("Vec", "src.graph", false)` with no `src.graph.Vec` registered must return `("Vec", false, ...)` — don't promote truly-external calls.
- `graphify check` still PASS on all 5 self-dogfood projects; no new cycles.
- `cargo test --workspace` green, clippy clean.

## Out of scope

- Cross-language equivalent (Python bare calls `foo()` inside a module where `foo` is a same-module helper). Same fix shape should apply — once option 1 is proven on Rust, extending to Python is mechanical. But file as a separate slice with its own regression test set.
- Method calls like `self.foo()` / `obj.foo()` (Rust `field_expression` shape). Those stayed out-of-scope per the FEAT-031 v1 policy and this task doesn't change that.

## Discovered context

Filed 2026-04-21 during the v0.11.8 dogfood check after BUG-018 shipped. Measured: 119 Calls in graphify-core with confidence_kind breakdown `{'ExpectedExternal': 61, 'Ambiguous': 56, 'Inferred': 2}`. Sample Ambiguous calls all match the bare-leaf-to-same-file-helper pattern.
