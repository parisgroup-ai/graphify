---
uid: feat-030
status: open
priority: normal
scheduled: 2026-04-20
timeEstimate: 90
pomodoros: 0
timeSpent: 0
contexts:
- settings
- cli
- extract
- workspace
- feature-flag
tags:
- task
- feat
ai:
  parallelParts: 0
  needsReview: true
  uncertainty: med
  estimateTokens: 120000
  hintsInferred: false
---

# feat(settings): decide feature-gate policy for workspace-wide ReExportGraph path

FEAT-028 shipped the workspace-wide `ReExportGraph` always-on when the topology triggers (≥2 projects AND ≥1 TS project). Step 8 of that plan was an explicit decision: keep always-on, add an opt-out flag, or add an opt-in flag — and whether to emit a stderr notice when the workspace path fires so users know their edge counts changed. This task produces that decision with a written rationale, and lands whatever code + docs are implied.

## Description

Today's trigger (`main.rs` run_extract gate) silently upgrades multi-project TS configs from per-project fan-out to workspace-wide fan-out. Single-project and non-TS-only configs keep the legacy fast path with zero overhead (confirmed in the FEAT-028 session notes). The trigger fires on the very first run after upgrading to `v0.11.0+` — so a user with a cached `graph.json` from `v0.10.x` will see their edge counts change on the first rebuild without any configuration change on their side.

The always-on default made sense during development (tests green, zero overhead when topology doesn't trigger). It's less obviously right at steady state:

1. **Reproducibility**: a user pinning graphify at `v0.10.x` and a collaborator on `v0.11.0+` produce different edge counts for the same monorepo. Without a flag, both are "default behavior" for their binary.
2. **Surprise factor**: downstream consumers (`/gf-drift-check`, `graphify check`) gate on analysis deltas. The first post-upgrade run will show a drift that's 100% tool-version-induced, 0% user code change.
3. **Debugging surface**: when an edge looks wrong, a flag to force the legacy path helps bisect "is this a workspace-graph artifact or a real bug?"

Against the flag: always-on means one code path to maintain; a flag means two behaviors, two test matrices, two failure modes.

## Motivation

- FEAT-028's follow-ups named two specific asks: "opt-out flag + stderr notice still to be decided". That decision is blocked on nobody — it's a product call that's been deferred twice (end of session 2026-04-20-1437 → session 2026-04-20-1811 → here).
- FEAT-029's benchmark will produce the size-of-effect number. If the redistribution is tiny, a flag is cheap insurance against surprise; if it's large, the upgrade is a feature worth advertising prominently (changelog entry + README note) rather than hiding behind a flag.
- Users upgrading from `v0.10.x` to `v0.11.0+` will hit the behavior change on their next `graphify run`. The longer we wait to decide, the more users have silently re-baselined their `graph.json` against the new edges and the more disruptive a retroactive flag becomes.

## Likely scope

1. **Decision write-up (ADR-style).** Produce a short doc under `docs/adr/` (or the existing design-doc location if there's an established convention) covering:
   - Option A: keep always-on, no flag, loud changelog note. Single code path.
   - Option B: opt-out flag `[settings] workspace_reexport_graph = false` defaulting to `true`. Users can pin legacy behavior.
   - Option C: opt-in flag defaulting to `false` for `v0.11.x`, flip default to `true` in `v0.12.0`. Slower rollout.
   - Option D: opt-out flag + stderr notice on every run where the workspace path fires ("info: workspace-wide ReExportGraph active across N projects, X cross-project edges emitted"). Most observable, noisiest.
   - Recommendation + tradeoffs table referencing FEAT-029's measured size-of-effect.
2. **Implementation (scoped to chosen option).** If B/C/D: add the flag to the `[settings]` block in the TOML schema (`crates/graphify-cli/src/config` — use the existing settings struct, don't scatter a new one). Thread it through to the gate that currently lives near `main.rs:1893` (call site described in CLAUDE.md FEAT-028 notes). For D, add an `eprintln!` behind the gate. Test: add one integration test per non-default value of the flag.
3. **Documentation updates.** CLAUDE.md's FEAT-028 paragraph loses "feature-gate decision: currently always-on when topology triggers; opt-out flag + stderr notice still to be decided" — replaced with a line pointing at the ADR. README section on `graphify.toml` gets the new flag documented if one is added. CHANGELOG entry for the decision.
4. **Cache interaction check.** The workspace-graph path alters edge counts. Confirm the extraction cache (`.graphify-cache.json`) does not silently serve stale per-file results when the flag flips — if it does, cache version bump OR flag-aware cache key. Likely requires one line in the cache key derivation.
5. **CI / release.** If the decision is anything other than "keep always-on, no flag", the change needs to land before a `v0.11.x` patch release so the flag ships alongside the feature it gates.

## Boundaries / non-goals for v1

- Does NOT revisit the FEAT-028 implementation itself — the workspace-graph code is considered correct (7 tests passing, tripwire inverted). This task is purely about the gate around it.
- Does NOT introduce a per-project override (project-level TOML flag). Workspace-scoped behavior only makes sense at the workspace/config level.
- Does NOT gate the `alternative_paths` writer fan-out (FEAT-025) — that's orthogonal and stays always-on.
- Does NOT touch the single-project fast path — regardless of flag value, single-project and non-TS-only configs keep the legacy path with zero overhead.

## Open questions

- What's the existing convention for feature flags in `graphify.toml`? Grep for prior `[settings]` additions — FEAT-020's `[consolidation]` block is the closest precedent (fail-fast regex validation at config load, `schema_version: 1` on JSON outputs). Follow that pattern for consistency.
- Does `graphify-mcp` need the same flag? The MCP server runs extraction eagerly on startup (per CLAUDE.md); if the flag lives in config, MCP picks it up for free. If the flag ends up being a CLI-only override, MCP needs a parallel mechanism.
- For option D (stderr notice): what's the verbosity default? If every CI run prints the notice, users will filter it out of logs and stop noticing. A quieter option is "only print when the cross-project fan-out changed edge count by >N% since the cached run", but that's a bigger implementation.

## Acceptance criteria

- An ADR or design note exists documenting the options, the chosen option, and the tradeoffs that drove it. Cross-references FEAT-029's benchmark number.
- If a flag is added: it's documented in README + CHANGELOG, has at least one integration test per value, and cache invalidation is handled.
- CLAUDE.md's FEAT-028 paragraph is updated to point at the ADR and drop the "still to be decided" language.
- The decision is reflected in the next `v0.11.x` or `v0.12.0` tag's release notes.

## Related

- [[sprint]] — Current sprint
- [[activeContext]] — Active context
- FEAT-028 — workspace-wide ReExportGraph (parent feature, step 8 deferred to here)
- FEAT-029 — cross-project edges redistribution benchmark (input to the decision)
- FEAT-020 — `[consolidation]` settings block precedent for config-level validation
- FEAT-019 — calibration flow, unrelated but a prior feature that landed a settings-adjacent TOML section
