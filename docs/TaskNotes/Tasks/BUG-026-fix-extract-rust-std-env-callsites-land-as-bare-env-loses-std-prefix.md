---
uid: bug-026
status: done
priority: normal
scheduled: 2026-04-27
completed: 2026-04-27
pomodoros: 0
tags:
- task
- bug
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: med
  hintsInferred: false
---

# fix(extract): Rust `std::env::*` callsites land as bare `env::` (loses `std` prefix)

`graphify suggest stubs` surfaces `env` as a 2-edge external candidate in `graphify-cli` despite all source callsites being fully qualified (`std::env::var_os`, `std::env::current_dir`, `std::env::current_exe`). The Rust extractor / resolver is dropping the `std::` prefix before reaching the `external_stubs` matcher, so the existing `std` stub never matches.

## Description

Surfaced post-FEAT-049 by the dogfood `graphify suggest stubs` run on this repo (2026-04-27). After adding `matches`/`toml_edit` as legitimate stubs, this is the only remaining stdlib-shape candidate. Source confirmation:

- `crates/graphify-cli/src/main.rs:3995` — `std::env::var_os("NO_COLOR")`
- `crates/graphify-cli/src/main.rs:5293` — `std::env::current_dir()`
- `crates/graphify-cli/src/main.rs:5318` — `std::env::current_exe()`

All three callsites carry the `std::` qualifier in source, yet `graphify suggest stubs` reports `env` as the prefix and `env` as the example, indicating the captured edge target is bare `env::*` (not `std::env::*`). The bug is somewhere between `extract_calls_recursive` (rust_lang.rs) and the post-resolution stub matcher.

## Investigation outcome (root cause was different)

Inspection of `report/graphify-cli/graph.json` (via `jq '.links[] | select(.target | test("env"))'`) showed the original hypothesis was **wrong**:

- The 3 `std::env::*` callsites at lines 3995, 5293, 5318 were ALREADY correctly classified as `ExpectedExternal` (confidence 0.5, kind `ExpectedExternal`) — the existing `std` stub matched them as `std::env::var_os`, `std::env::current_dir`, `std::env::current_exe` verbatim. No prefix-stripping bug exists in the extractor.
- The actual ambiguous `env` edges traced to 2 different callsites:
  - `crates/graphify-cli/src/main.rs:5333` — `env!("CARGO_PKG_VERSION")`
  - `crates/graphify-cli/src/session.rs:224` — `env!("CARGO_PKG_VERSION")`
- `env!` is a Rust stdlib macro. FEAT-031 strips trailing `!` from macro invocations (intentional, mirrors how `format!`, `println!`, `assert!`, `matches!`, `vec!` are handled — they all land as bare names against `external_stubs`). The bare `env` was missing from the stubs list, so it surfaced as a non-stubbed external candidate.

## Resolution

Config-only change: `env` added to `[settings].external_stubs` in `graphify.toml`, alongside the other stdlib macros (`format`, `println`, `eprintln`, `print`, `vec`, `write`, `writeln`, `assert*`, `panic`, `todo`, `unimplemented`, `unreachable`, `dbg`, `matches`, `include_str`).

No extractor change — the FEAT-031 macro-name-extraction is correct as designed. Per the CLAUDE.md self-dogfood rule, Rust macros are legitimate `external_stubs` candidates ("real workspace siblings, real third-party deps, **Rust macros**").

Self-dogfood: `graphify suggest stubs` candidate count 2 → 1. The remaining 1 candidate is `src.Community` (cross-crate `pub use graphify_core::community::Community;`), which is the FEAT-048 deferred gate signal (current count 1, threshold 5).

## Reproduction

```bash
graphify run --config graphify.toml --force
graphify suggest stubs --config graphify.toml | grep env
```

Expected: `env` not in candidate list (covered by existing `std` stub).
Actual: `env` listed under graphify-cli with 2 edges.

## Hypothesis

`scoped_identifier` extraction in `crates/graphify-extract/src/rust_lang.rs` likely walks the path nodes and joins segments, but may strip a leading segment when emitting the call target. Need to inspect what `extract_call_target` (or equivalent) emits for `std::env::current_dir()` AST node.

## Acceptance criteria

- `graphify suggest stubs --config graphify.toml` no longer lists `env` as a candidate after the fix
- Unit test in `crates/graphify-extract/src/rust_lang.rs` exercising `std::env::current_dir()` and asserting the captured Calls edge target retains `std::` (or, if the extractor canonically drops it, the resolver applies the `std` stub correctly to bare `env::*` — either resolution shape is acceptable, but the bug surfaces because both layers fail today)
- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass
- No regression in `graphify check --config graphify.toml`

## Out of scope

- Other stdlib namespace gaps (e.g., bare `fs::*`, `path::*`) — file separately if surfaced by future `suggest stubs` runs
- Generalized "stdlib heuristic" matcher in `external_stubs` — a fix at the extractor or resolver layer is preferred over silencing via stub config

## Related

- Surfaced from FEAT-043 self-dogfood (CLAUDE.md "Self-dogfood UX rule" — fix the root, don't paper over with stubs)
- BUG-024 (closures) note explicitly anticipated this case ("`std::env` bare references (~2 edges) are different fix shapes (macro recognizer / stdlib heuristic)")

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
