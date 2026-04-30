---
uid: bug-028
status: done
priority: normal
scheduled: 2026-04-30
completed: 2026-04-30
pomodoros: 0
tags:
- task
- bug
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: false
---

# fix(session): `baseline_age_days` reads dir mtime instead of `analysis.json` content/mtime

GH issue #15 — `graphify session brief` reports `stale: true` and `baseline_age_days: 12` even after the live analysis is regenerated and copied to `report/baseline/analysis.json`. The "12d" comes from the directory mtime of `report/baseline/`, which only updates when entries are added/removed — overwriting an existing file (the standard promotion gesture `cp report/<proj>/analysis.json report/baseline/analysis.json`) does NOT update the dir mtime. Root fix: add a top-level `generated_at` (ISO 8601 UTC) to `analysis.json` and have `baseline_age_days` read it from the freshest `analysis.json` under `report/baseline/`. Falls back to the file mtime (not dir) when the field is absent for legacy compat.

## Description

`crates/graphify-cli/src/session.rs::baseline_age_days` calls `std::fs::metadata(report/baseline).modified()` and converts to days. POSIX directory-entry semantics: an in-place file overwrite leaves the dir mtime untouched, so the function permanently reports the age of the dir's first creation, not the freshness of the contents.

`--force` doesn't help because it only regenerates the brief — it calls the same broken function.

## Approach (option C from session conversation)

Structural fix: tag every `analysis.json` with the moment it was written.

1. Add `generated_at: String` (ISO 8601 UTC, format `%Y-%m-%dT%H:%M:%SZ`) to the top-level of `analysis.json` via `graphify-report::json::write_analysis_json_with_allowlist`. Reuse the `format_epoch_seconds_utc` helper already present in `graphify-report::consolidation` (extract to a small `time_utils` module so json.rs and consolidation.rs share it without depending on `chrono`).
2. Add `pub generated_at: Option<String>` to `graphify_core::diff::AnalysisSnapshot` with `#[serde(default)]` for backward compatibility.
3. Rewrite `graphify-cli::session::baseline_age_days` to:
   - Look for `analysis.json` files inside `report/baseline/` at depth 0 and 1 (covers both single-project `baseline/analysis.json` and multi-project `baseline/<proj>/analysis.json`)
   - For each found file, parse the top-level `generated_at` if present; else fall back to file mtime
   - Return the **smallest age** (= youngest baseline) across all found files
   - Return `None` when no `analysis.json` is found under baseline
4. Tests: add fixtures under tempdirs covering (a) field present → age from field, (b) field absent → fallback to file mtime, (c) no baseline dir → None, (d) multi-project layout → youngest wins, (e) confirm dir-mtime is NOT consulted.
5. Update CLAUDE.md schema notes (analysis.json carries `generated_at` post-0.13.7).
6. Bump workspace version 0.13.6 → 0.13.7, build, install local, smoke-test against this repo's report/.

## Acceptance criteria

- After `graphify run --config graphify.toml` followed by `cp report/<any-project>/analysis.json report/baseline/analysis.json`, running `graphify session brief --force` reports `baseline_age_days: 0` (not 12, not the dir-creation age).
- Legacy `analysis.json` written by 0.13.6 or earlier still loads in `graphify diff` and `graphify session brief` — the `generated_at` field is optional everywhere it's read.
- All existing `cargo test --workspace` tests pass; new unit tests cover the fallback paths.
- `cargo fmt --all` + `cargo clippy --workspace -- -D warnings` clean.

## Out of scope

- FEAT-048 cross-crate `pub use` workspace fan-out (still gated, low priority).
- Documenting `graphify session brief --help` (separate UX polish; the bug fix makes the warning correct, which obviates the doc gap).
