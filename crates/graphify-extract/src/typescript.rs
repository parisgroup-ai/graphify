use crate::lang::{
    ExtractionResult, LanguageExtractor, NamedImportEntry, ReExportEntry, ReExportSpec,
};
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
///
/// FEAT-026 behaviour:
///   - Named / default imports populate a [`NamedImportEntry`] with the
///     upstream specifier names. The pipeline walks each specifier through
///     the re-export graph to the canonical declaration module and emits
///     one `Imports` edge per canonical target (deduped via
///     `CodeGraph::add_edge` weight increment). No module-level edge is
///     emitted from the extractor in this branch — the pipeline owns it.
///   - `import * as ns from '…'` and side-effect imports (`import 'x'`)
///     keep the pre-FEAT-026 behaviour of a single barrel edge, because
///     there are no specifiers to fan out.
fn extract_import_statement(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    let Some(source_node) = node.child_by_field_name("source") else {
        return;
    };
    let raw = source_node.utf8_text(source).unwrap_or("");
    let target = raw.trim_matches(|c| c == '\'' || c == '"');
    if target.is_empty() {
        return;
    }

    // Collect specifier names (upstream-facing, not the local alias) and
    // detect whether the statement is namespace / side-effect / type-only.
    let mut specs: Vec<String> = Vec::new();
    let mut has_namespace = false;
    let mut has_clause = false;
    let mut is_type_only = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // `import type { Foo } from '...'` — the grammar exposes a `type`
        // token directly under the import_statement.
        if child.kind() == "type" {
            is_type_only = true;
            continue;
        }
        if child.kind() != "import_clause" {
            continue;
        }
        has_clause = true;
        let mut inner = child.walk();
        for clause_child in child.children(&mut inner) {
            match clause_child.kind() {
                // Default import: `import foo from 'x'` — upstream name is
                // the literal `"default"`.
                "identifier" => {
                    specs.push("default".to_owned());
                }
                // Named imports: `import { a, b as c, type D } from 'x'`
                "named_imports" => {
                    let mut ni = clause_child.walk();
                    for spec in clause_child.children(&mut ni) {
                        if spec.kind() != "import_specifier" {
                            continue;
                        }
                        // Upstream lookup uses the source-side name. The
                        // `name` field is the source-side name; an `alias`
                        // field is present when the binding is renamed.
                        // For bare `import { Foo }`, `name` = `Foo` and no
                        // alias — we still want "Foo".
                        let upstream = spec
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                            .unwrap_or("");
                        if !upstream.is_empty() {
                            specs.push(upstream.to_owned());
                        }
                    }
                }
                // Namespace import: `import * as ns from 'x'` — no
                // specifiers to fan out; keep the single barrel edge.
                "namespace_import" => {
                    has_namespace = true;
                }
                _ => {}
            }
        }
    }

    // Namespace imports, side-effect imports, and imports with no parseable
    // specifiers keep the pre-FEAT-026 single-edge behaviour.
    if has_namespace || !has_clause || specs.is_empty() {
        result.edges.push((
            module_name.to_owned(),
            target.to_owned(),
            Edge::imports(line),
        ));
        return;
    }

    result.named_imports.push(NamedImportEntry {
        from_module: module_name.to_owned(),
        raw_target: target.to_owned(),
        line,
        specs,
        is_type_only,
    });
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

        // FEAT-021: detect `export * from '…'` — it has no export_clause child,
        // just a `*` token directly under the export_statement.
        let mut is_star = false;
        {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.kind() == "*" {
                    is_star = true;
                    break;
                }
            }
        }

        // Also emit Defines edges for each re-exported symbol AND record the
        // re-export metadata for the project-wide barrel-collapse pass.
        // `export { foo, bar as baz } from './mod'` defines `foo` and `baz`
        // in the current module's public API.
        let mut specs: Vec<ReExportSpec> = Vec::new();

        let mut outer = node.walk();
        for child in node.children(&mut outer) {
            if child.kind() == "export_clause" {
                let mut inner = child.walk();
                for specifier in child.children(&mut inner) {
                    if specifier.kind() == "export_specifier" {
                        // `name` field holds the exported-from-source name;
                        // `alias` field (when present) holds the renamed
                        // publication name. Fall back gracefully when the
                        // grammar omits `name`.
                        let exported_name = specifier
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                            .unwrap_or("")
                            .to_owned();
                        let local_name = specifier
                            .child_by_field_name("alias")
                            .and_then(|n| n.utf8_text(source).ok())
                            .unwrap_or(&exported_name)
                            .to_owned();

                        if !local_name.is_empty() {
                            let symbol_id = format!("{}.{}", module_name, local_name);
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

                        if !exported_name.is_empty() && !local_name.is_empty() {
                            specs.push(ReExportSpec {
                                exported_name,
                                local_name,
                            });
                        }
                    }
                }
            }
        }

        if !target.is_empty() && (is_star || !specs.is_empty()) {
            result.reexports.push(ReExportEntry {
                from_module: module_name.to_owned(),
                raw_target: target.to_owned(),
                line,
                specs,
                is_star,
            });
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
    fn named_import_captures_named_import_entry() {
        // FEAT-026: the extractor no longer emits a module-level Imports
        // edge for named imports — it captures a NamedImportEntry and lets
        // the pipeline fan the edge out to the canonical module per
        // specifier via the re-export graph.
        let result = extract("import { api } from '@/lib/api';\n");
        let direct_imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "@/lib/api")
            .collect();
        assert!(
            direct_imports.is_empty(),
            "named imports no longer emit direct Imports edges, got {:?}",
            direct_imports
        );
        assert_eq!(
            result.named_imports.len(),
            1,
            "expected 1 NamedImportEntry, got {:?}",
            result.named_imports
        );
        let entry = &result.named_imports[0];
        assert_eq!(entry.raw_target, "@/lib/api");
        assert_eq!(entry.specs, vec!["api".to_string()]);
        assert!(!entry.is_type_only);
    }

    // -----------------------------------------------------------------------
    // Default import
    // -----------------------------------------------------------------------

    #[test]
    fn default_import_captures_named_import_entry() {
        // FEAT-026: `import React from 'react'` now goes through the
        // NamedImportEntry path with the synthetic spec name "default" so
        // the pipeline can walk `export { default } from '…'` chains.
        let result = extract("import React from 'react';\n");
        let direct_imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "react")
            .collect();
        assert!(
            direct_imports.is_empty(),
            "default imports no longer emit direct Imports edges, got {:?}",
            direct_imports
        );
        assert_eq!(result.named_imports.len(), 1);
        let entry = &result.named_imports[0];
        assert_eq!(entry.raw_target, "react");
        assert_eq!(entry.specs, vec!["default".to_string()]);
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
        // FEAT-026: named imports go through the NamedImportEntry path, so
        // use a namespace import here — it still emits a direct Imports edge
        // from the extractor and is the clearest place to assert that
        // extractor-level confidence stays 1.0 / Extracted.
        use graphify_core::types::ConfidenceKind;
        let result = extract("import * as api from '@/lib/api';\n");
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

    // -----------------------------------------------------------------------
    // FEAT-021: re-export metadata capture
    // -----------------------------------------------------------------------

    #[test]
    fn named_reexport_captures_reexport_entry() {
        let result = extract("export { foo, bar } from './baz';\n");
        assert_eq!(result.reexports.len(), 1, "expected exactly one reexport");
        let entry = &result.reexports[0];
        assert_eq!(entry.from_module, "module");
        assert_eq!(entry.raw_target, "./baz");
        assert!(!entry.is_star);
        assert_eq!(entry.specs.len(), 2);
        assert_eq!(entry.specs[0].exported_name, "foo");
        assert_eq!(entry.specs[0].local_name, "foo");
        assert_eq!(entry.specs[1].exported_name, "bar");
        assert_eq!(entry.specs[1].local_name, "bar");
    }

    #[test]
    fn aliased_reexport_records_both_names() {
        let result = extract("export { foo as Bar } from './baz';\n");
        let entry = result.reexports.first().expect("reexport captured");
        assert_eq!(entry.specs.len(), 1);
        assert_eq!(entry.specs[0].exported_name, "foo");
        assert_eq!(entry.specs[0].local_name, "Bar");
        // The existing Defines edge behaviour (alias wins) must still hold.
        let defines: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Defines && t == "module.Bar")
            .collect();
        assert_eq!(defines.len(), 1, "alias becomes the published symbol");
    }

    #[test]
    fn star_reexport_captures_star_entry_with_no_specs() {
        let result = extract("export * from './barrel';\n");
        let entry = result
            .reexports
            .first()
            .expect("star reexport must be captured");
        assert!(entry.is_star);
        assert!(entry.specs.is_empty());
        assert_eq!(entry.raw_target, "./barrel");
    }

    // -----------------------------------------------------------------------
    // FEAT-026: named-import specifier capture
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_named_imports_capture_all_specs_under_one_entry() {
        let result = extract("import { Foo, Bar } from './entities';\n");
        assert_eq!(result.named_imports.len(), 1);
        let entry = &result.named_imports[0];
        assert_eq!(entry.raw_target, "./entities");
        assert_eq!(entry.specs, vec!["Foo".to_string(), "Bar".to_string()]);
    }

    #[test]
    fn aliased_named_import_keys_on_upstream_name() {
        // `import { Foo as MyFoo }` should record "Foo" — the re-export graph
        // lookup uses the upstream name, not the local alias.
        let result = extract("import { Foo as MyFoo } from './entities';\n");
        assert_eq!(result.named_imports.len(), 1);
        assert_eq!(result.named_imports[0].specs, vec!["Foo".to_string()]);
    }

    #[test]
    fn default_and_named_combined_capture_both_specs() {
        // `import React, { useState } from 'react'` captures both a
        // "default" spec and the named "useState" spec.
        let result = extract("import React, { useState } from 'react';\n");
        assert_eq!(result.named_imports.len(), 1);
        let specs: HashSet<String> = result.named_imports[0].specs.iter().cloned().collect();
        assert!(specs.contains("default"));
        assert!(specs.contains("useState"));
    }

    #[test]
    fn type_only_named_import_marks_is_type_only() {
        let result = extract("import type { Foo } from './foo';\n");
        assert_eq!(result.named_imports.len(), 1);
        assert!(result.named_imports[0].is_type_only);
    }

    #[test]
    fn namespace_import_still_emits_direct_edge() {
        // `import * as X from '…'` has no specifiers to fan out — keep the
        // pre-FEAT-026 single barrel edge.
        let result = extract("import * as api from '@/lib/api';\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "@/lib/api")
            .collect();
        assert_eq!(imports.len(), 1);
        assert!(result.named_imports.is_empty());
    }

    #[test]
    fn side_effect_import_still_emits_direct_edge() {
        // `import 'x'` — no clause, no specifiers — keep the single edge.
        let result = extract("import './polyfills';\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "./polyfills")
            .collect();
        assert_eq!(imports.len(), 1);
        assert!(result.named_imports.is_empty());
    }

    #[test]
    fn plain_imports_do_not_produce_reexport_entries() {
        let result = extract("import { foo } from './bar';\n");
        assert!(
            result.reexports.is_empty(),
            "plain imports should not populate reexports"
        );
    }

    #[test]
    fn local_export_does_not_produce_reexport_entry() {
        // `export function foo() {}` is not a re-export — no `from` clause.
        let result = extract("export function foo() {}\n");
        assert!(result.reexports.is_empty());
    }

    #[test]
    fn extensions_returns_ts_and_tsx() {
        let ext = TypeScriptExtractor::new();
        assert!(ext.extensions().contains(&"ts"));
        assert!(ext.extensions().contains(&"tsx"));
    }
}
