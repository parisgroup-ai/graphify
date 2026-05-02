# Active Context


## Notes

- **2026-04-30** — FEAT-048 closed as done — gate failed (1/5 cross-crate hits), parked via ADR-0002. Re-file if evidence shifts.
- **2026-05-02** — FEAT-050 shipped as v0.14.0: multi-root `local_prefix` (string OR array form, no-wrap mode for Expo Router). 14 commits `dcefa79..7d85e10`, +1500 LOC, 942 tests, dogfood byte-identical hotspots vs baseline. **Label drift:** spec/plan/commits all say "FEAT-049" but the slot was already taken by the Rust pub-type-alias task done 2026-04-27 — the actual tn task is FEAT-050. Cross-ref in `[[FEAT-050]]`.
- **2026-05-02** — `is_local` and per-node `language` live in `graph.json`, NOT `analysis.json` (analysis.json is `MetricsRecord`-only + `edges[]`). Tooling that needs node-level provenance must read `graph.json`. Surfaced by FEAT-050 Task 10 integration test.
- **2026-05-02** — MCP server `load_config` is duplicated from CLI and skips `validate_local_prefix` — single-element-array warning (FEAT-050) fires from CLI but not from MCP. Tracked by `[[CHORE-012]]` (hoist validator into `graphify-extract::local_prefix`).
- **2026-05-02** — `[[CHORE-012]]` shipped as v0.14.1: `validate_local_prefix` hoisted to `graphify-extract::local_prefix` (re-exported from crate root); both CLI and MCP `load_config` now share validation + DOC-002 PHP+string warning. 6 unit tests moved alongside. Commits `2297962` (impl) + `eee0fa6` (release). `[[FEAT-050]]` v0.14.0 tag also pushed in same session — release.yml ran for both tags in parallel. GH #16 closed referencing v0.14.0.
