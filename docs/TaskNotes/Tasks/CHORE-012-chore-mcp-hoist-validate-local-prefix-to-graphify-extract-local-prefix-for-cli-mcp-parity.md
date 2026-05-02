---
uid: chore-012
status: open
priority: low
scheduled: 2026-05-02
pomodoros: 0
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# chore(mcp): hoist validate_local_prefix to graphify-extract::local_prefix for CLI/MCP parity

FEAT-050 follow-up. Surfaced during Task 7 of the FEAT-049 (mislabel — actually FEAT-050) implementation: MCP `load_config` (`crates/graphify-mcp/src/main.rs:158-173`) is a duplicate of CLI's `load_config` and does NOT call `validate_local_prefix`. Result: configs with `local_prefix = ["src"]` (single-element array) emit a "use string form" warning when invoked via the `graphify` CLI but NOT when invoked via the MCP server. Also: empty array fail-fast and PHP rejection only fire from CLI.

## TL;DR

Move `validate_local_prefix` from `crates/graphify-cli/src/main.rs` into `crates/graphify-extract/src/local_prefix.rs` (the canonical home for the type). Have both CLI and MCP `load_config` paths call it.

## Description

Today the validator lives at `crates/graphify-cli/src/main.rs:1925` (free function) and is invoked by `load_config` (CLI side, `:2013-2031`). MCP duplicates the config-loading code without it. The validator only depends on `LocalPrefix` (now in `graphify-extract`) and language strings — no CLI-specific dependencies — so hoisting is mechanical.

## Subtasks

- [ ] Move `validate_local_prefix` into `crates/graphify-extract/src/local_prefix.rs` as a `pub fn`. Keep the same signature: `(project_name: &str, lp: &Option<LocalPrefix>, languages: &[String]) -> Result<Option<String>, String>`.
- [ ] Re-export from `crates/graphify-extract/src/lib.rs` alongside `LocalPrefix` / `EffectiveLocalPrefix`.
- [ ] Update CLI `load_config` to call `graphify_extract::validate_local_prefix` instead of the local copy.
- [ ] Update MCP `load_config` to call it too — same shape (Err → exit 1, Ok(Some(warn)) → eprintln!, Ok(None) → no-op).
- [ ] Move the 7 unit tests from `crates/graphify-cli/src/main.rs` to `crates/graphify-extract/src/local_prefix.rs` `mod tests`.
- [ ] Smoke verify: run MCP with a `[settings]` block containing `local_prefix = ["src"]` (single-element array). stderr should now show the "Prefer the string form" warning.

## Notes

- Mechanical change. Estimated 30min including tests.
- Closes the documented limitation in `CHANGELOG.md` `## [0.14.0]` "Known Limitations" section.
- Once shipped, remove that limitation note from CHANGELOG and possibly add a release note.

## Related

- Parent feature: `[[FEAT-050]]` — multi-root local_prefix v0.14.0
- Related convention: `crates/graphify-extract/src/local_prefix.rs` is already the home for `LocalPrefix` + `EffectiveLocalPrefix` types
- CLAUDE.md note about MCP/CLI config duplication: "MCP server config is duplicated from CLI (small, stable structs — extract if a third consumer appears)" — this CHORE is the natural extraction point
- [[sprint]] - Current sprint
- [[activeContext]] - Active context
