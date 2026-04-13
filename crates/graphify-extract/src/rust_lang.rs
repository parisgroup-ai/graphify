use crate::lang::{ExtractionResult, LanguageExtractor};
use graphify_core::types::{Edge, Language, Node, NodeKind};
use std::path::Path;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// RustExtractor
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RustExtractor;

impl RustExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageExtractor for RustExtractor {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Failed to load Rust grammar");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();

        // Every file gets a module node.
        result
            .nodes
            .push(Node::module(module_name, path, Language::Rust, 1, true));

        // Walk top-level statements.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "use_declaration" => {
                    extract_use_declaration(&child, source, module_name, &mut result);
                }
                "mod_item" => {
                    extract_mod_item(&child, source, module_name, &mut result);
                }
                "function_item" => {
                    extract_function_item(&child, source, path, module_name, &mut result);
                }
                "struct_item" => {
                    extract_struct_item(&child, source, path, module_name, &mut result);
                }
                "enum_item" => {
                    extract_enum_item(&child, source, path, module_name, &mut result);
                }
                "trait_item" => {
                    extract_trait_item(&child, source, path, module_name, &mut result);
                }
                "impl_item" => {
                    extract_impl_item(&child, source, path, module_name, &mut result);
                }
                "macro_invocation" => {
                    extract_macro_invocation(&child, source, module_name, &mut result);
                }
                _ => {
                    extract_calls_recursive(&child, source, module_name, &mut result);
                }
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Use declaration extraction
// ---------------------------------------------------------------------------

fn extract_use_declaration(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // The `argument` field contains the use path (e.g., scoped_identifier,
    // scoped_use_list, identifier, etc.).
    if let Some(arg) = node.child_by_field_name("argument") {
        let line = node.start_position().row + 1;
        collect_use_paths(&arg, source, module_name, line, result);
    }
}

/// Recursively collect import paths from use declaration arguments.
/// Handles: identifiers, scoped_identifier, scoped_use_list, use_as_clause, use_list.
fn collect_use_paths(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "identifier" | "scoped_identifier" | "crate" | "self" | "super" => {
            let path_str = node.utf8_text(source).unwrap_or("").to_owned();
            if !path_str.is_empty() {
                result
                    .edges
                    .push((module_name.to_owned(), path_str, Edge::imports(line)));
            }
        }
        "use_as_clause" => {
            // `use foo::bar as baz` — the path is the first child.
            if let Some(path_node) = node.child_by_field_name("path") {
                let path_str = path_node.utf8_text(source).unwrap_or("").to_owned();
                if !path_str.is_empty() {
                    result
                        .edges
                        .push((module_name.to_owned(), path_str, Edge::imports(line)));
                }
            }
        }
        "scoped_use_list" => {
            // `use std::{io, fs}` — path prefix + list of imports.
            let prefix = node
                .child_by_field_name("path")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if let Some(list_node) = node.child_by_field_name("list") {
                let mut cursor = list_node.walk();
                for child in list_node.children(&mut cursor) {
                    if !child.is_named() {
                        continue;
                    }
                    match child.kind() {
                        "identifier" | "self" => {
                            let name = child.utf8_text(source).unwrap_or("");
                            if !name.is_empty() {
                                let full_path = if prefix.is_empty() {
                                    name.to_owned()
                                } else {
                                    format!("{}::{}", prefix, name)
                                };
                                result.edges.push((
                                    module_name.to_owned(),
                                    full_path,
                                    Edge::imports(line),
                                ));
                            }
                        }
                        "scoped_identifier" | "scoped_use_list" | "use_as_clause" => {
                            // Nested: reconstruct with prefix.
                            let child_text = child.utf8_text(source).unwrap_or("");
                            if !child_text.is_empty() {
                                let full_path = if prefix.is_empty() {
                                    child_text.to_owned()
                                } else {
                                    format!("{}::{}", prefix, child_text)
                                };
                                result.edges.push((
                                    module_name.to_owned(),
                                    full_path,
                                    Edge::imports(line),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        "use_list" => {
            // Bare use list (rare, e.g. `use {foo, bar}`).
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_use_paths(&child, source, module_name, line, result);
                }
            }
        }
        "use_wildcard" => {
            // `use foo::*` — import the path without the wildcard.
            let text = node.utf8_text(source).unwrap_or("");
            let path = text.strip_suffix("::*").unwrap_or(text);
            if !path.is_empty() {
                result
                    .edges
                    .push((module_name.to_owned(), path.to_owned(), Edge::imports(line)));
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Module declaration
// ---------------------------------------------------------------------------

fn extract_mod_item(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // `mod name;` (file-level) or `mod name { ... }` (inline).
    // Only emit an Imports edge for file-level mod declarations (no body).
    let has_body = node.child_by_field_name("body").is_some();
    if has_body {
        return; // Inline mod — items are already in this file's AST.
    }

    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let mod_name = name_node.utf8_text(source).unwrap_or("");
    if mod_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    result.edges.push((
        module_name.to_owned(),
        mod_name.to_owned(),
        Edge::imports(line),
    ));
}

// ---------------------------------------------------------------------------
// Function item
// ---------------------------------------------------------------------------

fn extract_function_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let func_name = name_node.utf8_text(source).unwrap_or("");
    if func_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    let symbol_id = format!("{}.{}", module_name, func_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Function,
        path,
        Language::Rust,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Scan function body for call sites.
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_recursive(&body, source, module_name, result);
    }
}

// ---------------------------------------------------------------------------
// Struct, Enum, Trait items
// ---------------------------------------------------------------------------

fn extract_struct_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Class, result);
}

fn extract_enum_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Enum, result);
}

fn extract_trait_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    extract_named_type(node, source, path, module_name, NodeKind::Trait, result);
}

fn extract_named_type(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    kind: NodeKind,
    result: &mut ExtractionResult,
) {
    let name_node = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    let type_name = name_node.utf8_text(source).unwrap_or("");
    if type_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    let symbol_id = format!("{}.{}", module_name, type_name);

    result.nodes.push(Node::symbol(
        &symbol_id,
        kind,
        path,
        Language::Rust,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));
}

// ---------------------------------------------------------------------------
// Impl block
// ---------------------------------------------------------------------------

fn extract_impl_item(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // Extract the type being implemented.
    let type_name = node
        .child_by_field_name("type")
        .and_then(|n| {
            // The type field can be type_identifier, generic_type, etc.
            // For generic_type, get the base type.
            if n.kind() == "generic_type" || n.kind() == "scoped_type_identifier" {
                // Walk to find the type_identifier child.
                let mut cursor = n.walk();
                for child in n.children(&mut cursor) {
                    if child.kind() == "type_identifier" {
                        return child.utf8_text(source).ok();
                    }
                }
                n.utf8_text(source).ok()
            } else {
                n.utf8_text(source).ok()
            }
        })
        .unwrap_or("");

    // Walk the body (declaration_list) for function_items.
    let body = match node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_item" {
            let method_name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if method_name.is_empty() {
                continue;
            }

            let line = child.start_position().row + 1;

            let symbol_id = if type_name.is_empty() {
                format!("{}.{}", module_name, method_name)
            } else {
                format!("{}.{}.{}", module_name, type_name, method_name)
            };

            result.nodes.push(Node::symbol(
                &symbol_id,
                NodeKind::Method,
                path,
                Language::Rust,
                line,
                true,
            ));
            result
                .edges
                .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

            // Scan method body for call sites.
            if let Some(fn_body) = child.child_by_field_name("body") {
                extract_calls_recursive(&fn_body, source, module_name, result);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Macro invocation
// ---------------------------------------------------------------------------

fn extract_macro_invocation(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let macro_node = match node.child_by_field_name("macro") {
        Some(n) => n,
        None => return,
    };

    // Only extract simple identifier macros (e.g. `println!`, `vec!`).
    // Skip scoped macros (e.g. `std::println!`).
    if macro_node.kind() != "identifier" {
        return;
    }

    let macro_name = macro_node.utf8_text(source).unwrap_or("");
    if macro_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    result.edges.push((
        module_name.to_owned(),
        macro_name.to_owned(),
        Edge::calls(line).with_confidence(0.6, graphify_core::types::ConfidenceKind::Inferred),
    ));
}

// ---------------------------------------------------------------------------
// Call extraction
// ---------------------------------------------------------------------------

fn extract_calls_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    match node.kind() {
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                // Only extract bare identifier calls (not field_expression or scoped_identifier).
                if func.kind() == "identifier" {
                    let callee = func.utf8_text(source).unwrap_or("").to_owned();
                    if !callee.is_empty() {
                        let line = node.start_position().row + 1;
                        result.edges.push((
                            module_name.to_owned(),
                            callee,
                            Edge::calls(line).with_confidence(
                                0.7,
                                graphify_core::types::ConfidenceKind::Inferred,
                            ),
                        ));
                    }
                }
            }
        }
        "macro_invocation" => {
            extract_macro_invocation(node, source, module_name, result);
        }
        _ => {}
    }

    // Recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_calls_recursive(&child, source, module_name, result);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::types::{ConfidenceKind, EdgeKind};

    fn extract(source: &str) -> ExtractionResult {
        let extractor = RustExtractor::new();
        extractor.extract_file(
            Path::new("src/handler.rs"),
            source.as_bytes(),
            "src.handler",
        )
    }

    #[test]
    fn extensions() {
        let e = RustExtractor::new();
        assert_eq!(e.extensions(), &["rs"]);
    }

    #[test]
    fn module_node_always_created() {
        let r = extract("");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].id, "src.handler");
        assert_eq!(r.nodes[0].kind, NodeKind::Module);
        assert_eq!(r.nodes[0].language, Language::Rust);
    }

    #[test]
    fn simple_use_declaration() {
        let r = extract("use std::io;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "std::io");
    }

    #[test]
    fn use_crate_path() {
        let r = extract("use crate::models::user;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "crate::models::user");
    }

    #[test]
    fn grouped_use() {
        let r = extract("use std::{io, fs};\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"std::io"));
        assert!(imports.contains(&"std::fs"));
    }

    #[test]
    fn use_as_alias() {
        let r = extract("use std::io::Result as IoResult;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "std::io::Result");
    }

    #[test]
    fn mod_declaration_file_level() {
        let r = extract("mod handler;\nmod services;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"handler"));
        assert!(imports.contains(&"services"));
    }

    #[test]
    fn inline_mod_no_import_edge() {
        let r = extract("mod inner { pub fn foo() {} }\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert!(
            imports.is_empty(),
            "inline mod should not produce Imports edge"
        );
    }

    #[test]
    fn function_item() {
        let r = extract("pub fn handle_request() -> Result<()> { Ok(()) }\n");
        let funcs: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].id, "src.handler.handle_request");

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "src.handler.handle_request");
    }

    #[test]
    fn struct_item() {
        let r = extract("pub struct Config {\n    port: u16,\n}\n");
        let structs: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Class)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].id, "src.handler.Config");
    }

    #[test]
    fn enum_item() {
        let r = extract("pub enum AppError {\n    NotFound,\n    Internal,\n}\n");
        let enums: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].id, "src.handler.AppError");
    }

    #[test]
    fn trait_item() {
        let r = extract("pub trait Handler {\n    fn handle(&self);\n}\n");
        let traits: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Trait)
            .collect();
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].id, "src.handler.Handler");
    }

    #[test]
    fn impl_block_methods() {
        let r = extract(
            r#"struct Server { port: u16 }
impl Server {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
    pub fn start(&self) {
        listen(self.port);
    }
}
"#,
        );
        let methods: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(methods.len(), 2);
        let method_ids: Vec<&str> = methods.iter().map(|m| m.id.as_str()).collect();
        assert!(method_ids.contains(&"src.handler.Server.new"));
        assert!(method_ids.contains(&"src.handler.Server.start"));

        // `listen` is a bare call inside start()
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "listen")
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2.confidence, 0.7);
    }

    #[test]
    fn bare_call_expression() {
        let r = extract("fn main() { process(); }\n");
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.2.confidence > 0.6)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "process");
        assert_eq!(calls[0].2.confidence, 0.7);
        assert_eq!(calls[0].2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn method_call_not_extracted_as_bare() {
        let r = extract("fn main() { self.handle(); obj.method(); }\n");
        let bare_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.2.confidence == 0.7)
            .collect();
        assert!(
            bare_calls.is_empty(),
            "field_expression calls should not produce bare Calls edges"
        );
    }

    #[test]
    fn macro_invocation() {
        let r = extract("fn main() { println!(\"hello\"); vec![1, 2, 3]; }\n");
        let macro_calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.2.confidence < 0.7)
            .collect();
        assert!(macro_calls.len() >= 2);
        let names: Vec<&str> = macro_calls.iter().map(|e| e.1.as_str()).collect();
        assert!(names.contains(&"println"));
        assert!(names.contains(&"vec"));
        assert_eq!(macro_calls[0].2.confidence, 0.6);
        assert_eq!(macro_calls[0].2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn import_confidence_is_extracted() {
        let r = extract("use std::io;\n");
        let imp = r
            .edges
            .iter()
            .find(|e| e.2.kind == EdgeKind::Imports)
            .unwrap();
        assert_eq!(imp.2.confidence, 1.0);
        assert_eq!(imp.2.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn full_rust_file() {
        let r = extract(
            r#"use std::io;
use crate::models::User;

pub struct Config {
    host: String,
    port: u16,
}

pub enum AppError {
    NotFound,
    Internal(String),
}

pub trait Handler {
    fn handle(&self) -> Result<(), AppError>;
}

pub fn process(config: &Config) -> Result<(), AppError> {
    validate(config)?;
    Ok(())
}

impl Config {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }
}
"#,
        );

        // 1 module + 1 struct + 1 enum + 1 trait + 1 function + 1 method = 6 nodes
        assert_eq!(r.nodes.len(), 6);

        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"std::io"));
        assert!(imports.contains(&"crate::models::User"));

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 5); // Config, AppError, Handler, process, Config.new

        // `validate` is a bare call inside process()
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls && e.1 == "validate")
            .collect();
        assert_eq!(calls.len(), 1);
    }
}
