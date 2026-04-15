# FEAT-019 PHP Language Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add PHP as a first-class language in Graphify at parity with Go/Rust — PhpExtractor via tree-sitter, PSR-4 autoload resolution via `composer.json`, PHPUnit test-file exclusion, and CLI/watch wiring. A typical Laravel/Symfony codebase should yield a useful graph where inter-namespace `use` statements resolve to local modules.

**Architecture:** New `Language::Php` variant in `graphify-core`. New `PhpExtractor` (`crates/graphify-extract/src/php.rs`) implementing `LanguageExtractor`, mirroring `GoExtractor` shape. New `ModuleResolver::load_composer_json()` that parses PSR-4 autoload mappings with a hand-written parser (no new deps). Walker gains PSR-4 path translation so `src/Services/Llm.php` maps to module `App.Services.Llm` when `composer.json` declares `"App\\": "src/"`. CLI registers the extractor and loads `composer.json` when PHP appears in the project's `lang` list. Watch mode maps `.php` changes to affected projects.

**Tech Stack:** Rust 2021, tree-sitter 0.25 + tree-sitter-php 0.23, petgraph, rayon, serde (all already in the workspace except `tree-sitter-php`).

**Spec:** `docs/superpowers/specs/2026-04-15-feat-019-php-support-design.md`

---

## Pre-flight

From the repo root:

```bash
cargo test --workspace           # expect: all green baseline
cargo build --release -p graphify-cli   # build CLI for later end-to-end test
git status --short               # expect: clean-ish; ignore target/ artifacts
```

If the baseline is not green, **stop** and fix before proceeding.

Confirm `tree-sitter-php = "0.23"` is compatible with `tree-sitter = "0.25"` (our workspace version). At the time of writing, `tree-sitter-php 0.23.x` is the version published that targets the `tree-sitter 0.25` ABI. If a tree-sitter compile error appears in Task 1, check crates.io for the latest `tree-sitter-php` compatible with `tree-sitter = 0.25` and adjust the Cargo.toml pin in Task 1.

---

## Task 1: Add `Language::Php` variant

**Context:** The central `Language` enum in `graphify-core` is the discriminator routed through walker → extractor → resolver → reports. Adding the variant is a prerequisite for every other task.

**Files:**
- Modify: `crates/graphify-core/src/types.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests { ... }` block in `crates/graphify-core/src/types.rs` (after the existing language serialization tests, around line 372-382):

```rust
#[test]
fn language_php_serialization() {
    let php_json = serde_json::to_string(&Language::Php).expect("serialize");
    assert_eq!(php_json, "\"Php\"");
    let php_back: Language = serde_json::from_str(&php_json).expect("deserialize");
    assert_eq!(php_back, Language::Php);
}

#[test]
fn create_php_module_node() {
    let node = Node::module(
        "App.Services.Llm",
        "src/Services/Llm.php",
        Language::Php,
        1,
        true,
    );
    assert_eq!(node.language, Language::Php);
    assert_eq!(node.kind, NodeKind::Module);
}
```

- [ ] **Step 2: Run the test — confirm it fails**

```bash
cargo test -p graphify-core language_php_serialization
```

Expected: compile error — `no variant or associated item named 'Php' found for enum 'Language'`.

- [ ] **Step 3: Add the variant**

In `crates/graphify-core/src/types.rs`, update the `Language` enum (around line 9-14):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Python,
    TypeScript,
    Go,
    Rust,
    Php,
}
```

- [ ] **Step 4: Run the test — confirm it passes**

```bash
cargo test -p graphify-core language_php_serialization create_php_module_node
```

Expected: `2 passed`.

- [ ] **Step 5: Run the full core crate tests**

```bash
cargo test -p graphify-core
```

Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-core/src/types.rs
git commit -m "feat(core): add Language::Php variant"
```

---

## Task 2: Add `tree-sitter-php` dependency

**Context:** The PhpExtractor needs the tree-sitter-php grammar crate. Added before writing any extractor code so the crate compiles.

**Files:**
- Modify: `crates/graphify-extract/Cargo.toml`

- [ ] **Step 1: Add the dependency**

In `crates/graphify-extract/Cargo.toml`, add below the existing tree-sitter lines (around line 9-12):

```toml
tree-sitter-php = "0.23"
```

The `[dependencies]` block should end up looking like:

```toml
[dependencies]
graphify-core = { path = "../graphify-core" }
tree-sitter = "0.25"
tree-sitter-python = "0.25"
tree-sitter-typescript = "0.23"
tree-sitter-go = "0.25"
tree-sitter-rust = "0.24"
tree-sitter-php = "0.23"
rayon = "1"
sha2 = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Verify the dep resolves and compiles**

```bash
cargo build -p graphify-extract
```

Expected: builds without errors. If the build fails with an ABI mismatch against `tree-sitter = 0.25`, try `tree-sitter-php = "0.22"` or the latest published compatible version, then re-run.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-extract/Cargo.toml Cargo.lock
git commit -m "chore(extract): add tree-sitter-php dependency"
```

---

## Task 3: Walker — extension detection and PHPUnit test file exclusion

**Context:** The walker is where file discovery happens. Before the extractor exists, we need the walker to emit `.php` files tagged as `Language::Php` and to exclude PHPUnit `*Test.php` test files.

**Files:**
- Modify: `crates/graphify-extract/src/walker.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests { ... }` block in `crates/graphify-extract/src/walker.rs` (at the end of the existing `tests` module, after the Rust section):

```rust
// -----------------------------------------------------------------------
// PHP support
// -----------------------------------------------------------------------

#[test]
fn path_to_module_php_regular_file() {
    // Without PSR-4 mapping (added in a later task), path-based fallback
    // applies: src/Services/Llm.php → "src.Services.Llm"
    let base = Path::new("/repo");
    let file = Path::new("/repo/src/Services/Llm.php");
    assert_eq!(path_to_module(base, file, ""), "src.Services.Llm");
}

#[test]
fn is_test_file_php_phpunit_pattern() {
    assert!(is_test_file("UserTest.php"));
    assert!(is_test_file("LlmTest.php"));
    assert!(!is_test_file("User.php"));
    assert!(!is_test_file("TestingHelper.php")); // "Testing" prefix is not a test
    // Note: bare "Test.php" also matches `ends_with("Test.php")` and is treated
    // as a test. In practice, production files are never literally named Test.php,
    // so the false-positive is acceptable and not asserted here.
}

#[test]
fn discover_php_files_and_exclude_phpunit_tests() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("Service.php"), b"<?php\nclass Service {}").unwrap();
    std::fs::write(src.join("ServiceTest.php"), b"<?php\nclass ServiceTest {}").unwrap();

    let files = discover_files(tmp.path(), &[Language::Php], "", &[]);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].module_name, "src.Service");
    assert_eq!(files[0].language, Language::Php);
    assert!(!files[0].is_package, "PHP files are never packages");
}
```

The `ends_with("Test.php")` implementation (next step) will match bare `Test.php` as well as `UserTest.php`. We treat this as acceptable — production files are never literally named `Test.php`.

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-extract walker::tests::path_to_module_php_regular_file
cargo test -p graphify-extract walker::tests::is_test_file_php_phpunit_pattern
cargo test -p graphify-extract walker::tests::discover_php_files_and_exclude_phpunit_tests
```

Expected: `path_to_module_php_regular_file` **may pass** (path-based fallback already works for arbitrary extensions). The other two fail because `"php"` is not mapped and `"Test.php"` is not recognized.

- [ ] **Step 3: Add PHP to `language_for_extension`**

In `crates/graphify-extract/src/walker.rs`, update `language_for_extension` (around lines 71-79):

```rust
fn language_for_extension(ext: &str) -> Option<Language> {
    match ext {
        "py" => Some(Language::Python),
        "ts" | "tsx" => Some(Language::TypeScript),
        "go" => Some(Language::Go),
        "rs" => Some(Language::Rust),
        "php" => Some(Language::Php),
        _ => None,
    }
}
```

- [ ] **Step 4: Add PHPUnit pattern to `is_test_file`**

In `crates/graphify-extract/src/walker.rs`, extend `is_test_file` (around lines 37-66). Add before the final `false`:

```rust
// PHP / PHPUnit convention: <ClassName>Test.php
if file_name.ends_with("Test.php") {
    return true;
}
```

So the full `is_test_file` ends with:

```rust
    // Go conventions: *_test.go
    if file_name.ends_with("_test.go") {
        return true;
    }

    // PHP / PHPUnit convention: <ClassName>Test.php
    if file_name.ends_with("Test.php") {
        return true;
    }

    false
}
```

- [ ] **Step 5: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract walker::tests::path_to_module_php
cargo test -p graphify-extract walker::tests::is_test_file_php
cargo test -p graphify-extract walker::tests::discover_php_files_and_exclude_phpunit_tests
```

Expected: all three pass.

- [ ] **Step 6: Run the full walker tests to confirm no regression**

```bash
cargo test -p graphify-extract walker
```

Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-extract/src/walker.rs
git commit -m "feat(walker): detect .php files and exclude *Test.php"
```

---

## Task 4: `PhpExtractor` skeleton — module node only

**Context:** Create the new file with the struct, trait impl, and minimal `extract_file` that emits exactly one module node. Every subsequent task will add one node/edge kind to it.

**Files:**
- Create: `crates/graphify-extract/src/php.rs`
- Modify: `crates/graphify-extract/src/lib.rs`

- [ ] **Step 1: Create `php.rs` with skeleton + failing test**

Create `crates/graphify-extract/src/php.rs`:

```rust
use crate::lang::{ExtractionResult, LanguageExtractor};
use graphify_core::types::{Language, Node};
use std::path::Path;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// PhpExtractor
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct PhpExtractor;

impl PhpExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageExtractor for PhpExtractor {
    fn extensions(&self) -> &[&str] {
        &["php"]
    }

    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("Failed to load PHP grammar");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();

        // Every file gets a module node.
        result
            .nodes
            .push(Node::module(module_name, path, Language::Php, 1, true));

        // NOTE: top-level dispatch will be added in subsequent tasks.
        let _root = tree.root_node();

        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::NodeKind;

    fn extract(source: &str) -> ExtractionResult {
        let extractor = PhpExtractor::new();
        extractor.extract_file(Path::new("src/Main.php"), source.as_bytes(), "App.Main")
    }

    #[test]
    fn extensions_returns_php() {
        let e = PhpExtractor::new();
        assert_eq!(e.extensions(), &["php"]);
    }

    #[test]
    fn module_node_always_created() {
        let r = extract("<?php\n");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].id, "App.Main");
        assert_eq!(r.nodes[0].kind, NodeKind::Module);
        assert_eq!(r.nodes[0].language, Language::Php);
    }
}
```

The exact name `LANGUAGE_PHP` is the current public const in `tree_sitter_php`. If compile fails with "not found in `tree_sitter_php`", replace with `language()` (older API) and adjust to `&tree_sitter_php::language()` — the tree-sitter-php README is the source of truth.

- [ ] **Step 2: Register the module in `lib.rs`**

In `crates/graphify-extract/src/lib.rs`, add `php` to the module list and re-export:

```rust
pub mod cache;
pub mod drizzle;
pub mod go;
pub mod lang;
pub mod php;
pub mod python;
pub mod resolver;
pub mod rust_lang;
pub mod ts_contract;
pub mod typescript;
pub mod walker;

pub use drizzle::{extract_drizzle_contract, extract_drizzle_contract_at, DrizzleParseError};
pub use go::GoExtractor;
pub use lang::{ExtractionResult, LanguageExtractor};
pub use php::PhpExtractor;
pub use python::PythonExtractor;
pub use rust_lang::RustExtractor;
pub use ts_contract::{
    extract_ts_contract, extract_ts_contract_at, parse_all_ts_contracts, parse_all_ts_contracts_at,
    TsContractParseError,
};
pub use typescript::TypeScriptExtractor;
pub use walker::{detect_local_prefix, discover_files, path_to_module, DiscoveredFile};
```

- [ ] **Step 3: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract php::tests
```

Expected: `2 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/src/php.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(extract): add PhpExtractor skeleton with module node extraction"
```

---

## Task 5: PhpExtractor — `use` declarations (simple, aliased, group, function, const)

**Context:** Imports are the most important edge source — they drive local-module resolution and coupling metrics. Tree-sitter-php emits all `use` forms under `namespace_use_declaration`, with children varying by form.

**Files:**
- Modify: `crates/graphify-extract/src/php.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module in `crates/graphify-extract/src/php.rs`:

```rust
#[test]
fn simple_use_produces_imports_and_calls_edges() {
    use graphify_core::types::EdgeKind;
    let r = extract("<?php\nuse App\\Services\\Llm;\n");

    // Imports edge to the containing module
    let imports: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Imports)
        .collect();
    assert_eq!(imports.len(), 1, "expected 1 Imports edge, got {:?}", imports);
    assert_eq!(imports[0].0, "App.Main");
    assert_eq!(imports[0].1, "App.Services");

    // Calls edge to the qualified symbol
    let calls: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "App.Services.Llm")
        .collect();
    assert_eq!(calls.len(), 1, "expected Calls edge to App.Services.Llm");
}

#[test]
fn aliased_use_ignores_alias_and_targets_original_name() {
    use graphify_core::types::EdgeKind;
    let r = extract("<?php\nuse App\\Services\\Llm as L;\n");

    let calls: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(calls.contains(&"App.Services.Llm"), "got {:?}", calls);
    assert!(!calls.contains(&"App.Services.L"), "alias must not become a target");
}

#[test]
fn group_use_expands_to_multiple_edges() {
    use graphify_core::types::EdgeKind;
    let r = extract("<?php\nuse App\\Services\\{Llm, Embed};\n");

    let calls: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(calls.contains(&"App.Services.Llm"), "got {:?}", calls);
    assert!(calls.contains(&"App.Services.Embed"), "got {:?}", calls);
}

#[test]
fn use_function_produces_calls_edge() {
    use graphify_core::types::EdgeKind;
    let r = extract("<?php\nuse function App\\helpers\\format;\n");
    let calls: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "App.helpers.format")
        .collect();
    assert_eq!(calls.len(), 1);
}

#[test]
fn use_const_produces_calls_edge() {
    use graphify_core::types::EdgeKind;
    let r = extract("<?php\nuse const App\\helpers\\VERSION;\n");
    let calls: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "App.helpers.VERSION")
        .collect();
    assert_eq!(calls.len(), 1);
}

#[test]
fn import_confidence_is_extracted_1_0() {
    use graphify_core::types::{ConfidenceKind, EdgeKind};
    let r = extract("<?php\nuse App\\Services\\Llm;\n");
    let imp = r
        .edges
        .iter()
        .find(|e| e.2.kind == EdgeKind::Imports)
        .expect("Imports edge");
    assert_eq!(imp.2.confidence, 1.0);
    assert_eq!(imp.2.confidence_kind, ConfidenceKind::Extracted);
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-extract php::tests::simple_use_produces
cargo test -p graphify-extract php::tests::aliased_use_ignores
cargo test -p graphify-extract php::tests::group_use_expands
cargo test -p graphify-extract php::tests::use_function_produces
cargo test -p graphify-extract php::tests::use_const_produces
```

Expected: all fail — no `use` handling yet.

- [ ] **Step 3: Implement `use` dispatch**

Replace the body of `extract_file` in `crates/graphify-extract/src/php.rs` (the part after the module-node push) and add helpers. Also merge the existing `use graphify_core::types::{Language, Node};` into a single `use` line that includes `Edge` (added here). The updated `extract_file` body should be:

```rust
        let mut result = ExtractionResult::new();
        result
            .nodes
            .push(Node::module(module_name, path, Language::Php, 1, true));

        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            // PHP wraps top-level in `php_tag` sometimes; drill into `program`-equivalent.
            dispatch_top_level(&child, source, path, module_name, &mut result);
        }

        result
```

Update the imports at the top of the file to include `Edge` (will be used by the helpers below):

```rust
use graphify_core::types::{Edge, Language, Node};
```

Add the helper functions below the `impl LanguageExtractor` block (before the `#[cfg(test)]` block). Note that `dispatch_top_level` takes `path: &Path` — later tasks emit symbol nodes that require it, and pre-threading it now avoids a signature churn.

```rust
// ---------------------------------------------------------------------------
// Top-level dispatch
// ---------------------------------------------------------------------------

fn dispatch_top_level(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "namespace_use_declaration" => {
            extract_namespace_use(node, source, module_name, result);
        }
        // Other top-level forms added in later tasks.
        _ => {
            // Recurse in case the node wraps further statements (e.g. php_tag
            // wraps the statement list).
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                dispatch_top_level(&child, source, path, module_name, result);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `use` declarations
// ---------------------------------------------------------------------------

/// Handle `use X\Y\Z;`, `use X\Y as Z;`, `use function X\y;`, `use const X\Y;`,
/// and group forms `use X\{A, B};`.
fn extract_namespace_use(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    // Walk clauses; each `namespace_use_clause` or `namespace_use_group` holds
    // the names.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "namespace_use_clause" => {
                emit_use_clause(&child, source, module_name, line, None, result);
            }
            "namespace_use_group" => {
                // Group form: `use Prefix\{ A, B };` — need to extract the prefix
                // and then each inner clause.
                extract_namespace_use_group(&child, source, module_name, line, result);
            }
            // Some grammars use `namespace_use_group_clause` for entries inside a group;
            // handled inside `extract_namespace_use_group`.
            _ => {}
        }
    }
}

/// Emit edges for a single `namespace_use_clause` node (optionally prefixed by
/// a group path like `App\Services`).
fn emit_use_clause(
    clause: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    group_prefix: Option<&str>,
    result: &mut ExtractionResult,
) {
    // The qualified name lives in a `qualified_name` or `name` child.
    // Alias (when present) lives in a `name` child after an `as` keyword.
    let mut qualified: Option<String> = None;
    let mut cursor = clause.walk();
    for sub in clause.children(&mut cursor) {
        match sub.kind() {
            "qualified_name" | "name" => {
                if qualified.is_none() {
                    qualified = Some(sub.utf8_text(source).unwrap_or("").to_owned());
                }
                // Subsequent `name` children after an `as` are aliases — ignored.
            }
            _ => {}
        }
    }

    let Some(raw) = qualified else { return };
    let raw = raw.trim_start_matches('\\').to_owned();
    if raw.is_empty() {
        return;
    }

    // Apply group prefix if present.
    let full = match group_prefix {
        Some(prefix) => format!("{}\\{}", prefix.trim_start_matches('\\'), raw),
        None => raw,
    };

    // Normalize `\` → `.` for the graph id.
    let symbol_id = full.replace('\\', ".");

    // Module id = symbol's parent namespace; strip the last dot-segment.
    let module_id = match symbol_id.rsplit_once('.') {
        Some((parent, _)) => parent.to_owned(),
        None => symbol_id.clone(), // bare `use Foo;` — module and symbol coincide
    };

    if !module_id.is_empty() && module_id != symbol_id {
        result
            .edges
            .push((module_name.to_owned(), module_id, Edge::imports(line)));
    } else {
        // Single-segment import: emit the Imports edge targeting the symbol itself
        // so there's still a coupling record in the graph.
        result
            .edges
            .push((module_name.to_owned(), symbol_id.clone(), Edge::imports(line)));
    }

    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::calls(line)));
}

/// Handle a `namespace_use_group` node. Extracts the group prefix and iterates
/// inner clauses.
fn extract_namespace_use_group(
    group: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    result: &mut ExtractionResult,
) {
    // The prefix is the first `qualified_name` or `namespace_name` sibling.
    let mut prefix: Option<String> = None;
    let mut cursor = group.walk();
    for child in group.children(&mut cursor) {
        match child.kind() {
            "qualified_name" | "namespace_name" | "name" if prefix.is_none() => {
                prefix = Some(child.utf8_text(source).unwrap_or("").to_owned());
            }
            "namespace_use_group_clause" | "namespace_use_clause" => {
                if let Some(ref p) = prefix {
                    emit_use_clause(&child, source, module_name, line, Some(p), result);
                }
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract php::tests
```

Expected: the 6 use-related tests plus the 2 skeleton tests pass. If tree-sitter-php reports different node kind names for your grammar version (e.g. `namespace_use_group_clause` vs `namespace_use_clause` inside groups), the failing assertion error will tell you which name; adjust the match arms and re-run.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/php.rs
git commit -m "feat(php): extract use declarations (simple, aliased, group, function, const)"
```

---

## Task 6: PhpExtractor — class/interface/trait/enum/function declarations

**Context:** `Defines` edges come from top-level declarations. All five have the same shape: name field + body field. Differ only in which `NodeKind` variant to emit.

**Files:**
- Modify: `crates/graphify-extract/src/php.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module:

```rust
#[test]
fn class_declaration_creates_class_node_and_defines() {
    use graphify_core::types::{EdgeKind, NodeKind};
    let r = extract("<?php\nclass Llm {}\n");
    let class = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Class)
        .expect("Class node");
    assert_eq!(class.id, "App.Main.Llm");

    let defines: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Defines && e.1 == "App.Main.Llm")
        .collect();
    assert_eq!(defines.len(), 1);
}

#[test]
fn interface_declaration_creates_trait_node() {
    use graphify_core::types::NodeKind;
    let r = extract("<?php\ninterface Servicer {}\n");
    let ifc = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Trait)
        .expect("Trait node for interface");
    assert_eq!(ifc.id, "App.Main.Servicer");
}

#[test]
fn trait_declaration_creates_trait_node() {
    use graphify_core::types::NodeKind;
    let r = extract("<?php\ntrait Loggable {}\n");
    let tr = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Trait && n.id == "App.Main.Loggable")
        .expect("Trait node");
    assert_eq!(tr.language, Language::Php);
}

#[test]
fn enum_declaration_creates_enum_node() {
    use graphify_core::types::NodeKind;
    let r = extract("<?php\nenum Status { case Active; case Archived; }\n");
    let en = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Enum)
        .expect("Enum node");
    assert_eq!(en.id, "App.Main.Status");
}

#[test]
fn top_level_function_definition_creates_function_node() {
    use graphify_core::types::NodeKind;
    let r = extract("<?php\nfunction format(string $s): string { return $s; }\n");
    let fn_node = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Function)
        .expect("Function node");
    assert_eq!(fn_node.id, "App.Main.format");
}

#[test]
fn defines_confidence_is_extracted_1_0() {
    use graphify_core::types::{ConfidenceKind, EdgeKind};
    let r = extract("<?php\nclass Llm {}\n");
    let def = r
        .edges
        .iter()
        .find(|e| e.2.kind == EdgeKind::Defines)
        .expect("Defines edge");
    assert_eq!(def.2.confidence, 1.0);
    assert_eq!(def.2.confidence_kind, ConfidenceKind::Extracted);
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-extract php::tests::class_declaration_creates
cargo test -p graphify-extract php::tests::interface_declaration_creates
cargo test -p graphify-extract php::tests::trait_declaration_creates
cargo test -p graphify-extract php::tests::enum_declaration_creates
cargo test -p graphify-extract php::tests::top_level_function_definition
```

Expected: all fail — no declaration handling yet.

- [ ] **Step 3: Implement the declaration handlers**

In `crates/graphify-extract/src/php.rs`, extend `dispatch_top_level` with arms for the five declaration kinds and add the `extract_symbol` helper. Replace the full `dispatch_top_level` function with:

```rust
fn dispatch_top_level(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "namespace_use_declaration" => {
            extract_namespace_use(node, source, module_name, result);
        }
        "class_declaration" => {
            extract_symbol(node, source, path, module_name, NodeKind::Class, result);
        }
        "interface_declaration" => {
            extract_symbol(node, source, path, module_name, NodeKind::Trait, result);
        }
        "trait_declaration" => {
            extract_symbol(node, source, path, module_name, NodeKind::Trait, result);
        }
        "enum_declaration" => {
            extract_symbol(node, source, path, module_name, NodeKind::Enum, result);
        }
        "function_definition" => {
            extract_symbol(node, source, path, module_name, NodeKind::Function, result);
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                dispatch_top_level(&child, source, path, module_name, result);
            }
        }
    }
}
```

Extend the `use graphify_core::types::...` line at the top of the file to include `NodeKind`:

```rust
use graphify_core::types::{Edge, Language, Node, NodeKind};
```

Add the `extract_symbol` helper before the `#[cfg(test)]` block:

```rust
// ---------------------------------------------------------------------------
// Symbol declarations (class, interface, trait, enum, function)
// ---------------------------------------------------------------------------

fn extract_symbol(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    kind: NodeKind,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = name_node.utf8_text(source).unwrap_or("");
    if name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        kind,
        path,
        Language::Php,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));
}
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract php::tests
```

Expected: all tests so far (skeleton + use + declarations) pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/php.rs
git commit -m "feat(php): extract class/interface/trait/enum/function declarations"
```

---

## Task 7: PhpExtractor — method declarations inside class/trait/enum bodies

**Context:** Methods live inside class bodies. Symbol id scheme: `{module}.{ClassName}.{method}`. The `Defines` edge is emitted **from the module** (not from the class) — this matches Go's behavior, so shortest-path queries `graphify path module method` work.

**Files:**
- Modify: `crates/graphify-extract/src/php.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module:

```rust
#[test]
fn method_declaration_inside_class_creates_method_node() {
    use graphify_core::types::{EdgeKind, NodeKind};
    let r = extract(
        r#"<?php
class Llm {
    public function call(): string { return "x"; }
}
"#,
    );

    let method = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Method)
        .expect("Method node");
    assert_eq!(method.id, "App.Main.Llm.call");

    let defines: Vec<_> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Defines && e.1 == "App.Main.Llm.call")
        .collect();
    assert_eq!(defines.len(), 1);
    assert_eq!(defines[0].0, "App.Main", "Defines edge must come from module");
}

#[test]
fn method_inside_trait_creates_method_node() {
    use graphify_core::types::NodeKind;
    let r = extract(
        r#"<?php
trait Loggable {
    public function log(string $m): void {}
}
"#,
    );
    let method = r
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Method && n.id == "App.Main.Loggable.log")
        .expect("Method node inside trait");
    assert_eq!(method.language, Language::Php);
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-extract php::tests::method_declaration_inside_class
cargo test -p graphify-extract php::tests::method_inside_trait_creates
```

Expected: fail — no method handling.

- [ ] **Step 3: Implement method extraction**

In `crates/graphify-extract/src/php.rs`, modify `extract_symbol` to walk the declaration's body (if present) and emit method nodes for any `method_declaration` descendants. Update `extract_symbol` to pass both the name (needed for the method id prefix) and recurse:

```rust
fn extract_symbol(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    kind: NodeKind,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = name_node.utf8_text(source).unwrap_or("");
    if name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        kind.clone(),
        path,
        Language::Php,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id.clone(), Edge::defines(line)));

    // Walk body: method declarations emit Method nodes (only for class/trait/enum).
    // Functions don't contain methods, so short-circuit for NodeKind::Function.
    if matches!(kind, NodeKind::Function) {
        return;
    }

    if let Some(body) = node.child_by_field_name("body") {
        extract_methods_in_body(&body, source, path, module_name, name, result);
    }
}

/// Walk a class/trait/enum body and emit `Method` nodes for each
/// `method_declaration`. Nested types (inner class) are not expected in PHP;
/// do not recurse past method bodies.
fn extract_methods_in_body(
    body: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    owner_name: &str,
    result: &mut ExtractionResult,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "method_declaration" {
            emit_method(&child, source, path, module_name, owner_name, result);
        }
    }
}

fn emit_method(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    owner_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let method_name = name_node.utf8_text(source).unwrap_or("");
    if method_name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}.{}", module_name, owner_name, method_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Method,
        path,
        Language::Php,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));
}
```

The `kind.clone()` requires `NodeKind` to implement `Clone`. Check `crates/graphify-core/src/types.rs`: `NodeKind` already derives `Clone` (it's on the `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`), so this compiles.

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract php::tests::method_declaration_inside_class
cargo test -p graphify-extract php::tests::method_inside_trait_creates
cargo test -p graphify-extract php::tests
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/php.rs
git commit -m "feat(php): extract method declarations inside classes/traits/enums"
```

---

## Task 8: PhpExtractor — bare function calls; skip static/instance method calls

**Context:** Same policy as Python (`identifier` child) and Go (`identifier` function-field): only bare function calls are tracked. `Class::method()` (scoped) and `$obj->method()` (member) are skipped to match Level A scope.

**Files:**
- Modify: `crates/graphify-extract/src/php.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module:

```rust
#[test]
fn bare_call_inside_function_produces_calls_edge() {
    use graphify_core::types::EdgeKind;
    let r = extract(
        r#"<?php
function main() {
    setup();
    configure(true);
}
"#,
    );
    let calls: Vec<&str> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(calls.contains(&"setup"), "got {:?}", calls);
    assert!(calls.contains(&"configure"), "got {:?}", calls);
}

#[test]
fn bare_call_confidence_is_inferred_0_7() {
    use graphify_core::types::{ConfidenceKind, EdgeKind};
    let r = extract(
        r#"<?php
function main() {
    foo();
}
"#,
    );
    let call = r
        .edges
        .iter()
        .find(|e| e.2.kind == EdgeKind::Calls && e.1 == "foo")
        .expect("Calls edge to foo");
    assert_eq!(call.2.confidence, 0.7);
    assert_eq!(call.2.confidence_kind, ConfidenceKind::Inferred);
}

#[test]
fn static_call_not_extracted_as_bare() {
    use graphify_core::types::EdgeKind;
    let r = extract(
        r#"<?php
function main() {
    Llm::call();
}
"#,
    );
    let calls: Vec<&str> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(!calls.contains(&"call"), "static call must be skipped; got {:?}", calls);
}

#[test]
fn instance_method_call_not_extracted_as_bare() {
    use graphify_core::types::EdgeKind;
    let r = extract(
        r#"<?php
function main() {
    $llm = new Llm();
    $llm->call();
}
"#,
    );
    let calls: Vec<&str> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(!calls.contains(&"call"), "instance call must be skipped; got {:?}", calls);
}

#[test]
fn bare_call_inside_method_produces_calls_edge() {
    use graphify_core::types::EdgeKind;
    let r = extract(
        r#"<?php
class Llm {
    public function run() {
        helper();
    }
}
"#,
    );
    let calls: Vec<&str> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(calls.contains(&"helper"), "got {:?}", calls);
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-extract php::tests::bare_call_inside_function
cargo test -p graphify-extract php::tests::bare_call_confidence
cargo test -p graphify-extract php::tests::static_call_not_extracted
cargo test -p graphify-extract php::tests::instance_method_call_not_extracted
cargo test -p graphify-extract php::tests::bare_call_inside_method
```

Expected: the positive tests fail (no calls emitted); the negative ones may pass trivially (no calls at all). After the implementation, both sets must pass.

- [ ] **Step 3: Implement call extraction**

Add to `crates/graphify-extract/src/php.rs` (after `emit_method`, before tests):

```rust
// ---------------------------------------------------------------------------
// Bare call extraction
// ---------------------------------------------------------------------------

/// Recursively scan `node` for `function_call_expression` whose callee is a
/// bare `name` identifier, emitting a `Calls` edge with confidence 0.7 /
/// Inferred. Skips `scoped_call_expression` (e.g. `A::b()`) and
/// `member_call_expression` (e.g. `$a->b()`), matching the Go/Python policy.
fn extract_calls_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    if node.kind() == "function_call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            // Accept only bare identifiers (`name` or `qualified_name` with no
            // leading slash and no separator). Qualified calls like
            // `App\Services\foo()` would produce a different edge target —
            // Level A treats them as bare if the callee is a single-segment
            // name.
            if func.kind() == "name" {
                let callee = func.utf8_text(source).unwrap_or("").to_owned();
                if !callee.is_empty() {
                    let line = node.start_position().row + 1;
                    result.edges.push((
                        module_name.to_owned(),
                        callee,
                        Edge::calls(line)
                            .with_confidence(0.7, graphify_core::types::ConfidenceKind::Inferred),
                    ));
                }
            }
            // `scoped_call_expression` and `member_call_expression` fall
            // through without emitting an edge.
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(&child, source, module_name, result);
    }
}
```

Now call `extract_calls_recursive` from the declaration handlers. Update `extract_symbol` to recurse through the body for bare calls **before** returning:

```rust
fn extract_symbol(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    kind: NodeKind,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = name_node.utf8_text(source).unwrap_or("");
    if name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        kind.clone(),
        path,
        Language::Php,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id.clone(), Edge::defines(line)));

    // For functions: scan the body for bare calls and return (no methods).
    if matches!(kind, NodeKind::Function) {
        if let Some(body) = node.child_by_field_name("body") {
            extract_calls_recursive(&body, source, module_name, result);
        }
        return;
    }

    // For class/trait/enum: walk body for methods + bare calls anywhere.
    if let Some(body) = node.child_by_field_name("body") {
        extract_methods_in_body(&body, source, path, module_name, name, result);
        extract_calls_recursive(&body, source, module_name, result);
    }
}
```

Also update `dispatch_top_level`'s fallback branch to scan any non-matched top-level node for bare calls (covers top-level `foo();` statements in script-style PHP files):

```rust
        _ => {
            // First try matching deeper — PHP wraps statements in containers
            // we may not enumerate explicitly.
            let mut has_specific_child = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "namespace_use_declaration"
                    | "class_declaration"
                    | "interface_declaration"
                    | "trait_declaration"
                    | "enum_declaration"
                    | "function_definition" => {
                        dispatch_top_level(&child, source, path, module_name, result);
                        has_specific_child = true;
                    }
                    _ => {}
                }
            }
            // If none of the children were language-level declarations, treat
            // this subtree as expression statements and scan for calls.
            if !has_specific_child {
                extract_calls_recursive(node, source, module_name, result);
            }
        }
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract php::tests
```

Expected: all PHP tests pass so far (~18+ tests).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/php.rs
git commit -m "feat(php): extract bare function calls, skip static/instance calls"
```

---

## Task 9: PhpExtractor — full-file integration test

**Context:** Sanity check that class + method + use + call all compose correctly in one file, matching the `full_go_file` test in the Go extractor.

**Files:**
- Modify: `crates/graphify-extract/src/php.rs`

- [ ] **Step 1: Write the integration test**

Append to the `tests` module:

```rust
#[test]
fn full_php_file_integration() {
    use graphify_core::types::{EdgeKind, NodeKind};
    let r = extract(
        r#"<?php
namespace App\Services;

use App\Models\User;
use App\Logging\{Logger, Level};

interface Servicer {
    public function serve(): string;
}

class Llm implements Servicer {
    public function serve(): string {
        $user = new User();
        log_event("served");
        return "x";
    }

    public function helper(): void {
        helper_fn();
    }
}

function make_llm(): Llm {
    setup();
    return new Llm();
}
"#,
    );

    // 1 module + 1 interface + 1 class + 2 methods + 1 function = 6 nodes
    let nodes_by_kind = |k: NodeKind| -> Vec<&str> {
        r.nodes
            .iter()
            .filter(|n| n.kind == k)
            .map(|n| n.id.as_str())
            .collect()
    };
    assert_eq!(nodes_by_kind(NodeKind::Module), vec!["App.Main"]);
    assert_eq!(nodes_by_kind(NodeKind::Trait), vec!["App.Main.Servicer"]);
    assert_eq!(nodes_by_kind(NodeKind::Class), vec!["App.Main.Llm"]);
    let methods = nodes_by_kind(NodeKind::Method);
    assert!(methods.contains(&"App.Main.Servicer.serve"), "got {:?}", methods);
    assert!(methods.contains(&"App.Main.Llm.serve"), "got {:?}", methods);
    assert!(methods.contains(&"App.Main.Llm.helper"), "got {:?}", methods);
    assert_eq!(nodes_by_kind(NodeKind::Function), vec!["App.Main.make_llm"]);

    let imports: Vec<&str> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Imports)
        .map(|e| e.1.as_str())
        .collect();
    assert!(imports.contains(&"App.Models"), "Imports to App.Models; got {:?}", imports);
    assert!(imports.contains(&"App.Logging"), "Imports to App.Logging (group); got {:?}", imports);

    let calls: Vec<&str> = r
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    // `use` produces Calls to qualified symbols
    assert!(calls.contains(&"App.Models.User"));
    assert!(calls.contains(&"App.Logging.Logger"));
    assert!(calls.contains(&"App.Logging.Level"));
    // Bare calls from method/function bodies
    assert!(calls.contains(&"log_event"));
    assert!(calls.contains(&"helper_fn"));
    assert!(calls.contains(&"setup"));
}
```

- [ ] **Step 2: Run the test — confirm it passes**

```bash
cargo test -p graphify-extract php::tests::full_php_file_integration
```

Expected: pass. If any assertion fails, the message reveals which edge/node is missing; likely causes are tree-sitter-php node-kind names differing from what the plan assumed — fix the match arms in `dispatch_top_level` or `extract_namespace_use`.

- [ ] **Step 3: Run the full extract test suite to check for regressions**

```bash
cargo test -p graphify-extract
```

Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/src/php.rs
git commit -m "test(php): add full-file integration test for PhpExtractor"
```

---

## Task 10: Resolver — `load_composer_json()`

**Context:** The resolver needs to know PSR-4 mappings for two reasons: (1) the walker calls it to translate paths (Task 12), (2) the resolver itself uses them transparently — actually, the current design keeps PSR-4 translation entirely in the walker; the resolver only normalizes `\` → `.` for `use` targets. But `ModuleResolver` is where the parsed mappings live so the CLI can populate them once and pass them both to the walker (via a parameter) and keep them accessible.

**Files:**
- Modify: `crates/graphify-extract/src/resolver.rs`

- [ ] **Step 1: Write failing tests**

Append to `crates/graphify-extract/src/resolver.rs`'s `#[cfg(test)] mod tests { ... }` block (find it near the end of the file; add at the end of that module):

```rust
#[test]
fn load_composer_json_parses_psr4_mappings() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("composer.json");
    std::fs::write(
        &path,
        r#"{
  "name": "vendor/pkg",
  "autoload": {
    "psr-4": {
      "App\\": "src/",
      "App\\Legacy\\": "src/legacy/"
    }
  }
}"#,
    )
    .unwrap();

    let mut resolver = ModuleResolver::new(tmp.path());
    resolver.load_composer_json(&path);

    let mappings = resolver.psr4_mappings();
    assert!(
        mappings.iter().any(|(ns, dir)| ns == "App\\" && dir == "src/"),
        "App\\ → src/ mapping; got {:?}",
        mappings
    );
    assert!(
        mappings
            .iter()
            .any(|(ns, dir)| ns == "App\\Legacy\\" && dir == "src/legacy/"),
        "App\\Legacy\\ → src/legacy/ mapping; got {:?}",
        mappings
    );
}

#[test]
fn load_composer_json_merges_autoload_dev() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("composer.json");
    std::fs::write(
        &path,
        r#"{
  "autoload": { "psr-4": { "App\\": "src/" } },
  "autoload-dev": { "psr-4": { "Tests\\": "tests/" } }
}"#,
    )
    .unwrap();

    let mut resolver = ModuleResolver::new(tmp.path());
    resolver.load_composer_json(&path);

    let mappings = resolver.psr4_mappings();
    assert!(mappings.iter().any(|(ns, _)| ns == "App\\"));
    assert!(mappings.iter().any(|(ns, _)| ns == "Tests\\"));
}

#[test]
fn load_composer_json_handles_malformed_without_panic() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("composer.json");
    std::fs::write(&path, "{ this is not valid json").unwrap();

    let mut resolver = ModuleResolver::new(tmp.path());
    resolver.load_composer_json(&path); // must not panic

    let mappings = resolver.psr4_mappings();
    assert!(mappings.is_empty(), "malformed file leaves mappings empty");
}

#[test]
fn load_composer_json_missing_file_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nonexistent.json");

    let mut resolver = ModuleResolver::new(tmp.path());
    resolver.load_composer_json(&path); // must not panic

    assert!(resolver.psr4_mappings().is_empty());
}
```

Ensure `tempfile` is already a dev-dep (it is — confirmed earlier in `Cargo.toml`).

- [ ] **Step 2: Run the tests — confirm they fail to compile**

```bash
cargo test -p graphify-extract resolver::tests::load_composer_json
```

Expected: fails — `load_composer_json` and `psr4_mappings` don't exist.

- [ ] **Step 3: Add the field, accessor, and loader**

In `crates/graphify-extract/src/resolver.rs`, update the `ModuleResolver` struct (around lines 16-27). Add a new field `psr4_mappings: Vec<(String, String)>`:

```rust
pub struct ModuleResolver {
    known_modules: HashMap<String, String>,
    ts_aliases: Vec<(String, String)>,
    go_module_path: Option<String>,
    psr4_mappings: Vec<(String, String)>,
    #[allow(dead_code)]
    root: PathBuf,
}
```

Update `new` (around line 34-42) to initialize the field:

```rust
    pub fn new(root: &Path) -> Self {
        Self {
            known_modules: HashMap::new(),
            ts_aliases: Vec::new(),
            go_module_path: None,
            psr4_mappings: Vec::new(),
            root: root.to_path_buf(),
        }
    }
```

Add a public accessor and the loader as new methods on the `impl ModuleResolver` block (right after the existing `load_go_mod` method — find it in the file; it's near line 162):

```rust
    /// Return an immutable view over the parsed PSR-4 mappings. Used by the
    /// walker to translate file paths to namespace-prefixed module names.
    pub fn psr4_mappings(&self) -> &[(String, String)] {
        &self.psr4_mappings
    }

    /// Parse `composer.json` and load `autoload.psr-4` + `autoload-dev.psr-4`
    /// mappings. Tolerates missing files and malformed JSON — failures are
    /// logged to stderr and leave the mappings empty.
    pub fn load_composer_json(&mut self, composer_path: &Path) {
        let text = match std::fs::read_to_string(composer_path) {
            Ok(t) => t,
            Err(_) => return,
        };

        // Parse both `"autoload"` and `"autoload-dev"` blocks.
        for section in ["autoload", "autoload-dev"] {
            if let Some(psr4_block) = find_psr4_block(&text, section) {
                for pair in parse_psr4_pairs(&psr4_block) {
                    self.psr4_mappings.push(pair);
                }
            }
        }
    }
```

Add two hand-written parser helpers at the module level (before `#[cfg(test)]`):

```rust
// ---------------------------------------------------------------------------
// Composer.json mini-parser (PSR-4 only, no serde_json dep)
// ---------------------------------------------------------------------------

/// Find the body of `<section>.psr-4` as a raw substring between its outer
/// `{` and the matching `}`. Returns None if not found or if the JSON is too
/// malformed to locate the block.
fn find_psr4_block(text: &str, section: &str) -> Option<String> {
    // Locate the section key, e.g. `"autoload"`.
    let section_key = format!("\"{}\"", section);
    let section_pos = text.find(&section_key)?;

    // After the key, find `"psr-4"` before the section ends.
    let after_section = &text[section_pos + section_key.len()..];
    let psr4_pos = after_section.find("\"psr-4\"")?;

    // Skip past `"psr-4"` and locate the opening brace.
    let after_psr4 = &after_section[psr4_pos + "\"psr-4\"".len()..];
    let open_brace = after_psr4.find('{')?;

    // Walk forward counting braces until balanced.
    let body_start = open_brace + 1;
    let body_bytes = after_psr4[body_start..].as_bytes();
    let mut depth: i32 = 1;
    let mut end: Option<usize> = None;
    for (i, &b) in body_bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;

    Some(after_psr4[body_start..body_start + end].to_owned())
}

/// Parse the body of a PSR-4 block, returning `(namespace_prefix, dir_prefix)`
/// pairs. Accepts both `"App\\": "src/"` and `"App\\": ["src/"]` (first entry
/// of an array wins). Tolerates whitespace and unknown keys.
fn parse_psr4_pairs(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip whitespace and separators.
        while i < bytes.len()
            && (bytes[i].is_ascii_whitespace() || bytes[i] == b',')
        {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        // Expect a quoted key.
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let key_start = i + 1;
        let key_end = match find_unescaped_quote(bytes, key_start) {
            Some(p) => p,
            None => break,
        };
        let key = &body[key_start..key_end];
        i = key_end + 1;

        // Skip to colon.
        while i < bytes.len() && bytes[i] != b':' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        i += 1; // past colon

        // Skip whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // Value is either `"..."` or `[...]`.
        let value = if i < bytes.len() && bytes[i] == b'"' {
            let v_start = i + 1;
            let v_end = match find_unescaped_quote(bytes, v_start) {
                Some(p) => p,
                None => break,
            };
            let v = body[v_start..v_end].to_owned();
            i = v_end + 1;
            Some(v)
        } else if i < bytes.len() && bytes[i] == b'[' {
            // Find first string inside the array.
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b']' {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'"' {
                let v_start = i + 1;
                let v_end = match find_unescaped_quote(bytes, v_start) {
                    Some(p) => p,
                    None => break,
                };
                let v = body[v_start..v_end].to_owned();
                i = v_end + 1;
                // Skip to closing bracket.
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                Some(v)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(v) = value {
            // Un-escape `\\` → `\` in the key (namespace prefix).
            let key_unescaped = unescape_backslashes(key);
            out.push((key_unescaped, v));
        }
    }

    out
}

/// Find the position of the next unescaped `"` at or after `start`.
fn find_unescaped_quote(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Replace every occurrence of `\\` with `\` (JSON string unescape, subset).
fn unescape_backslashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '\\' {
                    out.push('\\');
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-extract resolver::tests::load_composer_json
```

Expected: all 4 pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/resolver.rs
git commit -m "feat(resolver): parse composer.json PSR-4 autoload mappings"
```

---

## Task 11: Resolver — resolve PHP `use` targets

**Context:** When the extractor emits a raw target like `App.Services.Llm` (already normalized `\`→`.` by the PhpExtractor in Task 5), the resolver's job is just to match it against `known_modules`. But if an extractor pushes a raw-with-backslashes form, the resolver should also handle it. The current resolver handles Python relative, TS alias, TS relative, Go module, and Rust `crate::`. We add a PHP branch that detects `\` in the raw, normalizes it, and matches.

**Files:**
- Modify: `crates/graphify-extract/src/resolver.rs`

- [ ] **Step 1: Write failing tests**

Append to `resolver.rs`'s tests module:

```rust
#[test]
fn resolve_php_use_matches_known_module() {
    let mut resolver = ModuleResolver::new(Path::new("/repo"));
    resolver.register_module("App.Services.Llm");

    let (resolved, is_local, confidence) = resolver.resolve("App\\Services\\Llm", "App.Main", false);
    assert_eq!(resolved, "App.Services.Llm");
    assert!(is_local);
    assert_eq!(confidence, 1.0);
}

#[test]
fn resolve_php_use_nonlocal_still_extracted_confidence() {
    let resolver = ModuleResolver::new(Path::new("/repo"));
    // No known_modules registered — this namespace is external.
    let (resolved, is_local, confidence) =
        resolver.resolve("Symfony\\HttpFoundation\\Request", "App.Main", false);
    assert_eq!(resolved, "Symfony.HttpFoundation.Request");
    assert!(!is_local);
    assert_eq!(confidence, 1.0);
}

#[test]
fn resolve_php_use_strips_leading_backslash() {
    let mut resolver = ModuleResolver::new(Path::new("/repo"));
    resolver.register_module("App.Models.User");

    let (resolved, is_local, _) = resolver.resolve("\\App\\Models\\User", "App.Main", false);
    assert_eq!(resolved, "App.Models.User");
    assert!(is_local);
}
```

- [ ] **Step 2: Run — confirm they fail**

```bash
cargo test -p graphify-extract resolver::tests::resolve_php_use
```

Expected: `resolve_php_use_matches_known_module` fails (returns the raw input with `\` intact); `resolve_php_use_nonlocal_still_extracted_confidence` also fails on the resolved-string assertion; `resolve_php_use_strips_leading_backslash` fails similarly.

- [ ] **Step 3: Add the PHP branch in `resolve()`**

In `crates/graphify-extract/src/resolver.rs`, extend `ModuleResolver::resolve` (around lines 178-228). Add a new branch **before** "Direct module name" (step 6):

```rust
        // 5b. PHP `use` targets (contain a backslash separator).
        if raw.contains('\\') {
            let normalized = raw.trim_start_matches('\\').replace('\\', ".");
            let is_local = self.known_modules.contains_key(&normalized);
            return (normalized, is_local, 1.0);
        }
```

Place it right after the Rust `super::`/`self::` branch, before step 6 ("Direct module name"). The full `resolve` method should now read (focus on the new branch ordering):

```rust
    pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool, f64) {
        // 1. Python relative imports
        if raw.starts_with('.') && !raw.starts_with("./") && !raw.starts_with("../") {
            let resolved = resolve_python_relative(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 2. TypeScript path aliases
        for (alias_pat, target_pat) in &self.ts_aliases {
            if let Some(resolved) = apply_ts_alias(raw, alias_pat, target_pat) {
                let is_local = self.known_modules.contains_key(&resolved);
                return (resolved, is_local, 0.85);
            }
        }

        // 3. TS / generic relative imports
        if raw.starts_with("./") || raw.starts_with("../") {
            let resolved = resolve_ts_relative(raw, from_module);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 4. Go module-path imports
        if let Some(ref go_mod) = self.go_module_path {
            if let Some(rest) = raw.strip_prefix(go_mod.as_str()) {
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                if !rest.is_empty() {
                    let resolved = rest.replace('/', ".");
                    let is_local = self.known_modules.contains_key(&resolved);
                    return (resolved, is_local, 0.9);
                }
            }
        }

        // 5. Rust `crate::`, `super::`, `self::` imports
        if let Some(rest) = raw.strip_prefix("crate::") {
            let resolved = rest.replace("::", ".");
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }
        if raw.starts_with("super::") || raw.starts_with("self::") {
            let resolved = resolve_rust_path(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 5b. PHP `use` targets (contain a backslash separator).
        if raw.contains('\\') {
            let normalized = raw.trim_start_matches('\\').replace('\\', ".");
            let is_local = self.known_modules.contains_key(&normalized);
            return (normalized, is_local, 1.0);
        }

        // 6. Direct module name
        let is_local = self.known_modules.contains_key(raw);
        (raw.to_owned(), is_local, 1.0)
    }
```

- [ ] **Step 4: Run — confirm they pass**

```bash
cargo test -p graphify-extract resolver::tests::resolve_php_use
cargo test -p graphify-extract resolver
```

Expected: all pass (both the new tests and the pre-existing resolver tests).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/resolver.rs
git commit -m "feat(resolver): normalize PHP use targets to dot notation"
```

---

## Task 12: Walker — PSR-4 path translation with longest-prefix match

**Context:** When composer.json has `"App\\": "src/"`, the walker must map `src/Services/Llm.php` → `App.Services.Llm`. Multiple overlapping mappings (e.g. `App\\` → `src/` and `App\\Legacy\\` → `src/legacy/`) must resolve to the longest-dir-prefix match.

**Files:**
- Modify: `crates/graphify-extract/src/walker.rs`

- [ ] **Step 1: Write failing tests**

Append to the walker's tests module (at the very end, after the PHP section added in Task 3):

```rust
#[test]
fn path_to_module_php_with_psr4_mapping() {
    let base = Path::new("/repo");
    let file = Path::new("/repo/src/Services/Llm.php");
    let mappings = &[("App\\".to_string(), "src/".to_string())][..];
    assert_eq!(
        path_to_module_psr4(base, file, "", mappings),
        "App.Services.Llm"
    );
}

#[test]
fn path_to_module_php_without_composer_falls_back() {
    let base = Path::new("/repo");
    let file = Path::new("/repo/src/Services/Llm.php");
    let mappings: &[(String, String)] = &[];
    assert_eq!(
        path_to_module_psr4(base, file, "", mappings),
        "src.Services.Llm"
    );
}

#[test]
fn path_to_module_php_longest_prefix_wins() {
    let base = Path::new("/repo");
    let file = Path::new("/repo/src/legacy/Old/Dto.php");
    let mappings = &[
        ("App\\".to_string(), "src/".to_string()),
        ("App\\Legacy\\".to_string(), "src/legacy/".to_string()),
    ][..];
    // Longest-prefix wins: src/legacy/ → App\Legacy\
    assert_eq!(
        path_to_module_psr4(base, file, "", mappings),
        "App.Legacy.Old.Dto"
    );
}

#[test]
fn path_to_module_php_non_matching_path_falls_back() {
    let base = Path::new("/repo");
    let file = Path::new("/repo/scripts/seed.php");
    let mappings = &[("App\\".to_string(), "src/".to_string())][..];
    // No PSR-4 dir matches `scripts/` → fallback to path-based.
    assert_eq!(
        path_to_module_psr4(base, file, "", mappings),
        "scripts.seed"
    );
}
```

- [ ] **Step 2: Run — confirm they fail to compile**

```bash
cargo test -p graphify-extract walker::tests::path_to_module_php_with_psr4
```

Expected: error — `path_to_module_psr4` not found.

- [ ] **Step 3: Implement `path_to_module_psr4` and thread it through `discover_files`**

In `crates/graphify-extract/src/walker.rs`, add the new function after the existing `path_to_module` (around line 138):

```rust
/// PSR-4-aware variant of [`path_to_module`]. When `psr4_mappings` contains a
/// `(namespace_prefix, dir_prefix)` pair whose `dir_prefix` matches the start
/// of the file's path relative to `base`, applies the namespace translation.
/// Longest-matching `dir_prefix` wins. Falls back to [`path_to_module`] when no
/// mapping applies or when `psr4_mappings` is empty.
pub fn path_to_module_psr4(
    base: &Path,
    file: &Path,
    local_prefix: &str,
    psr4_mappings: &[(String, String)],
) -> String {
    if psr4_mappings.is_empty() {
        return path_to_module(base, file, local_prefix);
    }

    let rel = file.strip_prefix(base).unwrap_or(file);
    let rel_str = rel.to_string_lossy();

    // Find the longest `dir_prefix` that is a prefix of `rel_str`.
    let best = psr4_mappings
        .iter()
        .filter(|(_, dir)| rel_str.starts_with(dir.as_str()))
        .max_by_key(|(_, dir)| dir.len());

    let (ns_prefix, dir_prefix) = match best {
        Some(pair) => pair,
        None => return path_to_module(base, file, local_prefix),
    };

    // Strip the dir prefix, prepend the namespace prefix, strip extension.
    let remainder = &rel_str[dir_prefix.len()..];
    let remainder_no_ext = remainder.strip_suffix(".php").unwrap_or(remainder);

    let ns_prefix_clean = ns_prefix.trim_end_matches('\\');

    // Combine: `<ns_prefix_clean>\<remainder_no_ext>` (with `/` separators),
    // then normalize `\` and `/` to `.`.
    let combined = if ns_prefix_clean.is_empty() {
        remainder_no_ext.to_owned()
    } else {
        format!("{}/{}", ns_prefix_clean.replace('\\', "/"), remainder_no_ext)
    };

    combined.replace('/', ".").replace('\\', ".")
}
```

Update `discover_files` to accept `psr4_mappings` and thread it into `walk_dir`. Change the signature and helper:

```rust
pub fn discover_files(
    root: &Path,
    languages: &[Language],
    local_prefix: &str,
    extra_excludes: &[&str],
) -> Vec<DiscoveredFile> {
    discover_files_with_psr4(root, languages, local_prefix, extra_excludes, &[])
}

/// PSR-4-aware variant of [`discover_files`]. When `psr4_mappings` is empty,
/// behaves identically to the non-PSR-4 call.
pub fn discover_files_with_psr4(
    root: &Path,
    languages: &[Language],
    local_prefix: &str,
    extra_excludes: &[&str],
    psr4_mappings: &[(String, String)],
) -> Vec<DiscoveredFile> {
    let mut excludes: Vec<&str> = DEFAULT_EXCLUDES.to_vec();
    excludes.extend_from_slice(extra_excludes);

    let mut results = Vec::new();
    walk_dir(
        root,
        root,
        languages,
        local_prefix,
        &excludes,
        psr4_mappings,
        &mut results,
    );
    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}
```

Update `walk_dir` to accept and use `psr4_mappings`:

```rust
fn walk_dir(
    base: &Path,
    dir: &Path,
    languages: &[Language],
    local_prefix: &str,
    excludes: &[&str],
    psr4_mappings: &[(String, String)],
    out: &mut Vec<DiscoveredFile>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if path.is_dir() {
            if excludes.contains(&name) {
                continue;
            }
            walk_dir(
                base,
                &path,
                languages,
                local_prefix,
                excludes,
                psr4_mappings,
                out,
            );
        } else if path.is_file() {
            if let Some(lang) = is_eligible_source_file(&path, languages) {
                // PSR-4 translation only applies to PHP files.
                let module_name = if lang == Language::Php && !psr4_mappings.is_empty() {
                    path_to_module_psr4(base, &path, local_prefix, psr4_mappings)
                } else {
                    path_to_module(base, &path, local_prefix)
                };
                let is_package = name == "__init__.py"
                    || name == "index.ts"
                    || name == "index.tsx"
                    || name == "mod.rs"
                    || name == "lib.rs"
                    || name == "main.rs";
                out.push(DiscoveredFile {
                    path,
                    language: lang,
                    module_name,
                    is_package,
                });
            }
        }
    }
}
```

Export the new function by adding it to the re-exports in `crates/graphify-extract/src/lib.rs`:

```rust
pub use walker::{
    detect_local_prefix, discover_files, discover_files_with_psr4, path_to_module,
    path_to_module_psr4, DiscoveredFile,
};
```

- [ ] **Step 4: Run tests — confirm they pass**

```bash
cargo test -p graphify-extract walker
```

Expected: all walker tests pass (including the 4 new PSR-4 tests and the existing ones).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/walker.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(walker): PSR-4 aware path-to-module translation"
```

---

## Task 13: CLI — register `PhpExtractor`, parse `"php"`, load `composer.json`, update default config

**Context:** Wire everything into the pipeline. Five surgical edits in `main.rs`.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Write a CLI-level unit test for `parse_languages` accepting `"php"`**

Find where `parse_languages` is tested in the CLI crate — if no tests exist for it, add a minimal test. First, see existing patterns:

```bash
rg "parse_languages" crates/graphify-cli/src --no-filename -n | head -20
```

If there's no existing test, add one directly after the `parse_languages` function in `crates/graphify-cli/src/main.rs`. (If `parse_languages` is private, make the test a `#[cfg(test)] mod tests { use super::*; ... }` module at the bottom of the file.)

```rust
#[cfg(test)]
mod language_parse_tests {
    use super::*;
    use graphify_core::types::Language;

    #[test]
    fn parse_languages_accepts_php() {
        let langs = parse_languages(&["php".to_string()]);
        assert_eq!(langs, vec![Language::Php]);
    }

    #[test]
    fn parse_languages_accepts_all_five() {
        let langs = parse_languages(&[
            "python".to_string(),
            "typescript".to_string(),
            "go".to_string(),
            "rust".to_string(),
            "php".to_string(),
        ]);
        assert_eq!(
            langs,
            vec![
                Language::Python,
                Language::TypeScript,
                Language::Go,
                Language::Rust,
                Language::Php,
            ]
        );
    }
}
```

(If the file already has a `mod tests` at the bottom, append these tests inside that module instead of creating a new one.)

- [ ] **Step 2: Run — confirm the test fails**

```bash
cargo test -p graphify-cli language_parse_tests::parse_languages_accepts_php
```

Expected: fails — `"php"` doesn't match anything so returns an empty vec (or whatever the fallback is).

- [ ] **Step 3: Edit `parse_languages` to accept `"php"`**

In `crates/graphify-cli/src/main.rs`, find the `parse_languages` function (around line 1338-1350). Update the match:

```rust
    match lang_name.to_lowercase().as_str() {
        "python" | "py" => Some(Language::Python),
        "typescript" | "ts" => Some(Language::TypeScript),
        "go" => Some(Language::Go),
        "rust" | "rs" => Some(Language::Rust),
        "php" => Some(Language::Php),
        _ => None,
    }
```

- [ ] **Step 4: Register `PhpExtractor` in the pipeline**

Find the extractor instantiation block (around line 1440-1443). Add `php_extractor`:

```rust
    let python_extractor = PythonExtractor::new();
    let typescript_extractor = TypeScriptExtractor::new();
    let go_extractor = GoExtractor::new();
    let rust_extractor = RustExtractor::new();
    let php_extractor = PhpExtractor::new();
```

Find the imports at the top of `main.rs` (around lines 25-28):

```rust
use graphify_extract::{
    ExtractionResult, GoExtractor, LanguageExtractor, PhpExtractor, PythonExtractor, RustExtractor,
    TypeScriptExtractor,
};
```

(Add `PhpExtractor` to the existing `use` list.)

Find the match expression that dispatches per-language (around lines 1496-1501) and add the PHP arm:

```rust
    let extractor: &dyn LanguageExtractor = match file.language {
        Language::Python => &python_extractor,
        Language::TypeScript => &typescript_extractor,
        Language::Go => &go_extractor,
        Language::Rust => &rust_extractor,
        Language::Php => &php_extractor,
    };
```

- [ ] **Step 5: Load `composer.json` when PHP is in the language list**

Find the existing hook block that loads `tsconfig.json` and `go.mod` (around lines 1452-1465). Add a parallel block for `composer.json`:

```rust
    // Load tsconfig if TypeScript is in the language list.
    if languages.contains(&Language::TypeScript) {
        let tsconfig = repo_path.join("tsconfig.json");
        if tsconfig.exists() {
            resolver.load_tsconfig(&tsconfig);
        }
    }

    // Load go.mod if Go is in the language list.
    if languages.contains(&Language::Go) {
        let go_mod = repo_path.join("go.mod");
        if go_mod.exists() {
            resolver.load_go_mod(&go_mod);
        }
    }

    // Load composer.json if PHP is in the language list.
    if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            resolver.load_composer_json(&composer);
        } else {
            eprintln!(
                "Warning: PHP project at {:?} has no composer.json — PSR-4 resolution \
                 disabled, imports may not resolve to local modules",
                repo_path
            );
        }
    }
```

- [ ] **Step 6: Thread PSR-4 mappings into `discover_files`**

The CLI currently calls `discover_files` (not the PSR-4 variant). Since PSR-4 mappings are available on the resolver after step 5 above, but the walker happens **before** the resolver is constructed in the current code flow, we need to reorganize.

Check the code flow. Near `main.rs` around line 1417+, the sequence is:
1. Build `files = discover_files(...)`
2. Build `resolver`
3. Register each file's `module_name` with the resolver
4. Load tsconfig / go.mod
5. Extract

PSR-4 needs to be applied to the *module_name* at discovery time (step 1). So we must parse `composer.json` BEFORE `discover_files`. Refactor the order:

In `cmd_extract` (or whichever function hosts this logic), before calling `discover_files`, parse `composer.json` into a local `psr4_mappings` vector, then pass it into the walker via `discover_files_with_psr4`.

Replace the section (around lines 1417-1466) with:

```rust
    // Pre-parse composer.json if PHP is in the project's language list.
    // PSR-4 mappings must be known at walk time so module_names are computed
    // in namespace-space (not path-space).
    let psr4_mappings: Vec<(String, String)> = if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            let mut tmp = graphify_extract::resolver::ModuleResolver::new(&repo_path);
            tmp.load_composer_json(&composer);
            tmp.psr4_mappings().to_vec()
        } else {
            eprintln!(
                "Warning: PHP project at {:?} has no composer.json — PSR-4 resolution \
                 disabled, imports may not resolve to local modules",
                repo_path
            );
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Discover files — use PSR-4 variant when mappings are available (PHP
    // projects). For non-PHP projects, the mappings vec is empty and the call
    // degrades to the regular `discover_files`.
    let files = graphify_extract::discover_files_with_psr4(
        &repo_path,
        &languages,
        &effective_local_prefix,
        &extra_excludes_refs,
        &psr4_mappings,
    );
```

Then build the resolver as before. Because `load_composer_json` was already called on the temporary resolver above for parsing, call it again on the real resolver so its internal `psr4_mappings` field is populated for any future callers:

```rust
    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    for file in &files {
        resolver.register_module(&file.module_name);
    }

    if languages.contains(&Language::TypeScript) {
        let tsconfig = repo_path.join("tsconfig.json");
        if tsconfig.exists() {
            resolver.load_tsconfig(&tsconfig);
        }
    }

    if languages.contains(&Language::Go) {
        let go_mod = repo_path.join("go.mod");
        if go_mod.exists() {
            resolver.load_go_mod(&go_mod);
        }
    }

    if languages.contains(&Language::Php) {
        let composer = repo_path.join("composer.json");
        if composer.exists() {
            resolver.load_composer_json(&composer);
        }
        // Warning already printed above; don't duplicate.
    }
```

Inspect the diff to make sure `extra_excludes_refs` is the correct variable name used in the existing `discover_files` call — match what was there before.

- [ ] **Step 7: Update the default-config comment**

Find the `graphify init` default config template (around line 987):

```rust
lang = ["python"]           # Options: python, typescript, go, rust
```

Change to:

```rust
lang = ["python"]           # Options: python, typescript, go, rust, php
```

- [ ] **Step 8: Run all CLI tests**

```bash
cargo test -p graphify-cli
```

Expected: all green, including the new `parse_languages_accepts_php` and `parse_languages_accepts_all_five` tests. If the code has compile issues in the refactored PSR-4 section, fix them until everything builds.

- [ ] **Step 9: Full workspace test**

```bash
cargo test --workspace
```

Expected: all green.

- [ ] **Step 10: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat(cli): register PhpExtractor and load composer.json PSR-4 mappings"
```

---

## Task 14: Watch mode — map `"php"` to `.php` files

**Context:** Watch mode's per-project extension mapping decides which file changes trigger a rebuild.

**Files:**
- Modify: `crates/graphify-cli/src/watch.rs`

- [ ] **Step 1: Find existing watch tests**

```bash
rg "language_extensions|languages_from_project" crates/graphify-cli/src/watch.rs -n
```

- [ ] **Step 2: Write a failing test**

Append to `crates/graphify-cli/src/watch.rs`'s tests module (find the existing `#[cfg(test)] mod tests { ... }`; add inside it):

```rust
    #[test]
    fn php_language_maps_to_php_extension() {
        let exts = language_extensions(&["php".to_string()]);
        assert_eq!(exts, vec!["php".to_string()]);
    }
```

(If the helper is named differently, e.g. `extensions_for`, adjust the test accordingly.)

- [ ] **Step 3: Run — confirm it fails**

```bash
cargo test -p graphify-cli watch::tests::php_language_maps_to_php
```

Expected: fail (empty vec or missing arm).

- [ ] **Step 4: Add the mapping**

In `crates/graphify-cli/src/watch.rs`, update the `match` (around lines 19-24):

```rust
    languages
        .iter()
        .flat_map(|l| match l.as_str() {
            "python" => vec!["py".to_string()],
            "typescript" => vec!["ts".to_string(), "tsx".to_string()],
            "go" => vec!["go".to_string()],
            "rust" => vec!["rs".to_string()],
            "php" => vec!["php".to_string()],
            _ => vec![],
        })
        .collect()
```

- [ ] **Step 5: Run — confirm it passes**

```bash
cargo test -p graphify-cli watch
```

Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-cli/src/watch.rs
git commit -m "feat(watch): map php language to .php extension"
```

---

## Task 15: Fixture — `tests/fixtures/php_project/`

**Context:** Minimal Laravel-style layout for integration + E2E tests. Committed as real source files in-repo.

**Files:**
- Create: `tests/fixtures/php_project/composer.json`
- Create: `tests/fixtures/php_project/src/Main.php`
- Create: `tests/fixtures/php_project/src/Services/Llm.php`
- Create: `tests/fixtures/php_project/src/Models/User.php`
- Create: `tests/fixtures/php_project/src/Controllers/HomeController.php`
- Create: `tests/fixtures/php_project/tests/LlmTest.php`

- [ ] **Step 1: Create `composer.json`**

```json
{
  "name": "graphify/php-fixture",
  "description": "Fixture project for Graphify PHP extractor tests",
  "type": "project",
  "autoload": {
    "psr-4": {
      "App\\": "src/"
    }
  },
  "autoload-dev": {
    "psr-4": {
      "Tests\\": "tests/"
    }
  }
}
```

- [ ] **Step 2: Create `src/Main.php`**

```php
<?php

namespace App;

use App\Services\Llm;

function bootstrap(): Llm {
    setup_runtime();
    return new Llm();
}
```

- [ ] **Step 3: Create `src/Services/Llm.php`**

```php
<?php

namespace App\Services;

class Llm {
    public function call(string $prompt): string {
        log_event("llm:call");
        return "response";
    }

    public function stream(string $prompt): iterable {
        log_event("llm:stream");
        yield "chunk";
    }
}
```

- [ ] **Step 4: Create `src/Models/User.php`**

```php
<?php

namespace App\Models;

class User {
    public function __construct(
        public readonly string $id,
        public readonly string $name,
    ) {}

    public function display(): string {
        return $this->name;
    }
}
```

- [ ] **Step 5: Create `src/Controllers/HomeController.php`**

```php
<?php

namespace App\Controllers;

use App\Services\Llm;
use App\Models\User;

class HomeController {
    public function __construct(
        private Llm $llm,
    ) {}

    public function handle(User $user): string {
        log_event("home:handle");
        return $this->llm->call("hello, " . $user->display());
    }
}
```

- [ ] **Step 6: Create `tests/LlmTest.php` (must be excluded by is_test_file)**

```php
<?php

namespace Tests;

class LlmTest {
    public function testCallReturnsString(): void {
        assert(true);
    }
}
```

- [ ] **Step 7: Commit the fixture**

```bash
git add tests/fixtures/php_project
git commit -m "test(fixture): add PHP project fixture for extractor tests"
```

---

## Task 16: Integration tests — discover + extract fixture

**Context:** Verify end-to-end that the walker discovers the right files with the right module names, and that the extractor emits expected nodes and edges when run against the fixture.

**Files:**
- Create: `crates/graphify-extract/tests/php_fixture.rs`

- [ ] **Step 1: Create the integration test file**

```rust
use graphify_core::types::{EdgeKind, Language, NodeKind};
use graphify_extract::{
    discover_files_with_psr4, resolver::ModuleResolver, ExtractionResult, LanguageExtractor,
    PhpExtractor,
};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // workspace root
        .join("tests/fixtures/php_project")
}

fn psr4_mappings() -> Vec<(String, String)> {
    let mut resolver = ModuleResolver::new(&fixture_root());
    resolver.load_composer_json(&fixture_root().join("composer.json"));
    resolver.psr4_mappings().to_vec()
}

#[test]
fn discover_php_fixture_finds_four_source_files() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(
        &fixture_root(),
        &[Language::Php],
        "",
        &[],
        &mappings,
    );
    let names: Vec<&str> = files.iter().map(|f| f.module_name.as_str()).collect();
    assert_eq!(
        files.len(),
        4,
        "expected 4 PHP source files (LlmTest.php excluded), got {:?}",
        names
    );
}

#[test]
fn discover_php_fixture_applies_psr4_to_module_names() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(
        &fixture_root(),
        &[Language::Php],
        "",
        &[],
        &mappings,
    );
    let names: Vec<&str> = files.iter().map(|f| f.module_name.as_str()).collect();
    assert!(names.contains(&"App.Main"), "expected App.Main; got {:?}", names);
    assert!(
        names.contains(&"App.Services.Llm"),
        "expected App.Services.Llm; got {:?}",
        names
    );
    assert!(
        names.contains(&"App.Models.User"),
        "expected App.Models.User; got {:?}",
        names
    );
    assert!(
        names.contains(&"App.Controllers.HomeController"),
        "expected App.Controllers.HomeController; got {:?}",
        names
    );
    assert!(
        !names.iter().any(|n| n.contains("LlmTest")),
        "LlmTest.php must be excluded"
    );
}

#[test]
fn home_controller_imports_resolve_to_local_modules() {
    // Extract HomeController.php and verify its imports resolve to local modules.
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(
        &fixture_root(),
        &[Language::Php],
        "",
        &[],
        &mappings,
    );

    // Build resolver with all known modules.
    let mut resolver = ModuleResolver::new(&fixture_root());
    for f in &files {
        resolver.register_module(&f.module_name);
    }

    // Run the extractor on HomeController.
    let ctrl = files
        .iter()
        .find(|f| f.module_name == "App.Controllers.HomeController")
        .expect("HomeController discovered");

    let source = std::fs::read(&ctrl.path).expect("read fixture");
    let extractor = PhpExtractor::new();
    let result: ExtractionResult =
        extractor.extract_file(&ctrl.path, &source, &ctrl.module_name);

    // Verify the raw `use` targets are present as Calls edges.
    let calls_targets: Vec<&str> = result
        .edges
        .iter()
        .filter(|e| e.2.kind == EdgeKind::Calls)
        .map(|e| e.1.as_str())
        .collect();
    assert!(
        calls_targets.contains(&"App.Services.Llm"),
        "use App\\Services\\Llm should Calls-target App.Services.Llm; got {:?}",
        calls_targets
    );
    assert!(
        calls_targets.contains(&"App.Models.User"),
        "use App\\Models\\User should Calls-target App.Models.User; got {:?}",
        calls_targets
    );

    // Feed each raw target through the resolver and verify is_local=true.
    for raw_target in ["App.Services.Llm", "App.Models.User"] {
        let (resolved, is_local, _conf) =
            resolver.resolve(raw_target, &ctrl.module_name, false);
        assert_eq!(resolved, raw_target, "resolver must be identity for dot-form");
        assert!(is_local, "{} must resolve to local", raw_target);
    }
}

#[test]
fn home_controller_extracts_class_and_method_nodes() {
    let mappings = psr4_mappings();
    let files = discover_files_with_psr4(
        &fixture_root(),
        &[Language::Php],
        "",
        &[],
        &mappings,
    );

    let ctrl = files
        .iter()
        .find(|f| f.module_name == "App.Controllers.HomeController")
        .expect("HomeController");

    let source = std::fs::read(&ctrl.path).expect("read fixture");
    let extractor = PhpExtractor::new();
    let result = extractor.extract_file(&ctrl.path, &source, &ctrl.module_name);

    let class = result
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Class && n.id == "App.Controllers.HomeController.HomeController")
        .expect("class node");
    assert_eq!(class.language, Language::Php);

    let method = result
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Method && n.id == "App.Controllers.HomeController.HomeController.handle")
        .expect("handle method node");
    assert_eq!(method.language, Language::Php);
}
```

- [ ] **Step 2: Run the integration tests**

```bash
cargo test -p graphify-extract --test php_fixture
```

Expected: all 4 pass. If any fail, read the assertion messages — likely causes are (a) the walker was invoked but tests/ directory was not excluded because it's not in `DEFAULT_EXCLUDES` … actually, `tests` IS in `DEFAULT_EXCLUDES` (walker.rs:10). Double-check. (b) the constructor in HomeController uses `private Llm $llm` with `readonly` — tree-sitter-php 0.23 should handle it; if it chokes, simplify the constructor to a plain property.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-extract/tests/php_fixture.rs
git commit -m "test(php): add integration tests against tests/fixtures/php_project"
```

---

## Task 17: End-to-end CLI test — `graphify run` against the fixture

**Context:** Smoke-test the full pipeline through the CLI binary, asserting the resulting `analysis.json` contains expected nodes and local edges.

**Files:**
- Create: `crates/graphify-cli/tests/php_e2e.rs` (or extend an existing E2E test file if one exists)

- [ ] **Step 1: Check for existing E2E test conventions**

```bash
ls crates/graphify-cli/tests/
```

If a file like `run_pipeline.rs` or similar exists, extend it. Otherwise create a new file.

- [ ] **Step 2: Write the E2E test**

Create `crates/graphify-cli/tests/php_e2e.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn graphify_bin() -> PathBuf {
    workspace_root().join("target/debug/graphify")
}

fn ensure_binary_built() {
    if !graphify_bin().exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "graphify-cli"])
            .current_dir(workspace_root())
            .status()
            .expect("cargo build");
        assert!(status.success(), "cargo build failed");
    }
}

#[test]
fn graphify_run_against_php_fixture_produces_local_imports_edge() {
    ensure_binary_built();

    let fixture_root = workspace_root().join("tests/fixtures/php_project");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output_dir = tmp.path().join("report");
    std::fs::create_dir_all(&output_dir).expect("mkdir");

    // Write a minimal graphify.toml in tmp pointing at the fixture.
    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[settings]
output = "{}"
exclude = ["vendor"]
format = ["json"]

[[project]]
name = "php-fixture"
repo = "{}"
lang = ["php"]
local_prefix = ""
"#,
            output_dir.to_string_lossy(),
            fixture_root.to_string_lossy(),
        ),
    )
    .expect("write config");

    // Run the pipeline.
    let output = Command::new(graphify_bin())
        .args(["run", "--config"])
        .arg(&config_path)
        .output()
        .expect("run graphify");

    assert!(
        output.status.success(),
        "graphify run failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Read the produced analysis.json for the project.
    let analysis_path = output_dir.join("php-fixture/analysis.json");
    assert!(
        analysis_path.exists(),
        "analysis.json must exist at {:?}",
        analysis_path
    );

    let analysis_text = std::fs::read_to_string(&analysis_path).expect("read analysis.json");

    // Spot-check: the fixture's HomeController should appear, and App.Services.Llm
    // should be reachable as a local module.
    assert!(
        analysis_text.contains("App.Controllers.HomeController"),
        "analysis.json must reference App.Controllers.HomeController"
    );
    assert!(
        analysis_text.contains("App.Services.Llm"),
        "analysis.json must reference App.Services.Llm"
    );
    assert!(
        analysis_text.contains("App.Models.User"),
        "analysis.json must reference App.Models.User"
    );
}
```

If tests for the binary don't already build the binary via a `build.rs` or a setup step elsewhere, the `ensure_binary_built` helper ensures the binary is present.

- [ ] **Step 3: Run the E2E test**

```bash
cargo test -p graphify-cli --test php_e2e -- --nocapture
```

Expected: pass. If it fails because the binary built in debug mode doesn't exist, the helper builds it on first run — subsequent runs will be fast.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-cli/tests/php_e2e.rs
git commit -m "test(cli): e2e pipeline test against PHP fixture"
```

---

## Task 18: Update `CLAUDE.md`

**Context:** The project's contributor guide must reflect the new language and convention set.

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update "What is Graphify"**

In `CLAUDE.md`, update the opening paragraph of "## What is Graphify":

```
Graphify is a Rust CLI tool for architectural analysis of codebases. It extracts dependencies from Python, TypeScript, Go, Rust, and PHP source code using tree-sitter AST parsing, builds knowledge graphs with petgraph, and generates structured reports identifying architectural hotspots, circular dependencies, and community clusters.
```

- [ ] **Step 2: Add `php.rs` to the "Key modules" table**

In the "### Key modules" section, add the PHP row alongside the existing extractors:

```
| `crates/graphify-extract/src/php.rs` | PHP extractor (namespace, use, class/interface/trait/enum/function, calls) |
```

Also extend the description of `resolver.rs`:

```
| `crates/graphify-extract/src/resolver.rs` | Module resolver (Python relative w/ `is_package`, TS path aliases, Go go.mod, PHP PSR-4 via composer.json) |
```

And extend `walker.rs`:

```
| `crates/graphify-extract/src/walker.rs` | File discovery + dir exclusion + `is_package` detection + PSR-4 path translation for PHP |
```

- [ ] **Step 3: Add bullets to "## Conventions"**

Append these bullets to the existing conventions list:

```
- PHP PSR-4 mapping loaded from `composer.json` (`autoload.psr-4` + `autoload-dev.psr-4`); longest-prefix match wins; namespaces normalized `\` → `.`
- PHP test files excluded: `*Test.php` (PHPUnit convention)
- PHP confidence: `use X\Y\Z` → 1.0 / Extracted (fully qualified); bare calls 0.7 / Inferred (same as Go/Python)
- PHP method id scheme: `{module}.{ClassName}.{method}`
- PhpExtractor never sets `is_package = true` (PHP has no package entry-point equivalent)
- `graphify_extract::walker::discover_files_with_psr4` is the PSR-4-aware discovery entry; `discover_files` remains a thin wrapper for non-PHP projects
```

- [ ] **Step 4: Update "## Configuration" example**

The configuration example in `CLAUDE.md` already shows `lang = ["python"]` with comment. Either leave as-is (still valid) or add a PHP example block below:

```toml
[[project]]
name = "my-laravel-app"
repo = "./apps/my-laravel-app"
lang = ["php"]
local_prefix = ""   # PSR-4 via composer.json handles module naming
```

- [ ] **Step 5: Update "## Design docs" section**

Add the FEAT-019 spec + plan references at the appropriate position:

```
- **FEAT-019 spec**: `docs/superpowers/specs/2026-04-15-feat-019-php-support-design.md`
- **FEAT-019 plan**: `docs/superpowers/plans/2026-04-15-feat-019-php-support.md`
```

- [ ] **Step 6: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): document PHP support, PSR-4 conventions, and discover_files_with_psr4"
```

---

## Task 19: Final verification — fmt, clippy, full test suite, smoke test

**Context:** Run all CI gates and a quick smoke test before declaring the feature done.

- [ ] **Step 1: Format**

```bash
cargo fmt --all
git diff --quiet || echo "⚠️ fmt changed files — review and commit"
```

If fmt modified any files, review the diff; if acceptable:

```bash
git add -u
git commit -m "style: cargo fmt --all"
```

- [ ] **Step 2: Clippy (workspace, deny warnings — matches CI)**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: zero warnings. If any surface, fix them inline (they'll usually be minor — unused import, needless `clone`, etc.). After fixing, re-run and commit.

- [ ] **Step 3: Full test suite**

```bash
cargo test --workspace
```

Expected: all tests green. Target count: baseline (493 before FEAT-019) + ≥25 new PHP tests = ≥518 total.

- [ ] **Step 4: Smoke test against the fixture via the built binary**

```bash
cargo build --release -p graphify-cli
./target/release/graphify init --output /tmp/graphify-php-smoke/graphify.toml
```

Edit `/tmp/graphify-php-smoke/graphify.toml` to point at the fixture:

```toml
[settings]
output = "/tmp/graphify-php-smoke/report"
format = ["json", "md"]

[[project]]
name = "php-fixture"
repo = "./tests/fixtures/php_project"
lang = ["php"]
local_prefix = ""
```

(Run from the graphify repo root so the relative path resolves.)

```bash
cd /Users/cleitonparis/ai/graphify
./target/release/graphify run --config /tmp/graphify-php-smoke/graphify.toml
cat /tmp/graphify-php-smoke/report/php-fixture/analysis.json | head -40
```

Expected: runs without error. `analysis.json` contains `App.Controllers.HomeController`, `App.Services.Llm`, `App.Models.User`, `App.Main` as nodes. The architecture report markdown (`architecture_report.md`) should also generate without errors.

- [ ] **Step 5: Verify HTML/Obsidian/Neo4j reports render PHP nodes**

```bash
rm -rf /tmp/graphify-php-smoke/report
# Edit /tmp/graphify-php-smoke/graphify.toml to add: format = ["json", "md", "html", "neo4j", "obsidian"]
./target/release/graphify run --config /tmp/graphify-php-smoke/graphify.toml
ls /tmp/graphify-php-smoke/report/php-fixture/
```

Expected: `graph.json`, `analysis.json`, `architecture_report.md`, `architecture_graph.html`, `graph.cypher`, `obsidian_vault/` all present and non-empty.

- [ ] **Step 6: Final commit — mark FEAT-019 complete**

If any small adjustments were made in Steps 1-3, ensure they're committed. Create the task in TaskNotes and mark it ready for close:

```bash
# If tn CLI is installed:
tn add FEAT-019 --title "Add PHP language support" --status done \
  --related-spec "docs/superpowers/specs/2026-04-15-feat-019-php-support-design.md" \
  --related-plan "docs/superpowers/plans/2026-04-15-feat-019-php-support.md"
```

Or if tn isn't set up, create the task file manually per `docs/TaskNotes/` convention.

- [ ] **Step 7: Push**

```bash
git push origin main
```

(Only if the user has confirmed they want a push — per the solo-dev workflow, confirm before pushing.)

---

## Done

FEAT-019 is complete when:
- `cargo fmt --all --check` passes
- `cargo clippy --workspace -- -D warnings` passes
- `cargo test --workspace` passes with ≥25 new PHP tests
- Smoke test against `tests/fixtures/php_project` produces a valid `analysis.json` with local edges
- `CLAUDE.md` reflects PHP support
- TaskNotes entry FEAT-019 is closed
