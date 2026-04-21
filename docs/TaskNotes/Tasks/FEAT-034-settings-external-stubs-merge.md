---
uid: feat-034
status: done
priority: low
scheduled: 2026-04-21
completed: 2026-04-21
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- feat
- config
- external-stubs
- feat-032-followup
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# FEAT: `[settings].external_stubs` merge layer

FEAT-032 shipped per-project `external_stubs` arrays. For single-language workspaces — notably the graphify self-dogfood — every project wants the same ~30 prelude stubs (`std`, `Vec`, `String`, `Some`, `Ok`, `Err`, `format`, `writeln`, `panic`, `assert*`, etc.). Duplicating that list 5 times in `graphify.toml` is noise.

Proposal: add `[settings].external_stubs = [...]` that **merges** with each project's own `external_stubs` (not overrides — both contribute). Simplest semantics: at config-load time, concatenate `settings.external_stubs + project.external_stubs` before passing to `ExternalStubs::new()`. The longest-prefix-wins sort inside the matcher handles any overlap.

## Design

```toml
[settings]
external_stubs = [
  "std",
  "Vec", "String", "Box", "Option", "Result",
  "Some", "None", "Ok", "Err", "Self",
  "format", "writeln", "println", "eprintln", "print", "eprint",
  "vec", "write",
  "assert", "assert_eq", "assert_ne",
  "debug_assert", "debug_assert_eq", "debug_assert_ne",
  "panic", "todo", "unimplemented", "unreachable", "dbg",
]

[[project]]
name = "graphify-core"
repo = "./crates/graphify-core"
lang = ["rust"]
external_stubs = ["petgraph", "rand", "regex", "serde", "serde_json"]
```

After the merge, `graphify-core` gets all shared prelude stubs + its own crate-specific ones. Current repetition drops from 5×30=150 lines to ~30+5×5 = 55 lines in the graphify dogfood config.

## Implementation

1. Add `external_stubs: Option<Vec<String>>` to the `Settings` serde struct in `crates/graphify-cli/src/main.rs` (matches existing `exclude: Option<Vec<String>>` shape).
2. Propagate to MCP server's config as well (same duplication pattern as the CLI).
3. At the single call site (`crates/graphify-cli/src/main.rs:2484`), merge: `settings.external_stubs.iter().flatten().chain(project.external_stubs.iter()).cloned()`.
4. Documentation: update the `graphify init` stub template to suggest putting shared stubs at `[settings]` level.

## Test plan

- Unit: `ExternalStubs::new` already handles sorted-by-length dedup; verify concatenated input produces identical matching behaviour.
- Integration: simplify `graphify.toml` (move shared prelude to `[settings]`), confirm dogfood output is bit-identical to pre-change.

## Acceptance criteria

- `cargo test --workspace` green
- Dogfood `graphify.toml` can be written with shared prelude at `[settings]` level without regression in `ExpectedExternal` classification
- `cargo clippy --workspace -- -D warnings` clean
- Documentation example in `graphify init` template updated

## Out of scope

- Per-language default stub bundles (e.g. `[settings] rust_stubs = true` to auto-include std + prelude). More ambitious — separate ticket if FEAT-034 lands and the pattern grows.
- Stub globs (e.g. `serde*`). Current exact-prefix rule is fine for now.

## Discovered context

Discovered 2026-04-21 during FEAT-032 implementation. Filed as a follow-up because adding the settings layer would have doubled FEAT-032's scope (settings-struct plumbing + merge semantics + config-template docs) without addressing the critical `::` matcher bug. FEAT-032 shipped as a focused bug-fix for the matcher; FEAT-034 is the config-ergonomics polish.
