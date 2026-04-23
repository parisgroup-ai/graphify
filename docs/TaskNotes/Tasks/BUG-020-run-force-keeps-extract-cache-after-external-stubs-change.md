---
uid: bug-020
status: open
priority: normal
scheduled: 2026-04-22
pomodoros: 0
projects:
  - "[[sprint.md|Current Sprint]]"
contexts:
  - cli
  - cache
  - extract
  - config
tags:
  - task
  - bug
  - cli
  - cache-invalidation
  - dx
  - external_stubs
  - needs-consumer-evidence
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: high
  hintsInferred: true
---

# investigation: `external_stubs` edits appear to need `rm -rf report/<project>/` to take effect (not reproduced in v0.12.0)

## Status

**Filed as investigation, not confirmed bug.** The reported symptom could not be reproduced against graphify v0.12.0 in three controlled fixtures, and the proposed root cause is architecturally impossible in current code. Filing so that if another consumer hits the same symptom the diagnosis trail is already here — and so the minimum evidence needed to re-open is documented up front.

## Reported symptom (consumer side)

Filed 2026-04-22 while onboarding `external_stubs` across a 13-project TypeScript + Python monorepo (ToStudy).

After editing `external_stubs` in `graphify.toml` and running `graphify run --force`, the consumer observed that `report/<project>/analysis.json` did not reflect the new stubs — `confidence_kind == "ExpectedExternal"` counts stayed at zero. Only after `rm -rf report/<project>/` and re-running did the classification flip correctly.

Consumer-observed example (ana-service, 7-line Python config):

```toml
[[project]]
name = "ana-service"
repo = "./apps/ana-service"
lang = ["python"]
local_prefix = "app"
external_stubs = ["typing", "logging", "fastapi", "pydantic"]
```

```bash
jq '[.edges[] | select(.confidence_kind == "ExpectedExternal")] | length' \
  report/ana-service/analysis.json
# After --force alone:          → 0
# After rm -rf + --force:       → 1899
```

### Reported monorepo-wide impact (consumer side)

Applying the `rm -rf` workaround across 13 projects shifted Ambiguous edge counts by −40%:

- Ambiguous edges: 15 695 → 9 429 monorepo-wide
- web: 57% → 27% (−30 pp)
- api: 69% → 33% (−36 pp)
- mobile: 64% → 40% (−24 pp)
- pkg-email: 44% → 8% (−36 pp)

The delta is real — but this investigation cannot yet explain what produced it. Candidate explanations below.

## Investigation against v0.12.0 @ `b28ed2f` — NOT REPRODUCED

Three fixtures, tested with both the installed PATH binary (`graphify 0.12.0`) and a fresh `cargo build --release` from `main @ b28ed2f`. All three pass without `rm -rf` and without `--force`:

1. **Single-file Python** (`/tmp/gf-bug020`, 8 lines importing `typing`/`logging`/`fastapi`/`pydantic`):  
   baseline `{ambiguous: 8, expected_external: 0}` → after adding `external_stubs = ["typing","logging","fastapi","pydantic"]` → `{ambiguous: 0, expected_external: 8}`. Correct.

2. **Multi-project (Python + TypeScript)** (`/tmp/gf-bug020b`, two `[[project]]` blocks):  
   both projects flip from all-Ambiguous to all-ExpectedExternal after per-project `external_stubs`. Correct.

3. **Realistic `app/`-prefixed Python package** (`/tmp/gf-ana`, mirror of reported ana-service shape, 3 files across `app/services/` + `app/models/`):  
   baseline `{ambiguous: 20, expected_external: 0}` → `{ambiguous: 0, expected_external: 20}` after adding stubs. Correct.

All three scenarios also work **without `--force`** — adding `external_stubs` and re-running plain `graphify run` re-tags immediately because the stubs matcher runs on cache-hit edges too.

## Why the reported root cause is architecturally impossible in v0.12.0

`external_stubs.matches()` is called exactly once in the CLI pipeline — inside the edge-resolution loop at `crates/graphify-cli/src/main.rs:2570`, which runs unconditionally on every edge every run. The extract cache (`.graphify-cache.json`) stores `ExtractionResult`s whose edges carry extractor-time confidence (`Extracted` or `Inferred`) — never `ExpectedExternal` or `Ambiguous`. Those two classifications are computed downstream during resolution, on both cache hits and cache misses. `--force` already short-circuits the cache load at `main.rs:2014` and `main.rs:1770` — but this does not matter for the reported symptom, because the matcher would apply on cache hits too.

This architecture has held since the first commit introducing the feature (`6fc492b`, v0.11.5, 2026-04-17). No subsequent commit (FEAT-033 v0.11.6, FEAT-034 v0.11.7, or the 0.12.0 bump) moved the matcher into the extractor or into cached state.

Additional paths ruled out by code inspection:

- **Extract cache reuse of classification:** cache never stores the tag.
- **Resolver loop skipped on cache hit:** empirically works without `--force` in all three fixtures.
- **Report writer caching `analysis.json`:** `write_analysis_json_with_allowlist` at `crates/graphify-report/src/json.rs:189` unconditionally overwrites.

## Candidate explanations for the consumer-observed delta

Not ruled out by current evidence — any of these would need consumer-machine data to confirm:

1. **Shadowed binary.** A `./node_modules/.bin/graphify`, a locally-built older binary on `PATH` ahead of the v0.12.0 install, or a pnpm workspace script pinning a version. `which graphify && graphify --version` on the consumer would disambiguate.

2. **Pre-0.11.7 binary + `[settings].external_stubs` block.** FEAT-034's settings-level merge didn't exist before 0.11.7. An older binary would silently accept the TOML (serde is lenient about unknown fields) and never apply settings-level stubs. The `rm -rf` workaround would have no effect in that world either, so this isn't a full match — but a **mix** (project-level stubs + an older binary with a different bug) could produce confusing data.

3. **Inspected the wrong `analysis.json`.** Shell-history accident — `jq`-ing a different project's file, or checking before `graphify run` finished writing.

4. **Config format mismatch against binary version.** Using the settings-level merge form against a binary that only supports per-project, or vice-versa.

## Minimum evidence to confirm and re-open as a real bug

- Output of `which graphify && graphify --version` on the consumer machine at the time of the failed run.
- The exact `[[project]]` block + the exact command history showing the failing `--force` run.
- The `before` and `after` `analysis.json` files (or their `.confidence_summary` blocks) around the failing run.
- Ideally: a run with `GRAPHIFY_LOG=trace` (if added) or a `cargo build --release` binary from `main @ b28ed2f` used in place of the installed one, showing the symptom persists.

## Recommendation

Leave open with `needs-consumer-evidence` tag until the consumer can produce the evidence list above. Do not ship a "cache-invalidate on `--force`" change on the strength of this report alone — the cache does not store the classification being discussed, so that change would be a no-op for the reported symptom.

## Discovered context

Filed 2026-04-22 during external_stubs rollout across a 13-project TS+Python monorepo. Investigation done same day against v0.12.0. Consumer-side memory-bank note referenced in the original report lives in the consumer repo, not here: `memory-bank/topics/graphify-architecture.md` §BUG-EXTRACT-CACHE.
