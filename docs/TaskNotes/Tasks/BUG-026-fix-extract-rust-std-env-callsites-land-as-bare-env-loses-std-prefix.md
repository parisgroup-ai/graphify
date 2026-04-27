---
uid: bug-026
status: open
priority: normal
scheduled: 2026-04-27
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
