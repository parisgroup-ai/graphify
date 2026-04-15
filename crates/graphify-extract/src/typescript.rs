use crate::lang::{ExtractionResult, LanguageExtractor};
use graphify_core::types::{Edge, Language, Node, NodeKind};
use std::collections::HashSet;
use std::path::Path;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// TypeScriptExtractor
// ---------------------------------------------------------------------------

/// Extracts nodes and edges from TypeScript source files using tree-sitter.
pub struct TypeScriptExtractor;

impl TypeScriptExtractor {
    pub fn new() -> Self {
        TypeScriptExtractor
    }
}

impl Default for TypeScriptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageExtractor for TypeScriptExtractor {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }

    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        // Build a fresh Parser per call — Parser is not Send.
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Failed to load TypeScript grammar");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();

        // Add the module node for this file.
        result.nodes.push(Node::module(
            module_name,
            path,
            Language::TypeScript,
            1,
            true,
        ));

        // Issue #3: build a file-local set of names bound by top-level imports
        // (and CommonJS `require` declarations). `extract_calls_recursive` then
        // only emits a `Calls` edge when the callee is one of those bindings —
        // same-file helpers, function parameters, and JS globals no longer
        // pollute the graph with phantom nodes.
        let root = tree.root_node();
        let imported = collect_imported_bindings(root, source);

        // Walk the root children to extract statements.
        let mut cursor = root.walk();
        for node in root.children(&mut cursor) {
            match node.kind() {
                "import_statement" => {
                    extract_import_statement(node, source, module_name, &mut result);
                }
                "export_statement" => {
                    extract_export_statement(
                        node,
                        source,
                        module_name,
                        path,
                        &imported,
                        &mut result,
                    );
                }
                "lexical_declaration" | "variable_declaration" => {
                    // Handles `const x = require('./util')`
                    extract_require_calls(node, source, module_name, &mut result);
                    extract_calls_recursive(node, source, module_name, &imported, &mut result);
                }
                "expression_statement" => {
                    extract_require_calls(node, source, module_name, &mut result);
                    extract_calls_recursive(node, source, module_name, &imported, &mut result);
                }
                "function_declaration" => {
                    extract_function_declaration(
                        node,
                        source,
                        module_name,
                        path,
                        &imported,
                        &mut result,
                    );
                }
                "class_declaration" => {
                    extract_class_declaration(
                        node,
                        source,
                        module_name,
                        path,
                        &imported,
                        &mut result,
                    );
                }
                _ => {
                    // Scan anything else for bare function calls.
                    extract_calls_recursive(node, source, module_name, &imported, &mut result);
                }
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Import binding collection (issue #3)
// ---------------------------------------------------------------------------

/// Collect the set of names bound by top-level ES6 imports and CommonJS
/// `require` declarations. The result is the local alias actually visible
/// inside this file — `import { foo as bar }` binds `bar`, not `foo`.
///
/// Binding forms covered:
/// - `import foo from 'x'`                     → `foo`
/// - `import { a, b as c } from 'x'`           → `a`, `c`
/// - `import * as ns from 'x'`                 → `ns`
/// - `import type { T } from 'x'`              → `T`
/// - `import 'x'` (side-effect only)           → nothing
/// - `const foo = require('x')`                → `foo`
/// - `const { a, b: c } = require('x')`        → `a`, `c`
/// - `export { foo } from 'x'` (re-export)     → nothing (doesn't bind locally)
fn collect_imported_bindings(root: tree_sitter::Node, source: &[u8]) -> HashSet<String> {
    let mut bindings = HashSet::new();
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        match node.kind() {
            "import_statement" => {
                collect_from_import_statement(node, source, &mut bindings);
            }
            "lexical_declaration" | "variable_declaration" => {
                collect_from_require_declaration(node, source, &mut bindings);
            }
            _ => {}
        }
    }
    bindings
}

fn collect_from_import_statement(
    node: tree_sitter::Node,
    source: &[u8],
    bindings: &mut HashSet<String>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "import_clause" {
            continue;
        }
        let mut inner = child.walk();
        for clause_child in child.children(&mut inner) {
            match clause_child.kind() {
                // Default import: `import foo from 'x'`
                "identifier" => {
                    if let Ok(name) = clause_child.utf8_text(source) {
                        bindings.insert(name.to_owned());
                    }
                }
                // Named imports: `import { a, b as c } from 'x'`
                "named_imports" => {
                    let mut ni = clause_child.walk();
                    for spec in clause_child.children(&mut ni) {
                        if spec.kind() != "import_specifier" {
                            continue;
                        }
                        // Prefer alias over name — local binding uses the alias.
                        let bind_name = spec
                            .child_by_field_name("alias")
                            .or_else(|| spec.child_by_field_name("name"))
                            .and_then(|n| n.utf8_text(source).ok());
                        if let Some(name) = bind_name {
                            if !name.is_empty() {
                                bindings.insert(name.to_owned());
                            }
                        }
                    }
                }
                // Namespace import: `import * as ns from 'x'`
                "namespace_import" => {
                    let mut ii = clause_child.walk();
                    for n in clause_child.children(&mut ii) {
                        if n.kind() == "identifier" {
                            if let Ok(name) = n.utf8_text(source) {
                                bindings.insert(name.to_owned());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn collect_from_require_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    bindings: &mut HashSet<String>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let is_require = child
            .child_by_field_name("value")
            .map(|v| {
                v.kind() == "call_expression"
                    && v.child_by_field_name("function")
                        .and_then(|f| f.utf8_text(source).ok())
                        == Some("require")
            })
            .unwrap_or(false);
        if !is_require {
            continue;
        }
        if let Some(name_node) = child.child_by_field_name("name") {
            collect_binding_names(name_node, source, bindings);
        }
    }
}

fn collect_binding_names(node: tree_sitter::Node, source: &[u8], bindings: &mut HashSet<String>) {
    match node.kind() {
        "identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                bindings.insert(name.to_owned());
            }
        }
        "object_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    // `const { a } = ...` binds `a`.
                    "shorthand_property_identifier_pattern" => {
                        if let Ok(name) = child.utf8_text(source) {
                            bindings.insert(name.to_owned());
                        }
                    }
                    // `const { a: b } = ...` binds `b` (the value side).
                    "pair_pattern" => {
                        if let Some(value) = child.child_by_field_name("value") {
                            collect_binding_names(value, source, bindings);
                        }
                    }
                    _ => {}
                }
            }
        }
        "array_pattern" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_binding_names(child, source, bindings);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Import extraction helpers
// ---------------------------------------------------------------------------

/// Handle ES6 `import` statements:
///   - `import { api } from '@/lib/api';`      → Imports edge to `@/lib/api`
///   - `import React from 'react';`             → Imports edge to `react`
///   - `import * as fs from 'fs';`             → Imports edge to `fs`
fn extract_import_statement(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    // The `source` field on `import_statement` holds the string literal for the module path.
    if let Some(source_node) = node.child_by_field_name("source") {
        let raw = source_node.utf8_text(source).unwrap_or("");
        let target = raw.trim_matches(|c| c == '\'' || c == '"');
        if !target.is_empty() {
            result.edges.push((
                module_name.to_owned(),
                target.to_owned(),
                Edge::imports(line),
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Export statement extraction
// ---------------------------------------------------------------------------

/// Handle `export` statements:
///   - `export { foo } from './bar';`       → Imports edge to `./bar`
///   - `export function createUser() {}`    → Defines edge + Function node
///   - `export class UserService {}`        → Defines edge + Class node
///   - `export default function foo() {}`   → Defines edge + Function node
///   - `export default class Foo {}`        → Defines edge + Class node
fn extract_export_statement(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    path: &Path,
    imported: &HashSet<String>,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    // Check for re-export: `export { foo } from './bar';`
    // The `source` field is present for re-exports.
    if let Some(source_node) = node.child_by_field_name("source") {
        let raw = source_node.utf8_text(source).unwrap_or("");
        let target = raw.trim_matches(|c| c == '\'' || c == '"');
        if !target.is_empty() {
            result.edges.push((
                module_name.to_owned(),
                target.to_owned(),
                Edge::imports(line),
            ));
        }

        // Also emit Defines edges for each re-exported symbol.
        // `export { foo, bar as baz } from './mod'` defines `foo` and `baz`
        // in the current module's public API.
        let mut outer = node.walk();
        for child in node.children(&mut outer) {
            if child.kind() == "export_clause" {
                let mut inner = child.walk();
                for specifier in child.children(&mut inner) {
                    if specifier.kind() == "export_specifier" {
                        // Use the alias if present, otherwise the original name.
                        let exported_name = specifier
                            .child_by_field_name("alias")
                            .or_else(|| specifier.child_by_field_name("name"))
                            .and_then(|n| n.utf8_text(source).ok())
                            .unwrap_or("");
                        if !exported_name.is_empty() {
                            let symbol_id = format!("{}.{}", module_name, exported_name);
                            result.nodes.push(Node::symbol(
                                &symbol_id,
                                NodeKind::Function,
                                path,
                                Language::TypeScript,
                                line,
                                true,
                            ));
                            result.edges.push((
                                module_name.to_owned(),
                                symbol_id,
                                Edge::defines(line),
                            ));
                        }
                    }
                }
            }
        }

        return;
    }

    // Otherwise look at the declaration child (if any).
    // Walk children to find the declaration.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" | "function" => {
                extract_function_declaration(child, source, module_name, path, imported, result);
            }
            "class_declaration" | "class" => {
                extract_class_declaration(child, source, module_name, path, imported, result);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Definition extraction helpers
// ---------------------------------------------------------------------------

/// Handle `function funcName(...) { ... }` (top-level or exported).
fn extract_function_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    path: &Path,
    imported: &HashSet<String>,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    // The function name is in the `name` field.
    let func_name = if let Some(name_node) = node.child_by_field_name("name") {
        name_node.utf8_text(source).unwrap_or("").to_owned()
    } else {
        find_identifier_child(node, source)
    };

    if func_name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, func_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Function,
        path,
        Language::TypeScript,
        line,
        true,
    ));

    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Scan the function body for call sites.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "statement_block" {
            extract_calls_recursive(child, source, module_name, imported, result);
        }
    }
}

/// Handle `class ClassName { ... }` (top-level or exported).
fn extract_class_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    path: &Path,
    imported: &HashSet<String>,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    let class_name = if let Some(name_node) = node.child_by_field_name("name") {
        name_node.utf8_text(source).unwrap_or("").to_owned()
    } else {
        find_identifier_child(node, source)
    };

    if class_name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, class_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Class,
        path,
        Language::TypeScript,
        line,
        true,
    ));

    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Scan the class body for call sites.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_body" {
            extract_calls_recursive(child, source, module_name, imported, result);
        }
    }
}

// ---------------------------------------------------------------------------
// require() extraction
// ---------------------------------------------------------------------------

/// Scan a node's subtree for `require('./util')` calls and emit Imports edges.
/// This is used for CommonJS-style imports in variable declarations.
fn extract_require_calls(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // Look for call_expression nodes with function == "require".
    if node.kind() == "call_expression" {
        if let Some(func_child) = node.child_by_field_name("function") {
            if func_child.utf8_text(source).unwrap_or("") == "require" {
                // The argument list contains the path string.
                if let Some(args_node) = node.child_by_field_name("arguments") {
                    let mut arg_cursor = args_node.walk();
                    for arg in args_node.children(&mut arg_cursor) {
                        match arg.kind() {
                            "string" | "template_string" => {
                                let raw = arg.utf8_text(source).unwrap_or("");
                                let target =
                                    raw.trim_matches(|c| c == '\'' || c == '"' || c == '`');
                                if !target.is_empty() {
                                    let line = node.start_position().row + 1;
                                    result.edges.push((
                                        module_name.to_owned(),
                                        target.to_owned(),
                                        Edge::imports(line),
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                return; // Don't emit a Calls edge for require
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_require_calls(child, source, module_name, result);
    }
}

// ---------------------------------------------------------------------------
// Call site extraction
// ---------------------------------------------------------------------------

/// Recursively scan `node` and emit `Calls` edges for every bare function call
/// whose callee is a top-level import binding (see `collect_imported_bindings`).
///
/// Same-file helpers, function parameters, and JS globals are filtered out:
/// they never enter `imported`, so no edge is emitted. Edges are keyed by the
/// local alias used in the source (`import { foo as bar } ...; bar()` keys on
/// `bar`), and carry confidence 0.9 / Extracted — the symbol is a known import,
/// not a guess.
fn extract_calls_recursive(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    imported: &HashSet<String>,
    result: &mut ExtractionResult,
) {
    if node.kind() == "call_expression" {
        if let Some(func_child) = node.child_by_field_name("function") {
            if func_child.kind() == "identifier" {
                let callee = func_child.utf8_text(source).unwrap_or("");
                // Skip `require` — handled by extract_require_calls.
                if callee != "require" && !callee.is_empty() && imported.contains(callee) {
                    let line = node.start_position().row + 1;
                    result.edges.push((
                        module_name.to_owned(),
                        callee.to_owned(),
                        Edge::calls(line)
                            .with_confidence(0.9, graphify_core::types::ConfidenceKind::Extracted),
                    ));
                }
            }
            // If func_child is member_expression (e.g. `obj.method()`), skip.
        }
    }

    // Recurse into all children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(child, source, module_name, imported, result);
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Return the text of the first `identifier` child of `node`.
fn find_identifier_child(node: tree_sitter::Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).unwrap_or("").to_owned();
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::EdgeKind;

    fn extract(source: &str) -> ExtractionResult {
        let extractor = TypeScriptExtractor::new();
        extractor.extract_file(Path::new("src/module.ts"), source.as_bytes(), "module")
    }

    // -----------------------------------------------------------------------
    // Module node
    // -----------------------------------------------------------------------

    #[test]
    fn module_node_is_created() {
        let result = extract("const x = 1;\n");
        let module_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Module)
            .expect("module node must be created");
        assert_eq!(module_node.id, "module");
        assert!(module_node.is_local);
    }

    // -----------------------------------------------------------------------
    // Named import
    // -----------------------------------------------------------------------

    #[test]
    fn named_import_produces_imports_edge() {
        let result = extract("import { api } from '@/lib/api';\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "@/lib/api")
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected 1 Imports edge to @/lib/api, got {:?}",
            imports
        );
    }

    // -----------------------------------------------------------------------
    // Default import
    // -----------------------------------------------------------------------

    #[test]
    fn default_import_produces_imports_edge() {
        let result = extract("import React from 'react';\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "react")
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected 1 Imports edge to react, got {:?}",
            imports
        );
    }

    // -----------------------------------------------------------------------
    // Export function
    // -----------------------------------------------------------------------

    #[test]
    fn export_function_produces_defines_edge() {
        let result = extract("export function createUser() {}\n");
        let defines: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "module.createUser")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected 1 Defines edge to module.createUser, got {:?}",
            defines
        );
    }

    #[test]
    fn export_function_produces_function_node() {
        let result = extract("export function createUser() {}\n");
        let func_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Function && n.id == "module.createUser")
            .expect("Function node module.createUser must be created");
        assert!(func_node.is_local);
    }

    // -----------------------------------------------------------------------
    // Export class
    // -----------------------------------------------------------------------

    #[test]
    fn export_class_produces_defines_edge() {
        let result = extract("export class UserService {}\n");
        let defines: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "module.UserService")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected 1 Defines edge to module.UserService, got {:?}",
            defines
        );
    }

    #[test]
    fn export_class_produces_class_node() {
        let result = extract("export class UserService {}\n");
        let class_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Class && n.id == "module.UserService")
            .expect("Class node module.UserService must be created");
        assert!(class_node.is_local);
    }

    // -----------------------------------------------------------------------
    // require()
    // -----------------------------------------------------------------------

    #[test]
    fn require_produces_imports_edge() {
        let result = extract("const x = require('./util');\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "./util")
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected 1 Imports edge to ./util, got {:?}",
            imports
        );
    }

    // -----------------------------------------------------------------------
    // Re-export
    // -----------------------------------------------------------------------

    #[test]
    fn re_export_produces_imports_edge() {
        let result = extract("export { foo } from './bar';\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "./bar")
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected 1 Imports edge to ./bar, got {:?}",
            imports
        );
    }

    #[test]
    fn re_export_produces_defines_edge() {
        let result = extract("export { foo } from './bar';\n");
        let defines: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "module.foo")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected 1 Defines edge to module.foo, got {:?}",
            defines
        );
    }

    #[test]
    fn re_export_produces_symbol_node() {
        let result = extract("export { foo } from './bar';\n");
        let sym = result
            .nodes
            .iter()
            .find(|n| n.id == "module.foo" && n.kind == NodeKind::Function)
            .expect("symbol node module.foo must be created for re-export");
        assert!(sym.is_local);
    }

    #[test]
    fn re_export_multiple_symbols_produces_defines_edges() {
        let result = extract("export { foo, bar } from './baz';\n");
        let defines: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(
            defines.len(),
            2,
            "expected 2 Defines edges for re-export of foo and bar, got {:?}",
            defines
        );
        assert!(defines.iter().any(|(_, t, _)| t == "module.foo"));
        assert!(defines.iter().any(|(_, t, _)| t == "module.bar"));
    }

    #[test]
    fn re_export_with_alias_uses_alias_name() {
        let result = extract("export { foo as myFoo } from './bar';\n");
        let defines: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "module.myFoo")
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "expected 1 Defines edge to module.myFoo (alias), got {:?}",
            defines
        );
        // Should NOT define module.foo (the original name).
        let original: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "module.foo")
            .collect();
        assert!(
            original.is_empty(),
            "should not define module.foo when alias is present, got {:?}",
            original
        );
    }

    // -----------------------------------------------------------------------
    // Call expression (issue #3 — imported callees only)
    // -----------------------------------------------------------------------

    #[test]
    fn imported_callee_produces_calls_edge() {
        let result = extract("import { createUser } from './users';\ncreateUser(data);\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Calls && t == "createUser")
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "expected 1 Calls edge to imported createUser, got {:?}",
            calls
        );
    }

    #[test]
    fn same_file_helper_call_produces_no_edge() {
        // `sleep` is defined in this file and not imported — no Calls edge.
        let result = extract(
            "function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }\nsleep(100);\n",
        );
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Calls && t == "sleep")
            .collect();
        assert!(
            calls.is_empty(),
            "same-file helper must not emit Calls edge, got {:?}",
            calls
        );
    }

    #[test]
    fn js_globals_produce_no_calls_edge() {
        // JS globals (setTimeout, String, Array, ...) are never imported.
        let result = extract("setTimeout(fn, 100);\nString(123);\nArray.from([]);\n");
        let call_targets: Vec<&str> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .map(|(_, t, _)| t.as_str())
            .collect();
        assert!(
            !call_targets.contains(&"setTimeout"),
            "setTimeout global must be skipped, got {:?}",
            call_targets
        );
        assert!(
            !call_targets.contains(&"String"),
            "String global must be skipped, got {:?}",
            call_targets
        );
    }

    #[test]
    fn aliased_import_calls_are_keyed_by_alias() {
        let result = extract("import { foo as bar } from './x';\nbar(1);\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .collect();
        // Edge targets the local alias `bar`, not the imported name `foo`.
        assert!(
            calls.iter().any(|(_, t, _)| t == "bar"),
            "expected Calls edge to alias 'bar', got {:?}",
            calls
        );
        assert!(
            !calls.iter().any(|(_, t, _)| t == "foo"),
            "should not key on imported name 'foo', got {:?}",
            calls
        );
    }

    #[test]
    fn namespace_import_is_bound() {
        let result = extract("import * as util from './util';\nutil('x');\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Calls && t == "util")
            .collect();
        assert_eq!(calls.len(), 1, "namespace import must bind the alias");
    }

    #[test]
    fn require_destructuring_binds_callees() {
        let result = extract("const { foo, bar: baz } = require('./x');\nfoo();\nbaz();\n");
        let call_targets: Vec<&str> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .map(|(_, t, _)| t.as_str())
            .collect();
        assert!(
            call_targets.contains(&"foo"),
            "expected Calls edge to destructured 'foo', got {:?}",
            call_targets
        );
        assert!(
            call_targets.contains(&"baz"),
            "expected Calls edge to renamed 'baz', got {:?}",
            call_targets
        );
    }

    #[test]
    fn re_export_does_not_bind_locally() {
        // `export { foo } from 'x'` forwards the symbol but does NOT bind `foo`
        // in this file's scope — calling `foo()` here would be a TypeError.
        let result = extract("export { foo } from './x';\nfoo(1);\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Calls && t == "foo")
            .collect();
        assert!(
            calls.is_empty(),
            "re-export must not create a local binding, got {:?}",
            calls
        );
    }

    #[test]
    fn method_calls_are_skipped() {
        let result = extract("import { obj } from './obj';\nobj.method(data);\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .collect();
        // `obj.method()` is a member_expression — should be skipped regardless
        // of whether `obj` is imported.
        assert!(
            calls.iter().all(|(_, t, _)| t != "method"),
            "obj.method() should be skipped, got {:?}",
            calls
        );
    }

    // -----------------------------------------------------------------------
    // Confidence
    // -----------------------------------------------------------------------

    #[test]
    fn imported_callee_has_extracted_confidence() {
        use graphify_core::types::ConfidenceKind;
        let result = extract("import { createUser } from './users';\ncreateUser(data);\n");
        let call_edge = result
            .edges
            .iter()
            .find(|(_, t, e)| e.kind == EdgeKind::Calls && t == "createUser")
            .expect("should have Calls edge to createUser");
        assert_eq!(call_edge.2.confidence, 0.9);
        assert_eq!(call_edge.2.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn import_edges_have_extracted_confidence() {
        use graphify_core::types::ConfidenceKind;
        let result = extract("import { api } from '@/lib/api';\n");
        let import_edge = result
            .edges
            .iter()
            .find(|(_, _, e)| e.kind == EdgeKind::Imports)
            .expect("should have Imports edge");
        assert_eq!(import_edge.2.confidence, 1.0);
        assert_eq!(import_edge.2.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn defines_edges_have_extracted_confidence() {
        use graphify_core::types::ConfidenceKind;
        let result = extract("export function createUser() {}\n");
        let def_edge = result
            .edges
            .iter()
            .find(|(_, _, e)| e.kind == EdgeKind::Defines)
            .expect("should have Defines edge");
        assert_eq!(def_edge.2.confidence, 1.0);
        assert_eq!(def_edge.2.confidence_kind, ConfidenceKind::Extracted);
    }

    // -----------------------------------------------------------------------
    // Extensions
    // -----------------------------------------------------------------------

    #[test]
    fn extensions_returns_ts_and_tsx() {
        let ext = TypeScriptExtractor::new();
        assert!(ext.extensions().contains(&"ts"));
        assert!(ext.extensions().contains(&"tsx"));
    }
}
