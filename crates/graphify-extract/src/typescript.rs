use std::path::Path;
use tree_sitter::Parser;
use graphify_core::types::{Edge, Language, Node, NodeKind};
use crate::lang::{ExtractionResult, LanguageExtractor};

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

        // Walk the root children to extract statements.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for node in root.children(&mut cursor) {
            match node.kind() {
                "import_statement" => {
                    extract_import_statement(node, source, module_name, &mut result);
                }
                "export_statement" => {
                    extract_export_statement(node, source, module_name, path, &mut result);
                }
                "lexical_declaration" | "variable_declaration" => {
                    // Handles `const x = require('./util')`
                    extract_require_calls(node, source, module_name, &mut result);
                    extract_calls_recursive(node, source, module_name, &mut result);
                }
                "expression_statement" => {
                    extract_require_calls(node, source, module_name, &mut result);
                    extract_calls_recursive(node, source, module_name, &mut result);
                }
                "function_declaration" => {
                    extract_function_declaration(node, source, module_name, path, &mut result);
                }
                "class_declaration" => {
                    extract_class_declaration(node, source, module_name, path, &mut result);
                }
                _ => {
                    // Scan anything else for bare function calls.
                    extract_calls_recursive(node, source, module_name, &mut result);
                }
            }
        }

        result
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
                            let symbol_id =
                                format!("{}.{}", module_name, exported_name);
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
                extract_function_declaration(child, source, module_name, path, result);
            }
            "class_declaration" | "class" => {
                extract_class_declaration(child, source, module_name, path, result);
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

    result.edges.push((
        module_name.to_owned(),
        symbol_id,
        Edge::defines(line),
    ));

    // Scan the function body for call sites.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "statement_block" {
            extract_calls_recursive(child, source, module_name, result);
        }
    }
}

/// Handle `class ClassName { ... }` (top-level or exported).
fn extract_class_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    path: &Path,
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

    result.edges.push((
        module_name.to_owned(),
        symbol_id,
        Edge::defines(line),
    ));

    // Scan the class body for call sites.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_body" {
            extract_calls_recursive(child, source, module_name, result);
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
                                let target = raw.trim_matches(|c| c == '\'' || c == '"' || c == '`');
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

/// Recursively scan `node` and emit Calls edges for every bare function call
/// (i.e. `call_expression` nodes whose `function` field is an `identifier`,
/// not a `member_expression` or similar).
fn extract_calls_recursive(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    if node.kind() == "call_expression" {
        if let Some(func_child) = node.child_by_field_name("function") {
            if func_child.kind() == "identifier" {
                let callee = func_child.utf8_text(source).unwrap_or("");
                // Skip `require` — handled by extract_require_calls.
                if callee != "require" && !callee.is_empty() {
                    let line = node.start_position().row + 1;
                    result.edges.push((
                        module_name.to_owned(),
                        callee.to_owned(),
                        Edge::calls(line),
                    ));
                }
            }
            // If func_child is member_expression (e.g. `obj.method()`), skip.
        }
    }

    // Recurse into all children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(child, source, module_name, result);
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
        extractor.extract_file(
            Path::new("src/module.ts"),
            source.as_bytes(),
            "module",
        )
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
    // Call expression
    // -----------------------------------------------------------------------

    #[test]
    fn bare_call_expression_produces_calls_edge() {
        let result = extract("createUser(data);\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Calls && t == "createUser")
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "expected 1 Calls edge to createUser, got {:?}",
            calls
        );
    }

    #[test]
    fn method_calls_are_skipped() {
        let result = extract("obj.method(data);\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .collect();
        // `obj.method()` is a member_expression — should be skipped.
        assert!(
            calls.iter().all(|(_, t, _)| t != "method"),
            "obj.method() should be skipped, got {:?}",
            calls
        );
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
