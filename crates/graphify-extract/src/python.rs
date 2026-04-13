use crate::lang::{ExtractionResult, LanguageExtractor};
use graphify_core::types::{Edge, Language, Node, NodeKind};
use std::path::Path;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// PythonExtractor
// ---------------------------------------------------------------------------

/// Extracts nodes and edges from Python source files using tree-sitter.
pub struct PythonExtractor;

impl PythonExtractor {
    pub fn new() -> Self {
        PythonExtractor
    }
}

impl Default for PythonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageExtractor for PythonExtractor {
    fn extensions(&self) -> &[&str] {
        &["py"]
    }

    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        // Build a fresh Parser per call — Parser is not Send.
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("Failed to load Python grammar");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();

        // Add the module node for this file.
        result
            .nodes
            .push(Node::module(module_name, path, Language::Python, 1, true));

        // Walk the root children to extract statements.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for node in root.children(&mut cursor) {
            match node.kind() {
                "import_statement" => {
                    extract_import_statement(node, source, module_name, &mut result);
                }
                "import_from_statement" => {
                    extract_import_from_statement(node, source, module_name, &mut result);
                }
                "function_definition" => {
                    extract_function_definition(node, source, module_name, path, &mut result);
                }
                "class_definition" => {
                    extract_class_definition(node, source, module_name, path, &mut result);
                }
                _ => {
                    // For top-level expression statements or other constructs,
                    // scan for bare function calls.
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

/// Handle `import x`, `import x.y.z`, `import x, y`.
fn extract_import_statement(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" | "aliased_import" => {
                // For aliased_import, take the first dotted_name child.
                let target_name = if child.kind() == "aliased_import" {
                    child
                        .child(0)
                        .and_then(|c| c.utf8_text(source).ok())
                        .unwrap_or("")
                        .to_owned()
                } else {
                    child.utf8_text(source).unwrap_or("").to_owned()
                };
                if !target_name.is_empty() {
                    result
                        .edges
                        .push((module_name.to_owned(), target_name, Edge::imports(line)));
                }
            }
            _ => {}
        }
    }
}

/// Handle `from x import y`, `from x import y, z`, `from . import y`.
fn extract_import_from_statement(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    // Collect children in order to find the module and imported names.
    let mut children = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        children.push(child);
    }

    // The from-module is either a dotted_name or a relative_import (for `from .`).
    // Find it after the `from` keyword.
    let mut from_module: Option<String> = None;
    let mut past_from = false;
    let mut import_names: Vec<(String, usize)> = Vec::new();
    let mut past_import = false;

    for child in &children {
        match child.kind() {
            "from" => {
                past_from = true;
            }
            "dotted_name" if past_from && !past_import => {
                from_module = Some(child.utf8_text(source).unwrap_or("").to_owned());
            }
            "relative_import" if past_from && !past_import => {
                // `from . import y` — keep the raw relative marker.
                from_module = Some(child.utf8_text(source).unwrap_or(".").to_owned());
            }
            "import" => {
                past_import = true;
            }
            "dotted_name" if past_import => {
                let name = child.utf8_text(source).unwrap_or("").to_owned();
                if !name.is_empty() {
                    import_names.push((name, child.start_position().row + 1));
                }
            }
            "aliased_import" if past_import => {
                // `from x import y as z` — take the first dotted_name.
                if let Some(first) = child.child(0) {
                    let name = first.utf8_text(source).unwrap_or("").to_owned();
                    if !name.is_empty() {
                        import_names.push((name, child.start_position().row + 1));
                    }
                }
            }
            "wildcard_import" if past_import => {
                // `from x import *` — emit just the module import.
            }
            _ => {}
        }
    }

    if let Some(ref fm) = from_module {
        // Emit an Imports edge to the module.
        result
            .edges
            .push((module_name.to_owned(), fm.clone(), Edge::imports(line)));

        // Emit Calls edges for each imported name (qualified).
        for (name, name_line) in &import_names {
            let qualified = if fm.starts_with('.') {
                // Relative import: keep as-is (resolver handles it later).
                format!("{}.{}", fm, name)
            } else {
                format!("{}.{}", fm, name)
            };
            result
                .edges
                .push((module_name.to_owned(), qualified, Edge::calls(*name_line)));
        }
    }
}

// ---------------------------------------------------------------------------
// Definition extraction helpers
// ---------------------------------------------------------------------------

/// Handle `def func_name(...): ...` at the top level.
fn extract_function_definition(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    path: &Path,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    // The function name is the first `identifier` child.
    let func_name = find_identifier_child(node, source);
    if func_name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, func_name);

    // Add a Function node.
    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Function,
        path,
        Language::Python,
        line,
        true,
    ));

    // Add a Defines edge from the module to the symbol.
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Also scan the function body for bare call sites.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block" {
            extract_calls_recursive(child, source, module_name, result);
        }
    }
}

/// Handle `class ClassName: ...` at the top level.
fn extract_class_definition(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    path: &Path,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;

    let class_name = find_identifier_child(node, source);
    if class_name.is_empty() {
        return;
    }

    let symbol_id = format!("{}.{}", module_name, class_name);

    // Add a Class node.
    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Class,
        path,
        Language::Python,
        line,
        true,
    ));

    // Add a Defines edge.
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Scan the class body for bare call sites.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block" {
            extract_calls_recursive(child, source, module_name, result);
        }
    }
}

// ---------------------------------------------------------------------------
// Call site extraction
// ---------------------------------------------------------------------------

/// Recursively scan `node` and emit Calls edges for every bare function call
/// (i.e. `call` nodes whose function child is an `identifier`, not an
/// `attribute`).
fn extract_calls_recursive(
    node: tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    if node.kind() == "call" {
        // First child of a `call` node is the function expression.
        if let Some(func_child) = node.child(0) {
            if func_child.kind() == "identifier" {
                let callee = func_child.utf8_text(source).unwrap_or("").to_owned();
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
            // If func_child.kind() == "attribute", it's a method call — skip.
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
        let extractor = PythonExtractor::new();
        extractor.extract_file(Path::new("app/test.py"), source.as_bytes(), "app.test")
    }

    // -----------------------------------------------------------------------
    // Module node
    // -----------------------------------------------------------------------

    #[test]
    fn module_node_is_created() {
        let result = extract("x = 1\n");
        let module_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Module)
            .expect("module node must be created");
        assert_eq!(module_node.id, "app.test");
        assert!(module_node.is_local);
    }

    // -----------------------------------------------------------------------
    // import statements
    // -----------------------------------------------------------------------

    #[test]
    fn import_statement_produces_imports_edges() {
        let result = extract("import os\nimport json\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(
            imports.len(),
            2,
            "expected 2 Imports edges, got {:?}",
            imports
        );
        let targets: Vec<&str> = imports.iter().map(|(_, t, _)| t.as_str()).collect();
        assert!(targets.contains(&"os"), "expected 'os'");
        assert!(targets.contains(&"json"), "expected 'json'");
    }

    #[test]
    fn import_statement_source_is_module() {
        let result = extract("import os\n");
        let edge = result
            .edges
            .iter()
            .find(|(_, _, e)| e.kind == EdgeKind::Imports)
            .unwrap();
        assert_eq!(edge.0, "app.test");
    }

    // -----------------------------------------------------------------------
    // from … import
    // -----------------------------------------------------------------------

    #[test]
    fn from_import_produces_imports_edge_to_module() {
        let result = extract("from app.services.llm import call_llm\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Imports && t == "app.services.llm")
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected Imports edge to app.services.llm"
        );
    }

    #[test]
    fn from_import_produces_calls_edge_for_imported_name() {
        let result = extract("from app.services.llm import call_llm\n");
        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, t, e)| e.kind == EdgeKind::Calls && t == "app.services.llm.call_llm")
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "expected Calls edge to app.services.llm.call_llm"
        );
    }

    #[test]
    fn from_relative_import_kept_raw() {
        let result = extract("from . import utils\n");
        let imports: Vec<_> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        // The target starts with "." (relative, unresolved).
        assert!(
            imports[0].1.starts_with('.'),
            "expected relative import target, got {}",
            imports[0].1
        );
    }

    // -----------------------------------------------------------------------
    // Function definition
    // -----------------------------------------------------------------------

    #[test]
    fn function_definition_creates_function_node() {
        let result = extract("def my_func():\n    pass\n");
        let func_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Function)
            .expect("function node must be created");
        assert_eq!(func_node.id, "app.test.my_func");
        assert!(func_node.is_local);
    }

    #[test]
    fn function_definition_creates_defines_edge() {
        let result = extract("def my_func():\n    pass\n");
        let edge = result
            .edges
            .iter()
            .find(|(s, t, e)| {
                e.kind == EdgeKind::Defines && s == "app.test" && t == "app.test.my_func"
            })
            .expect("Defines edge from module to function");
        assert_eq!(edge.2.kind, EdgeKind::Defines);
    }

    // -----------------------------------------------------------------------
    // Class definition
    // -----------------------------------------------------------------------

    #[test]
    fn class_definition_creates_class_node() {
        let result = extract("class MyClass:\n    pass\n");
        let class_node = result
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Class)
            .expect("class node must be created");
        assert_eq!(class_node.id, "app.test.MyClass");
    }

    #[test]
    fn class_definition_creates_defines_edge() {
        let result = extract("class MyClass:\n    pass\n");
        let edge = result
            .edges
            .iter()
            .find(|(s, t, e)| {
                e.kind == EdgeKind::Defines && s == "app.test" && t == "app.test.MyClass"
            })
            .expect("Defines edge from module to class");
        assert_eq!(edge.2.kind, EdgeKind::Defines);
    }

    // -----------------------------------------------------------------------
    // Call sites
    // -----------------------------------------------------------------------

    #[test]
    fn bare_call_sites_produce_calls_edges() {
        let result = extract("def foo():\n    bar()\n    baz(x, y)\n");
        let call_targets: Vec<&str> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .map(|(_, t, _)| t.as_str())
            .collect();
        assert!(
            call_targets.contains(&"bar"),
            "expected Calls edge to 'bar'"
        );
        assert!(
            call_targets.contains(&"baz"),
            "expected Calls edge to 'baz'"
        );
    }

    #[test]
    fn method_calls_are_skipped() {
        let result = extract("def foo():\n    self.method()\n    obj.call(x)\n");
        let call_targets: Vec<&str> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .map(|(_, t, _)| t.as_str())
            .collect();
        // Neither "method" nor "call" should appear as targets from attribute calls.
        assert!(
            !call_targets.contains(&"method"),
            "self.method() should be skipped"
        );
        assert!(
            !call_targets.iter().any(|t| *t == "call"),
            "obj.call() should be skipped"
        );
    }

    #[test]
    fn top_level_bare_calls_are_captured() {
        let result = extract("setup()\nconfigure(debug=True)\n");
        let call_targets: Vec<&str> = result
            .edges
            .iter()
            .filter(|(_, _, e)| e.kind == EdgeKind::Calls)
            .map(|(_, t, _)| t.as_str())
            .collect();
        assert!(
            call_targets.contains(&"setup"),
            "expected Calls edge to 'setup'"
        );
        assert!(
            call_targets.contains(&"configure"),
            "expected Calls edge to 'configure'"
        );
    }

    // -----------------------------------------------------------------------
    // Extensions
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Confidence
    // -----------------------------------------------------------------------

    #[test]
    fn bare_call_sites_have_inferred_confidence() {
        use graphify_core::types::ConfidenceKind;
        let result = extract("def foo():\n    bar()\n");
        let call_edge = result
            .edges
            .iter()
            .find(|(_, t, e)| e.kind == EdgeKind::Calls && t == "bar")
            .expect("should have Calls edge to bar");
        assert_eq!(call_edge.2.confidence, 0.7);
        assert_eq!(call_edge.2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn import_edges_have_extracted_confidence() {
        use graphify_core::types::ConfidenceKind;
        let result = extract("import os\n");
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
        let result = extract("def my_func():\n    pass\n");
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
    fn extensions_returns_py() {
        let ext = PythonExtractor::new();
        assert_eq!(ext.extensions(), &["py"]);
    }
}
