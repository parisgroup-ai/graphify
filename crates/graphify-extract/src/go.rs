use crate::lang::{ExtractionResult, LanguageExtractor};
use graphify_core::types::{Edge, Language, Node, NodeKind};
use std::path::Path;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// GoExtractor
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct GoExtractor;

impl GoExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageExtractor for GoExtractor {
    fn extensions(&self) -> &[&str] {
        &["go"]
    }

    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("Failed to load Go grammar");

        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return ExtractionResult::new(),
        };

        let mut result = ExtractionResult::new();

        // Every file gets a module node.
        result
            .nodes
            .push(Node::module(module_name, path, Language::Go, 1, true));

        // Walk top-level statements.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "import_declaration" => {
                    extract_import_declaration(&child, source, module_name, &mut result);
                }
                "function_declaration" => {
                    extract_function_declaration(&child, source, path, module_name, &mut result);
                }
                "method_declaration" => {
                    extract_method_declaration(&child, source, path, module_name, &mut result);
                }
                "type_declaration" => {
                    extract_type_declaration(&child, source, path, module_name, &mut result);
                }
                _ => {
                    // Scan other top-level statements for bare calls.
                    extract_calls_recursive(&child, source, module_name, &mut result);
                }
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Import extraction
// ---------------------------------------------------------------------------

fn extract_import_declaration(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => {
                extract_import_spec(&child, source, module_name, result);
            }
            "import_spec_list" => {
                let mut inner_cursor = child.walk();
                for spec in child.children(&mut inner_cursor) {
                    if spec.kind() == "import_spec" {
                        extract_import_spec(&spec, source, module_name, result);
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_import_spec(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    // The `path` field holds the import path string literal.
    let path_node = match node.child_by_field_name("path") {
        Some(n) => n,
        None => return,
    };

    let raw = path_node.utf8_text(source).unwrap_or("");
    let target = raw.trim_matches(|c: char| c == '"' || c == '`');

    if target.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    result.edges.push((
        module_name.to_owned(),
        target.to_owned(),
        Edge::imports(line),
    ));
}

// ---------------------------------------------------------------------------
// Function declaration
// ---------------------------------------------------------------------------

fn extract_function_declaration(
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
        Language::Go,
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
// Method declaration
// ---------------------------------------------------------------------------

fn extract_method_declaration(
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
    let method_name = name_node.utf8_text(source).unwrap_or("");
    if method_name.is_empty() {
        return;
    }

    // Extract receiver type name for qualified method ID.
    let receiver_type = node
        .child_by_field_name("receiver")
        .and_then(|recv| find_type_identifier(&recv, source))
        .unwrap_or_default();

    let line = node.start_position().row + 1;

    let symbol_id = if receiver_type.is_empty() {
        format!("{}.{}", module_name, method_name)
    } else {
        format!("{}.{}.{}", module_name, receiver_type, method_name)
    };

    result.nodes.push(Node::symbol(
        &symbol_id,
        NodeKind::Method,
        path,
        Language::Go,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));

    // Scan method body for call sites.
    if let Some(body) = node.child_by_field_name("body") {
        extract_calls_recursive(&body, source, module_name, result);
    }
}

/// Walk a receiver parameter list to find the type identifier.
/// Handles both value receivers `(h Handler)` and pointer receivers `(h *Handler)`.
fn find_type_identifier(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            return child.utf8_text(source).ok().map(|s| s.to_owned());
        }
        // Recurse into parameter_declaration, pointer_type, etc.
        if let Some(found) = find_type_identifier(&child, source) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Type declaration
// ---------------------------------------------------------------------------

fn extract_type_declaration(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &Path,
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_spec" {
            extract_type_spec(&child, source, path, module_name, result);
        }
    }
}

fn extract_type_spec(
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
    let type_name = name_node.utf8_text(source).unwrap_or("");
    if type_name.is_empty() {
        return;
    }

    let line = node.start_position().row + 1;
    let symbol_id = format!("{}.{}", module_name, type_name);

    // Determine kind from the type field.
    let type_node = node.child_by_field_name("type");
    let kind = match type_node.as_ref().map(|n| n.kind()) {
        Some("interface_type") => NodeKind::Trait,
        _ => NodeKind::Class, // struct_type and others map to Class
    };

    result.nodes.push(Node::symbol(
        &symbol_id,
        kind,
        path,
        Language::Go,
        line,
        true,
    ));
    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::defines(line)));
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
    if node.kind() == "call_expression" {
        if let Some(func) = node.child_by_field_name("function") {
            // Only extract bare identifier calls (not selector_expression like fmt.Println).
            if func.kind() == "identifier" {
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
        }
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
        let extractor = GoExtractor::new();
        extractor.extract_file(Path::new("cmd/main.go"), source.as_bytes(), "cmd.main")
    }

    #[test]
    fn extensions() {
        let e = GoExtractor::new();
        assert_eq!(e.extensions(), &["go"]);
    }

    #[test]
    fn module_node_always_created() {
        let r = extract("package main\n");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].id, "cmd.main");
        assert_eq!(r.nodes[0].kind, NodeKind::Module);
        assert_eq!(r.nodes[0].language, Language::Go);
    }

    #[test]
    fn single_import() {
        let r = extract("package main\nimport \"fmt\"\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].0, "cmd.main");
        assert_eq!(imports[0].1, "fmt");
    }

    #[test]
    fn grouped_imports() {
        let r = extract(
            r#"package main
import (
    "fmt"
    "os"
    "net/http"
)
"#,
        );
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 3);
        assert!(imports.contains(&"fmt"));
        assert!(imports.contains(&"os"));
        assert!(imports.contains(&"net/http"));
    }

    #[test]
    fn aliased_import() {
        let r = extract(
            r#"package main
import (
    mypkg "github.com/user/repo/pkg"
)
"#,
        );
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].1, "github.com/user/repo/pkg");
    }

    #[test]
    fn function_declaration() {
        let r = extract(
            r#"package main
func NewHandler() *Handler {
    return &Handler{}
}
"#,
        );
        let func_nodes: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Function)
            .collect();
        assert_eq!(func_nodes.len(), 1);
        assert_eq!(func_nodes[0].id, "cmd.main.NewHandler");

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "cmd.main.NewHandler");
    }

    #[test]
    fn method_declaration_with_receiver() {
        let r = extract(
            r#"package main
func (h *Handler) Handle() error {
    return nil
}
"#,
        );
        let methods: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].id, "cmd.main.Handler.Handle");

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .collect();
        assert_eq!(defines.len(), 1);
        assert_eq!(defines[0].1, "cmd.main.Handler.Handle");
    }

    #[test]
    fn struct_type_declaration() {
        let r = extract(
            r#"package main
type Handler struct {
    Name string
}
"#,
        );
        let classes: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].id, "cmd.main.Handler");
    }

    #[test]
    fn interface_type_declaration() {
        let r = extract(
            r#"package main
type Servicer interface {
    Serve() error
}
"#,
        );
        let traits: Vec<_> = r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Trait)
            .collect();
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].id, "cmd.main.Servicer");
    }

    #[test]
    fn bare_call_expression() {
        let r = extract(
            r#"package main
func main() {
    handler := NewHandler()
}
"#,
        );
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "NewHandler");
        assert_eq!(calls[0].2.confidence, 0.7);
        assert_eq!(calls[0].2.confidence_kind, ConfidenceKind::Inferred);
    }

    #[test]
    fn selector_call_not_extracted_as_bare() {
        // fmt.Println is a selector expression, not a bare identifier call.
        let r = extract(
            r#"package main
import "fmt"
func main() {
    fmt.Println("hello")
}
"#,
        );
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .collect();
        assert!(
            calls.is_empty(),
            "selector calls should not produce bare Calls edges: {:?}",
            calls.iter().map(|e| &e.1).collect::<Vec<_>>()
        );
    }

    #[test]
    fn import_confidence_is_extracted() {
        let r = extract("package main\nimport \"fmt\"\n");
        let imp = r
            .edges
            .iter()
            .find(|e| e.2.kind == EdgeKind::Imports)
            .unwrap();
        assert_eq!(imp.2.confidence, 1.0);
        assert_eq!(imp.2.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn defines_confidence_is_extracted() {
        let r = extract("package main\nfunc Foo() {}\n");
        let def = r
            .edges
            .iter()
            .find(|e| e.2.kind == EdgeKind::Defines)
            .unwrap();
        assert_eq!(def.2.confidence, 1.0);
        assert_eq!(def.2.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn full_go_file() {
        let r = extract(
            r#"package main

import (
    "fmt"
    "net/http"
)

type Server struct {
    Port int
}

type Handler interface {
    ServeHTTP(w http.ResponseWriter, r *http.Request)
}

func NewServer(port int) *Server {
    return &Server{Port: port}
}

func (s *Server) Start() {
    fmt.Println("starting")
    listen(s.Port)
}
"#,
        );

        // 1 module + 1 struct + 1 interface + 1 function + 1 method = 5 nodes
        assert_eq!(r.nodes.len(), 5);

        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&"fmt"));
        assert!(imports.contains(&"net/http"));

        let defines: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Defines)
            .map(|e| e.1.as_str())
            .collect();
        assert_eq!(defines.len(), 4); // Server, Handler, NewServer, Start

        // `listen` is a bare call inside Start method body
        let calls: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Calls)
            .map(|e| e.1.as_str())
            .collect();
        assert!(calls.contains(&"listen"));
    }
}
