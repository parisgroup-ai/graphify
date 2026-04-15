# Issue #3 — Restrict `Calls` edges to imported callees

**Status:** Design approved — ready for implementation plan
**Scope:** `graphify-extract` (TypeScript + Python); FEAT-019 plan update for PHP (Task 8)
**Tracking:** https://github.com/parisgroup-ai/graphify/issues/3
**Related:** Issue #2 (report-layer confidence surfacing — separate follow-up)

## Summary

The TypeScript and Python extractors emit a `Calls` edge for every bare-identifier call site, using the raw callee identifier as the target node id at confidence `0.7 / Inferred`. This creates phantom nodes (same-file helpers, function parameters), duplicate nodes (bare names alongside their qualified counterparts), and pollutes ranking metrics with JS globals and Python builtins.

This spec restricts `Calls` edges to call sites whose callee is bound by a top-level import in the same file. Intra-module helpers, language globals, and builtins stop producing edges. The FEAT-019 PHP plan (Task 8) adopts the same pattern from the start.

## Evidence

From `parisgroup-ai/infra/packages/infra-core` on graphify 0.7.0 (post-#1):

- 9 of 89 nodes (~10%) are bare-name duplicates of qualified symbols.
- `ambiguous_pct = 98.75%`, `mean_confidence = 0.506`.
- Top-10 hotspots include `fn` (callback parameter), `stripHtml` (duplicate), `String` (JS global), `setTimeout` (JS global).

Full repro and edge dump: issue #3 body.

## Design

### Core algorithm

During `extract_file`, each extractor builds a file-local import set:

```rust
let imported: HashSet<String> = collect_imported_bindings(root, source);
```

When `extract_calls_recursive` encounters a bare-identifier call, it emits the `Calls` edge only if the callee name is in `imported`.

**Edge target:** the local alias the file actually uses. `import { foo as bar } from './x'; bar()` emits a `Calls` edge targeting `bar`, not `foo`. The existing `Imports` edge already carries the alias→module mapping, so traceability is preserved without encoding it twice.

**Confidence:** `0.9 / Extracted` (up from `0.7 / Inferred`). The symbol is a known import, not a guess — matching the resolver's confidence for Python relative and TS alias resolution.

**Lifetime:** file-local, lives only for one `extract_file` call. No cross-file state, no threading concerns — each file already runs its own parser.

**"Top-level" defined:** module-scope / file-scope statements only. Python imports inside functions and TS `require()` calls inside function bodies do not contribute to the binding set. Nested calls to those bindings produce no edge. This matches the existing extractor, which already only walks top-level statements for imports.

### TypeScript — bindings collected

| Import form | Binding(s) added |
|---|---|
| `import foo from 'x'` | `foo` |
| `import { a, b as c } from 'x'` | `a`, `c` |
| `import * as ns from 'x'` | `ns` |
| `import 'x'` (side-effect) | nothing |
| `const foo = require('x')` | `foo` |
| `const { a, b: c } = require('x')` | `a`, `c` |
| `import type { T } from 'x'` | `T` |
| `export { foo } from 'x'` (re-export) | nothing |

Type-only imports remain in the set: they represent a real structural dependency even though they erase at runtime. Re-exports do not bind a local name for use inside the file.

### Python — bindings collected

| Import form | Binding(s) added |
|---|---|
| `import foo` | `foo` |
| `import foo.bar` | `foo` |
| `import foo as f` | `f` |
| `from foo import bar` | `bar` |
| `from foo import bar as b` | `b` |
| `from foo import *` | nothing |

Star imports are intentionally unsupported — resolving them would require cross-file analysis and emit edges on a guess. Python builtins (`print`, `len`, `range`) are implicitly excluded because they never appear in the import set.

## Behavior changes

### Edges dropped (intentional)

- Same-file helper calls (`sleep()` defined and called in `retry.ts`).
- Function-parameter invocations (`fn()` where `fn` is a callback).
- JS globals (`setTimeout`, `String`, `Array`, `console`, `Promise`).
- Python builtins (`print`, `len`, `range`, `isinstance`).
- PHP global functions (`count`, `array_map`) and namespaced bare calls (`\foo()`) — applies when FEAT-019 Task 8 lands.

### Edges preserved

- Every call to an imported symbol (direct, aliased, namespaced, type-only).
- `Imports`, `Defines`, `Re-export` edges — unchanged.
- Member calls (`obj.method()`) — already skipped; unchanged.

## Tests

### New tests (mirrored per language)

1. Imported callee emits `Calls` edge with confidence `0.9 / Extracted`.
2. Same-file helper call produces no `Calls` edge.
3. JS global / Python builtin produces no `Calls` edge.
4. Aliased import's callee is keyed by the local alias (not the imported symbol).
5. TS: re-export (`export { foo } from 'x'`) does not create a binding.
6. Python: star import (`from foo import *`) does not create bindings.

### Tests to update or delete

Any existing test in `typescript.rs` or `python.rs` asserting a bare-name `Calls` edge for a same-file helper is rewritten or removed. Grep `extract_calls_recursive` and test modules for assertions against unqualified callees.

### Fixture regeneration

Goldens under `tests/fixtures/**/graph.json` and `tests/fixtures/**/analysis.json` are regenerated in the same PR. Diffs are reviewed for expected node and edge drops — unexpected losses indicate a binding the extractor missed.

## Acceptance criteria

Run the fixed extractor against `parisgroup-ai/infra/packages/infra-core`:

- Node count: 89 → ~80 (bare-name duplicates gone).
- `ambiguous_pct`: 98.75% → <40%.
- Top-10 hotspots contain no phantom nodes (`fn`, `String`, `setTimeout`, `stripHtml`).

Before/after counts captured in the PR description.

## Non-goals

- Report-layer confidence banner and per-hotspot confidence column (issue #2) — separate PR scoped to `graphify-report`.
- Cross-file symbol-table resolution (option (b) in the issue) — larger work, deferred.
- PHP extractor implementation — this spec only updates the FEAT-019 plan document's Task 8. PHP code lands on `feat-019-php-support` after it rebases onto the new `main`.

## Sequencing

1. **On `feat-019-php-support`:** commit the in-flight Task 4–5 progress (`crates/graphify-extract/src/php.rs`, `crates/graphify-extract/src/walker.rs`). Exclude `.obsidian/workspace.json` and deleted `target/*` artifacts.
2. **Branch `fix/issue-3-imported-callees` off `main`:** implement the TypeScript fix, then Python. Regenerate fixtures. Merge to `main`.
3. **Rebase `feat-019-php-support` onto new `main`.** Rewrite FEAT-019 plan Task 8 to the imported-bindings template. Continue PHP implementation.

## Files touched (this PR)

- `crates/graphify-extract/src/typescript.rs` — binding collection during import walk, filter in `extract_calls_recursive`, new tests.
- `crates/graphify-extract/src/python.rs` — same.
- `tests/fixtures/**/graph.json`, `tests/fixtures/**/analysis.json` — regenerated goldens.

## Files touched (follow-up, on feat-019 branch)

- `docs/superpowers/plans/2026-04-15-feat-019-php-support.md` — Task 8 rewrite using the imported-bindings pattern.
