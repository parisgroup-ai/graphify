# FEAT-019 — PHP Language Support

**Status:** Design approved (2026-04-15)
**Task:** FEAT-019 (to be created in TaskNotes at plan time)
**Related:** FEAT-008 (confidence scoring), the existing Go/Rust extractors (pattern reference)

---

## 1. Scope

Add PHP as a first-class supported language in Graphify, at **parity level with Go and Rust**: module/namespace, use/import, class/interface/trait/enum/function defines, bare function calls. The goal is that a typical Laravel/Symfony codebase — organized around PSR-4 autoloading — yields a useful dependency graph where inter-namespace `use` statements resolve to local modules.

### In scope (v1)

- New `Language::Php` variant in `graphify-core`
- New `PhpExtractor` in `graphify-extract/src/php.rs` implementing `LanguageExtractor`
- File discovery: `.php` extension, `*Test.php` test-file exclusion (PHPUnit convention), `vendor/` already excluded by `DEFAULT_EXCLUDES`
- PSR-4 autoload resolution via `composer.json` (analogous to `tsconfig.json` path aliases and `go.mod` module paths)
- Namespace → dot-notation conversion (`App\Services\Llm` → `App.Services.Llm`)
- `use` declarations (simple, aliased, group, `use function`, `use const`) emit `Imports` + `Calls` edges
- Class / interface / trait / enum / function declarations emit `Defines` edges with appropriate `NodeKind`
- Bare function calls (`foo()`) emit `Calls` edges with confidence 0.7 / `Inferred` — matching Python/Go policy
- Method declarations inside classes emit `Method` nodes with id `{module}.{ClassName}.{method}`
- CLI config: `lang = ["php"]` accepted in `graphify.toml`
- Watch mode: `.php` file changes trigger per-project rebuild
- Fixture: `tests/fixtures/php_project/` with composer.json + typical Laravel-style layout
- Updated `CLAUDE.md` (Architecture table + Conventions section)
- Default config example (`graphify init`) includes `php` in the allowed-languages comment

### Out of scope (v1) — rejected alternatives

| Rejected | Reason |
|---|---|
| `extends` / `implements` edges | Level B in brainstorm; deferred to keep parity with existing extractors (Go/Rust don't emit type-hierarchy edges either) |
| Resolution of `Class::method()` and `$obj->method()` | Level C; requires type-hint inference, doesn't fit the ~500-line-per-extractor pattern |
| Reading `namespace X;` declaration from source as module-name authority | Breaks the invariant that walker computes `module_name` before extractor runs. PSR-4 via `composer.json` gives the same result and follows the TS/Go precedent |
| Supporting non-PSR-4 legacy autoloaders (PSR-0, classmap, files) | Vanishingly rare in modern PHP; fallback to path-based module names covers the degraded case |
| `.phtml`, `.inc` extensions | Uncommon in modern PHP; can be opt-in extensions later |
| Attribute-based dependency tracking (`#[Route(...)]`, `#[Autowired]`) | Framework-specific; belongs in a separate FEAT |
| Trait imports inside a class body (`use TraitName;` in class scope) | Lexical collision with namespace `use`; tree-sitter distinguishes them, but tracking trait composition is Level B |

---

## 2. Architecture overview

### 2.1 Insertion points

8 files touched. Only `php.rs` is substantially new; everything else is 1–40 lines of routing.

| File | Change | Est. size |
|---|---|---|
| `crates/graphify-core/src/types.rs` | Add `Language::Php` variant + serde round-trip test | +3 lines + 1 test |
| `crates/graphify-extract/Cargo.toml` | Add `tree-sitter-php = "0.23"` dep | +1 line |
| `crates/graphify-extract/src/php.rs` | **New** `PhpExtractor` + unit tests | ~500 lines |
| `crates/graphify-extract/src/lib.rs` | `pub mod php;` + re-export `PhpExtractor` | +2 lines |
| `crates/graphify-extract/src/walker.rs` | `language_for_extension("php")`, `is_test_file` PHP patterns, PSR-4 translation hook in `path_to_module` | ~40 lines + tests |
| `crates/graphify-extract/src/resolver.rs` | `load_composer_json()` + PHP `use` resolution branch in `resolve()` | ~120 lines + tests |
| `crates/graphify-cli/src/main.rs` | Register `PhpExtractor`, `parse_languages` accepts `"php"`, load `composer.json` when PHP in langs, update default-config comment | ~15 lines |
| `crates/graphify-cli/src/watch.rs` | `"php" → vec!["php"]` extension mapping | +1 line + test |

### 2.2 Data flow (unchanged shape, new language branch)

```
graphify.toml (project with lang=["php"])
    ↓
Walker::discover_files()
    ├── detects *.php, skips *Test.php / vendor/
    └── path_to_module():
        ├── if project has composer.json & PSR-4 mapping matches path → apply PSR-4 translation
        └── else → fallback to existing path-based naming (prepends local_prefix)
    ↓
For each DiscoveredFile (language=Php):
    PhpExtractor::extract_file(path, source, module_name)
        └── tree-sitter-php parse → walk root → per-node dispatch
    ↓
ModuleResolver::resolve(raw, from_module, is_package)
    └── PHP `use X\Y\Z`: convert `\` to `.`, match against known_modules
    ↓
(rest of pipeline unchanged: CodeGraph → metrics → reports)
```

---

## 3. `PhpExtractor` — tree-sitter grammar mapping

Follows the `GoExtractor` shape (cleanest existing reference). One `Parser` built per `extract_file` call (tree-sitter `Parser` is not `Send`). Module node emitted unconditionally.

### 3.1 Node-kind dispatch table

| tree-sitter node | Action | Edge kind | Node kind | Confidence |
|---|---|---|---|---|
| `namespace_definition` | Record declared namespace (metadata only in v1; walker already provides authoritative `module_name`) | — | — | — |
| `namespace_use_declaration` → simple | Emit `Imports(module)` + `Calls(module.symbol)` | `Imports`, `Calls` | — | 1.0 / Extracted |
| `namespace_use_declaration` → aliased (`use X\Y as Z`) | Same as simple; alias is ignored (target is `X\Y`) | `Imports`, `Calls` | — | 1.0 / Extracted |
| `namespace_use_declaration` → group (`use X\{A, B}`) | Expand to N individual imports, one `Imports` per symbol-carrying module | `Imports`, `Calls` | — | 1.0 / Extracted |
| `use function X\Y\foo` / `use const X\Y\BAR` | Same as simple `use` | `Imports`, `Calls` | — | 1.0 / Extracted |
| `class_declaration` | Emit symbol node + `Defines` edge; recurse body for `method_declaration` + bare calls | `Defines` | `Class` | 1.0 / Extracted |
| `interface_declaration` | Emit symbol + `Defines`; recurse body | `Defines` | `Trait` (reuse variant, same as Go `interface`) | 1.0 / Extracted |
| `trait_declaration` | Emit symbol + `Defines`; recurse body | `Defines` | `Trait` | 1.0 / Extracted |
| `enum_declaration` (PHP 8.1+) | Emit symbol + `Defines`; recurse body | `Defines` | `Enum` | 1.0 / Extracted |
| `function_definition` (top-level) | Emit symbol + `Defines`; recurse body for bare calls | `Defines` | `Function` | 1.0 / Extracted |
| `method_declaration` (inside class/trait/enum) | Emit symbol with id `{module}.{ClassName}.{method}` + `Defines` edge **from module**; recurse body | `Defines` | `Method` | 1.0 / Extracted |
| `function_call_expression` where callee is bare `name` | Emit `Calls` from current module | `Calls` | — | 0.7 / Inferred |
| `scoped_call_expression` (`A::b()`), `member_call_expression` (`$a->b()`) | **Skip** (same policy as Go selector, Python attribute) | — | — | — |
| Other top-level statements | Recurse for bare-call extraction (covers config-style files) | — | — | — |

### 3.2 Symbol-id scheme

- **Namespace `App\Services` + class `Llm`** in file mapped via PSR-4 to module `App.Services.Llm` → class node id: `App.Services.Llm.Llm`
  - Rationale: matches Go, where class `Handler` in module `pkg.main` is `pkg.main.Handler`. Keeps `module.symbol` invariant everywhere.
- **Method `call` inside class `Llm`** → `App.Services.Llm.Llm.call`
- **Top-level function `format`** in file mapped to `App.Helpers` → function id: `App.Helpers.format`

### 3.3 Delegation of recursion

The extractor uses one recursive helper `extract_calls_recursive(node, source, module_name, result)` identical in behavior to Go/Python. Called from: body of every class/interface/trait/enum/function/method, and from any top-level statement not matched by a more specific handler.

---

## 4. Module resolution — PSR-4 via `composer.json`

### 4.1 `composer.json` parsing

New method on `ModuleResolver`:

```rust
/// Parse `composer.json` and load `autoload.psr-4` + `autoload-dev.psr-4`
/// mappings as `(namespace_prefix, dir_prefix)` pairs.
pub fn load_composer_json(&mut self, composer_path: &Path)
```

Parser strategy: hand-written, mirrors `load_tsconfig`. Locates `"psr-4"` blocks inside `"autoload"` and `"autoload-dev"`, extracts key→string-value pairs (first value wins when array is provided). No `serde_json` runtime dep introduced — keeps the resolver dependency surface unchanged.

**Storage:** new field `psr4_mappings: Vec<(String, String)>` on `ModuleResolver`. Populated as `("App\\", "src/")`, `("Tests\\", "tests/")`, etc. Backslashes in the namespace prefix are preserved as escaped (`\\`) during parsing and normalized to empty trailing-separator at lookup time (see 4.2).

### 4.2 Walker-side: path → namespace translation

Extend `path_to_module()` with an optional `psr4_mappings` parameter threaded from the CLI. Translation rule:

1. Compute the path relative to `base` (same as today).
2. For each `(namespace_prefix, dir_prefix)` pair, check if `rel.starts_with(dir_prefix)`.
   - If multiple match, prefer the longest `dir_prefix` (most specific).
3. On match: strip `dir_prefix`, prepend `namespace_prefix`, then normalize:
   - Remove trailing `\` from namespace prefix (PSR-4 convention has a trailing separator)
   - Replace `\` → `.` and `/` → `.`
   - Strip `.php` extension
4. On no match: fallback to existing behavior (path components joined by `.`, optional `local_prefix` prepend).

**Non-PHP languages are unaffected** — mappings vector is empty for projects whose languages don't include PHP, and the translation step is a no-op when the vector is empty.

**Example walk:**
```
composer.json:   "autoload": { "psr-4": { "App\\": "src/" } }
file:            src/Services/Llm.php
dir_prefix:      "src/"     (matches, length 4)
namespace_prefix:"App\\"
strip + prepend: App\Services\Llm.php
normalize:       App.Services.Llm
→ module_name = "App.Services.Llm"
```

### 4.3 Resolver-side: `use X\Y\Z` → canonical id

Add a branch in `ModuleResolver::resolve()` that activates when the raw import contains `\` (a PHP namespace separator). Behavior:

1. Normalize: trim leading `\` (absolute namespace marker), replace `\` with `.`.
2. Look up in `known_modules`; if present, return `(resolved, is_local=true, confidence=1.0)`.
3. Otherwise return `(resolved, is_local=false, confidence=1.0)`. (Same policy as a direct Python module name — the namespace syntax itself is unambiguous.)

**Confidence rationale:** PHP namespaces are fully qualified at the `use` site (unlike Python relative imports). No heuristics needed — 1.0 / `Extracted`.

### 4.4 CLI wiring

In `cmd_extract` (or equivalent), after building the resolver and before the extraction pass:

```rust
if languages.contains(&Language::Php) {
    let composer = repo_path.join("composer.json");
    if composer.exists() {
        resolver.load_composer_json(&composer);
    }
}
```

Match the existing `tsconfig.json` / `go.mod` hook position in `main.rs:1452–1465`.

### 4.5 Failure modes

| Condition | Behavior |
|---|---|
| Project has no `composer.json` | Fallback to path-based module names. Warning on stderr: `"PHP project without composer.json — PSR-4 resolution disabled, imports may not resolve to local modules"` |
| `composer.json` exists but malformed | Resolver logs stderr warning, proceeds with empty `psr4_mappings`. Does not abort. |
| File path doesn't match any PSR-4 `dir_prefix` | Fallback to path-based. No warning (common for root-level scripts) |
| Two PSR-4 mappings match the same path | Longest-prefix wins. Documented in `CLAUDE.md` Conventions |

---

## 5. File discovery

### 5.1 Extension + test-file patterns (walker changes)

In `language_for_extension`:
```rust
"php" => Some(Language::Php),
```

In `is_test_file`, add PHPUnit convention:
```rust
// PHPUnit: <ClassName>Test.php
if file_name.ends_with("Test.php") {
    return true;
}
```

Note: no `_test.php` lowercase variant (not PHPUnit-standard). `tests/` directory is already excluded via `DEFAULT_EXCLUDES`. `vendor/` too — no changes needed.

### 5.2 Package entry-point flag (`is_package`)

PHP has no `__init__.py` / `index.ts` / `mod.rs` equivalent. `is_package` is always `false` for PHP files. No changes to `DiscoveredFile` struct.

---

## 6. CLI + Watch changes

### 6.1 `parse_languages` (`main.rs:1342-1345`)

```rust
"php" => Some(Language::Php),
```

### 6.2 Extractor registration (`main.rs:1440-1502`)

Add alongside the others:
```rust
let php_extractor = PhpExtractor::new();
...
let extractor: &dyn LanguageExtractor = match file.language {
    Language::Python => &python_extractor,
    Language::TypeScript => &typescript_extractor,
    Language::Go => &go_extractor,
    Language::Rust => &rust_extractor,
    Language::Php => &php_extractor,
};
```

### 6.3 Default config comment (`main.rs:987`)

```rust
lang = ["python"]           # Options: python, typescript, go, rust, php
```

### 6.4 Watch mode (`watch.rs:20-23`)

```rust
"php" => vec!["php".to_string()],
```

---

## 7. Testing strategy

### 7.1 Unit tests — `php.rs` (target ≥20 tests)

Mirror `go.rs` test suite + PHP-specific coverage:
- `extensions_returns_php`
- `module_node_always_created`
- `namespace_definition_captured` (metadata-only assertion)
- `simple_use_produces_imports_and_calls_edges`
- `aliased_use_ignores_alias`
- `group_use_expands_to_multiple_edges`
- `use_function_produces_calls_edge`
- `use_const_produces_calls_edge`
- `class_declaration_creates_class_node_and_defines`
- `interface_declaration_creates_trait_node`
- `trait_declaration_creates_trait_node`
- `enum_declaration_creates_enum_node`
- `function_definition_creates_function_node`
- `method_declaration_inside_class_creates_method_node`
- `method_id_includes_class_name`
- `bare_call_inside_method_produces_calls_edge`
- `static_call_not_extracted_as_bare` (`A::b()`)
- `instance_method_call_not_extracted_as_bare` (`$a->b()`)
- `import_confidence_is_extracted_1_0`
- `bare_call_confidence_is_inferred_0_7`
- `full_php_file_integration` (namespace + use + class + method + calls)

### 7.2 Unit tests — `walker.rs` PSR-4

- `path_to_module_php_with_psr4_mapping`
- `path_to_module_php_without_composer_falls_back`
- `path_to_module_php_longest_prefix_wins` (two overlapping mappings)
- `is_test_file_php_phpunit_pattern` (`UserTest.php` → true, `User.php` → false)
- `discover_php_files_excludes_phpunit_tests`

### 7.3 Unit tests — `resolver.rs` PSR-4

- `load_composer_json_parses_psr4_mappings`
- `load_composer_json_parses_autoload_dev`
- `load_composer_json_handles_malformed_without_panic`
- `resolve_php_use_matches_known_module`
- `resolve_php_use_nonlocal_returns_extracted_confidence`
- `resolve_php_use_strips_leading_backslash`

### 7.4 Integration — new fixture `tests/fixtures/php_project/`

```
tests/fixtures/php_project/
├── composer.json              # autoload.psr-4: { "App\\": "src/" }
├── src/
│   ├── Main.php               # namespace App; use App\Services\Llm; call_llm()
│   ├── Services/
│   │   └── Llm.php            # namespace App\Services; class Llm { public function call() {} }
│   ├── Models/
│   │   └── User.php           # namespace App\Models; class User {}
│   └── Controllers/
│       └── HomeController.php # namespace App\Controllers; use App\Services\Llm; use App\Models\User;
└── tests/
    └── LlmTest.php            # MUST be excluded from discovery
```

Integration tests (in `graphify-extract/tests/php_fixture.rs` — new file):
- `discover_php_fixture_finds_four_source_files` (excludes `LlmTest.php`)
- `discover_php_fixture_correct_module_names` (PSR-4 applied)
- `home_controller_imports_resolve_to_local_modules` (cross-file resolution)

### 7.5 End-to-end CLI test

Add a test in `graphify-cli/tests/` (or extend existing) that runs `graphify run --config` against a generated config pointing at the PHP fixture, then asserts `analysis.json` contains expected nodes + at least one local `Imports` edge (`App.Controllers.HomeController → App.Services.Llm`).

---

## 8. Documentation updates

### 8.1 `CLAUDE.md` — "What is Graphify"

Update:
> Graphify is a Rust CLI tool for architectural analysis of codebases. It extracts dependencies from Python, TypeScript, Go, Rust, and **PHP** source code using tree-sitter AST parsing...

### 8.2 `CLAUDE.md` — "Architecture → Key modules" table

Add row:
| `crates/graphify-extract/src/php.rs` | PHP extractor (namespace, use, class/interface/trait/enum/function, calls) |

### 8.3 `CLAUDE.md` — "Conventions" section

Add bullets:
- PHP PSR-4 mapping loaded from `composer.json` (`autoload.psr-4` + `autoload-dev.psr-4`); longest-prefix match wins; namespaces normalized `\` → `.`
- PHP test files excluded: `*Test.php` (PHPUnit convention)
- PHP confidence: `use X\Y\Z` → 1.0 / Extracted (fully qualified); bare calls 0.7 / Inferred (same as Go/Python)
- PHP method id scheme: `{module}.{ClassName}.{method}`

### 8.4 README.md — language support list (if present)

Add PHP to the supported-languages list and mention PSR-4 autoload handling.

---

## 9. Verification plan

Before marking FEAT-019 done:

1. `cargo fmt --all -- --check` — pass
2. `cargo clippy --workspace -- -D warnings` — pass
3. `cargo test --workspace` — all tests pass, includes ≥20 new PHP tests
4. Manual smoke test: run `graphify run` against a real Laravel/Symfony project (e.g. one from `~/ai/`), verify `analysis.json` shows sane namespace structure and non-zero local imports
5. Verify graph metrics (betweenness, communities, cycles) execute without error on PHP-only output
6. Verify HTML + Obsidian + Neo4j report generators render PHP nodes without special-casing (should "just work" — all reports are language-agnostic)
7. Update `CLAUDE.md`, commit together with the code

---

## 10. Open questions / risks

| Question | Resolution |
|---|---|
| `tree-sitter-php` version compatibility with `tree-sitter = "0.25"` | Validate at plan time. If 0.23 is incompatible, pin to 0.22 or wait for a compat release. Fallback: skip PHP from the build and emit a clear error |
| Namespace declarations across multiple files in same package | Not possible in PHP — each file has one namespace. Simpler than Python's `__init__.py` packaging |
| Case sensitivity | PHP class/interface/trait names are case-insensitive at runtime but case-sensitive in source files. Tree-sitter preserves source case; graph uses source case (same as every other language) |
| Dynamic dispatch (runtime code evaluation, `new $class()`, variable function calls `$fn()`) | Level C territory; out of scope. Mentioned in CLAUDE.md limitations |
| Projects that use Composer classmap instead of PSR-4 | Rare in modern code. Fallback to path-based module names degrades gracefully — imports won't resolve to `is_local` but the graph still has structure |
