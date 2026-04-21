---
uid: bug-018
status: done
priority: normal
scheduled: 2026-04-21
completed: 2026-04-21
pomodoros: 0
tags:
- task
- bug
- resolver
- confidence
- feat-031-followup
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# fix(extract): local Calls edges tagged Ambiguous/0.5 after FEAT-031 scoped-call resolution

FEAT-031 scoped-identifier calls land on the correct local symbol id (e.g. `src.lang.ExtractionResult.new`) and contribute to in_degree, but the edge arrives with `confidence = 0.5` and `confidence_kind = Ambiguous`. The edge IS correct — the hit was not — but the confidence signal lies about whether it's a real local-to-local call or a cross-language guess.

## Root cause

`ModuleResolver::known_modules` only registers **module-level** ids built from file walker output. `Defines` targets (symbol-level ids like `src.lang.ExtractionResult.new`) are never seeded, so `resolver.rs::resolve()` sees the call target as non-local and applies the `is_local=false` downgrade (`min(conf, 0.5)` + `Ambiguous`) at the single downgrade site in `graphify-cli/src/main.rs` (around line 2520). FEAT-031's case-9 fallback rewrote the callee path correctly, but the subsequent lookup path doesn't know symbol-level names are locally-owned.

## Fix options

1. **Register symbol-level ids explicitly** — before the edge-resolution loop, iterate over `Defines` edges in `all_raw_edges` and register each target (`src.foo.Bar`, `src.foo.Bar.method`) as a local module in the resolver. Same shape as the existing package_modules pass. Probably the smallest surface.
2. **Register during extraction** — have each extractor emit symbol-level ids alongside module-level ids into `ExtractionResult::known_local_symbols: HashSet<String>`, then the resolver absorbs them via `register_local_symbols()`. Cleaner but touches 5 extractors + the cache.
3. **Post-resolve rewrite** — after `resolve()` returns `is_local=false`, do a second lookup in the live graph's node index. Cheapest implementation; dirtiest design (two-phase resolution with phase-boundary side effects).

Recommend option 1 — one loop, scoped to graphify-cli + graphify-mcp, additive.

## Acceptance

- `graphify explain src.lang.ExtractionResult.new --config graphify.toml` shows one or more incoming Calls edges with `confidence: 1.0` / `confidence_kind: Extracted` (or at least non-Ambiguous).
- No new cycles; hotspot scores move only where confidence stops capping contribution.
- `cargo test --workspace` green, clippy clean.
- A test in `graphify-cli` integration suite (or a unit test via a small graph factory) asserts that a bare call resolving to a `Defines` target keeps full confidence.

## Out of scope

- Changing the non-local downgrade rule for genuinely unresolved targets.
- Rebalancing `ConfidenceKind` for module-level ids.

## Discovered context

Surfaced in the v0.11.5 session brief (post-FEAT-031/BUG-017 ship, 2026-04-21) as "FEAT-031 confidence classification for local Calls edges" and carried across two session-brief cycles. Filed as BUG-018 in the 2026-04-21 evening session (post-FEAT-033/034 ship, v0.11.7).
