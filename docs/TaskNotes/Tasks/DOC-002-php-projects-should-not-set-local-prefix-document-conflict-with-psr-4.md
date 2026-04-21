---
uid: doc-002
status: done
priority: low
scheduled: 2026-04-21
completed: 2026-04-21
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- doc
- php
- config
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# PHP projects should not set local_prefix (document conflict with PSR-4)

For PHP projects, the PSR-4 autoload mapping in `composer.json` provides the namespace-prefix structure used as the module-id prefix (e.g. `App\Foo\Bar` → `App.Foo.Bar`). Setting `[[project]].local_prefix = "something"` in `graphify.toml` for a PHP project would either (a) be ignored by the PSR-4 branch of the resolver (case 7), which normalizes raw `use` targets via `\` → `.` and looks them up directly in `known_modules` without re-applying `local_prefix`, or (b) double-apply if a future refactor adds prefix handling there.

Surfaced in CHORE-007's resolver audit (2026-04-21) as the only "landmine" shape across all 10 branches. Not a bug today — PSR-4 and `local_prefix` don't collide in practice because PHP users don't typically set both. But nothing in the docs or `graphify init` template warns against it.

## Acceptance

- `CLAUDE.md` gets a one-liner in the PHP conventions section: "PHP projects should leave `local_prefix` unset; PSR-4 mappings from `composer.json` provide the namespace prefix structure."
- `crates/graphify-cli/src/main.rs` `graphify init` template's commented PHP example (if one exists) either omits `local_prefix` or comments it out with a "do not set for PHP" note.
- Optional: `load_config` emits a non-fatal stderr warning when a `[[project]]` has `lang = ["php"]` AND a non-empty `local_prefix`: `Warning: '<project>' sets local_prefix for a PHP project — PSR-4 mappings should be used instead. Consider removing local_prefix.`

## Out of scope

- Changing resolver case 7 to apply `local_prefix` (would be a breaking change for anyone who did set one — they'd get double-prefixed ids). If that ever becomes desirable, it's a FEAT with migration notes, not a DOC.

## Priority

Low — no user has reported this. Pure preventative documentation. ~15 min to land the CLAUDE.md line and the optional warning.
