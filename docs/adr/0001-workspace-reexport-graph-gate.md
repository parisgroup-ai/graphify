# ADR 0001 — Workspace-wide ReExportGraph gate

- **Status**: Accepted (2026-04-20)
- **Feature under gate**: FEAT-028 (shipped `v0.11.0`)
- **Decision owner**: solo-dev (Cleiton Paris)
- **Task**: FEAT-030

## Context

FEAT-028 shipped a workspace-wide `ReExportGraph` that fans out cross-project
TypeScript alias imports (`@repo/*`) to the sibling project's *canonical*
module, instead of stopping at the raw barrel symbol. The behaviour is always
on whenever the topology triggers (≥ 2 projects AND ≥ 1 TypeScript project).
Single-project and non-TS-only configs keep the legacy fast path with zero
overhead.

FEAT-028's own plan carried an explicit follow-up (step 8): **decide whether
this default is correct, whether to add a flag, and what the upgrade signal
should be.** Two sessions punted the decision forward; FEAT-030 resolves it.

### Forces

1. **Reproducibility**. A user pinned at `v0.10.x` and a collaborator on
   `v0.11.0+` produce different edge counts for the same monorepo. The
   difference is 100% tool-version-induced.
2. **Surprise factor**. Downstream consumers (`graphify check`,
   `/gf-drift-check`, CI drift gates) threshold on edge-count deltas. The
   first post-upgrade `graphify run` shows a drift with no user code change.
3. **Debugging surface**. When a cross-project edge looks wrong, an operator
   currently cannot force the legacy path to bisect "tool bug vs. real
   dependency."
4. **Maintenance cost of flags**. Any user-facing flag doubles the test
   matrix on the gated path and adds configuration surface that has to be
   documented and kept working over time.
5. **Ship state**. `v0.11.0` is already tagged and published as always-on.
   Any option that retroactively flips the default back to *off* is itself
   a disruption, not a staged rollout.

## Options considered

| ID | Policy | Code paths | Escape hatch | Surprise on upgrade |
|---|---|---|---|---|
| A | Always-on, no flag, loud changelog note | 1 | None | Silent edge-count change on first `run` post-upgrade |
| B | Opt-out flag, default `true` | 2 | `[settings] workspace_reexport_graph = false` | Same as A, but users can pin legacy |
| C | Opt-in flag, default `false` in `v0.11.x`, flip in `v0.12.0` | 2 | Explicit opt-in | Retroactive revert — `v0.11.0` shipped as on |
| D | Opt-out flag + stderr notice every run | 2 | Flag | Loud — log-noise-fatigue kicks in |

## Decision

**Option B — opt-out flag, default `true`.**

Added to `[settings]` as `workspace_reexport_graph: Option<bool>`. Absent
or `true` keeps FEAT-028 behaviour. Explicit `false` forces the legacy
per-project path.

## Rationale

1. **B is the cheapest real escape hatch.** One field on `Settings`, one
   short-circuit in `collect_workspace_reexport_graph`, one integration
   test. The cache is per-file SHA256 (pre-fan-out), so flipping the flag
   does not invalidate extraction caches — workspace-graph fan-out
   happens on top of cached extractions, not inside them.

2. **Default `true` preserves ship state.** `v0.11.0` was published as
   always-on. Users who have already re-baselined their `graph.json` or
   `analysis.json` against the new edge counts are not disrupted. Only
   users who explicitly want the old numbers pay the cost of flipping
   the flag.

3. **Option A lacks a lever.** When a cross-project edge turns out to be
   wrong, Option A leaves the operator with only "downgrade the binary"
   as a workaround. That's disproportionate for a diagnostic case.

4. **Option C is wrong for a shipped feature.** Flipping the default back
   to `false` in `v0.11.x` is a breaking behaviour change dressed up as
   a staged rollout. Anyone who has re-baselined against `v0.11.0` sees
   their drift gates trip *again* on the default flip.

5. **Option D's noise ages badly.** A stderr line on every run gets
   filtered out of CI logs within a day. The same information already
   lives in `graphify-summary.json` as `cross_project_edges` (per
   FEAT-028 step 6), which is the correct surface for quantitative
   diagnostics.

## Consequences

### Positive

- Users can pin legacy edge counts without downgrading the binary.
- `v0.10.x` reproducibility is achievable via config, not version pinning.
- Debuggers have a bisection tool for "is this edge real or a
  workspace-graph artifact?"
- The one extra boolean is discoverable: it shows up in `graphify init`
  templates as a commented line, in the README `[settings]` docs, and
  in the ADR.

### Negative

- Two code paths on the gate. The post-gate code is unchanged between
  modes — the gate simply returns `None` early when the flag is `false`,
  which is the exact return shape the legacy single-project branches
  already handle. Maintenance cost is bounded to the gate line and one
  integration test.
- Users who want the *other* default have to carry a config change.
  Acceptable: the loud behaviour (`true`) is the documented recommended
  default, and the task body's own recommendation pointed at B.

### Neutral

- No cache invalidation needed. The extraction cache
  (`.graphify-cache.json`) is keyed by per-file SHA256 content, which is
  identical under either flag value. The workspace-graph fan-out runs
  *after* the cache is consulted. Flag flips take effect on the next
  `graphify run` with zero cache churn.

## Implementation

- Field: `workspace_reexport_graph: Option<bool>` on
  `graphify-cli::Settings` (absent treated as `true`).
- Gate: `collect_workspace_reexport_graph` returns `None` when the flag
  is explicitly `Some(false)`, before the topology check. Seven existing
  call sites inherit the behaviour without change.
- Template: `graphify init` emits the flag commented-out under
  `[settings]`, with default + pin-legacy hint inline.
- Test: `feat_030_opt_out_flag_restores_legacy_cross_project_path` in
  `tests/integration_test.rs` — mirror of the FEAT-028 test with
  inverted assertions on the `ts_cross_project_alias` fixture.
- Docs: this ADR; `CHANGELOG.md` entry under `[Unreleased]`; CLAUDE.md
  FEAT-028 paragraph updated to point at this ADR.

## Related

- FEAT-028 — workspace-wide `ReExportGraph` (the feature this gate wraps)
- FEAT-029 — `cross_project_edges` redistribution benchmark (measures
  the size-of-effect the gate toggles). Not a blocker for this ADR: the
  decision is about the *existence* of the escape hatch, not the size
  of the effect.
- FEAT-020 — `[consolidation]` settings block, the pattern this field
  follows (opt-in config value on the existing `Settings` struct,
  `None`-is-default, no new sub-struct until more than one related
  field lives together).
