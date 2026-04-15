use crate::lang::{ExtractionResult, LanguageExtractor};
use graphify_core::types::{Edge, Language, Node, NodeKind};
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
    }
}

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
    }
}

// ---------------------------------------------------------------------------
// `use` declarations
// ---------------------------------------------------------------------------

/// Handle `use X\Y\Z;`, `use X\Y as Z;`, `use function X\y;`, `use const X\Y;`,
/// and group forms `use X\{A, B};`.
///
/// Tree-sitter-php AST structure for group use:
///   namespace_use_declaration
///     use
///     namespace_name      ← group prefix (sibling of namespace_use_group)
///     \
///     namespace_use_group
///       { namespace_use_clause ... }
///     ;
fn extract_namespace_use(
    node: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    result: &mut ExtractionResult,
) {
    let line = node.start_position().row + 1;
    let mut cursor = node.walk();

    // Collect children first so we can look for a group prefix.
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

    // Check if this is a group use: find namespace_name sibling + namespace_use_group.
    let group_prefix: Option<String> = children.iter().find_map(|c| {
        if c.kind() == "namespace_name" {
            Some(c.utf8_text(source).unwrap_or("").to_owned())
        } else {
            None
        }
    });

    for child in &children {
        match child.kind() {
            "namespace_use_clause" => {
                emit_use_clause(child, source, module_name, line, None, result);
            }
            "namespace_use_group" => {
                extract_namespace_use_group(
                    child,
                    source,
                    module_name,
                    line,
                    group_prefix.as_deref(),
                    result,
                );
            }
            _ => {}
        }
    }
}

/// Emit edges for a single `namespace_use_clause` node (optionally prefixed by
/// a group path like `App\Services`).
///
/// A clause may contain a leading `function` or `const` keyword (for
/// `use function X\y` / `use const X\Y`) which we skip — only the
/// `qualified_name` or `name` matters.
fn emit_use_clause(
    clause: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    group_prefix: Option<&str>,
    result: &mut ExtractionResult,
) {
    let mut qualified: Option<String> = None;
    let mut cursor = clause.walk();
    for sub in clause.children(&mut cursor) {
        match sub.kind() {
            "qualified_name" | "name" => {
                if qualified.is_none() {
                    qualified = Some(sub.utf8_text(source).unwrap_or("").to_owned());
                }
            }
            _ => {}
        }
    }

    let Some(raw) = qualified else { return };
    let raw = raw.trim_start_matches('\\').to_owned();
    if raw.is_empty() {
        return;
    }

    let full = match group_prefix {
        Some(prefix) => format!("{}\\{}", prefix.trim_start_matches('\\'), raw),
        None => raw,
    };

    let symbol_id = full.replace('\\', ".");

    let module_id = match symbol_id.rsplit_once('.') {
        Some((parent, _)) => parent.to_owned(),
        None => symbol_id.clone(),
    };

    if !module_id.is_empty() && module_id != symbol_id {
        result
            .edges
            .push((module_name.to_owned(), module_id, Edge::imports(line)));
    } else {
        result.edges.push((
            module_name.to_owned(),
            symbol_id.clone(),
            Edge::imports(line),
        ));
    }

    result
        .edges
        .push((module_name.to_owned(), symbol_id, Edge::calls(line)));
}

/// Handle a `namespace_use_group` node. Iterates inner clauses using the
/// group prefix that was already extracted by `extract_namespace_use`.
fn extract_namespace_use_group(
    group: &tree_sitter::Node,
    source: &[u8],
    module_name: &str,
    line: usize,
    group_prefix: Option<&str>,
    result: &mut ExtractionResult,
) {
    let mut cursor = group.walk();
    for child in group.children(&mut cursor) {
        if child.kind() == "namespace_use_clause" {
            emit_use_clause(&child, source, module_name, line, group_prefix, result);
        }
    }
}

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
        kind.clone(),
        path,
        Language::Php,
        line,
        true,
    ));
    result.edges.push((
        module_name.to_owned(),
        symbol_id.clone(),
        Edge::defines(line),
    ));

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
        }
    }

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

    #[test]
    fn simple_use_produces_imports_and_calls_edges() {
        use graphify_core::types::EdgeKind;
        let r = extract("<?php\nuse App\\Services\\Llm;\n");
        let imports: Vec<_> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "expected 1 Imports edge, got {:?}",
            imports
        );
        assert_eq!(imports[0].0, "App.Main");
        assert_eq!(imports[0].1, "App.Services");
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
        assert!(
            !calls.contains(&"App.Services.L"),
            "alias must not become a target"
        );
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
        assert_eq!(
            defines[0].0, "App.Main",
            "Defines edge must come from module"
        );
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
        assert!(
            !calls.contains(&"call"),
            "static call must be skipped; got {:?}",
            calls
        );
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
        assert!(
            !calls.contains(&"call"),
            "instance call must be skipped; got {:?}",
            calls
        );
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

    #[test]
    fn full_php_file_integration() {
        use graphify_core::types::EdgeKind;
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

        // 1 module + 1 interface + 1 class + 3 methods (Servicer::serve, Llm::serve, Llm::helper) + 1 function = 7 nodes
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
        assert!(
            methods.contains(&"App.Main.Servicer.serve"),
            "got {:?}",
            methods
        );
        assert!(methods.contains(&"App.Main.Llm.serve"), "got {:?}", methods);
        assert!(
            methods.contains(&"App.Main.Llm.helper"),
            "got {:?}",
            methods
        );
        assert_eq!(nodes_by_kind(NodeKind::Function), vec!["App.Main.make_llm"]);

        let imports: Vec<&str> = r
            .edges
            .iter()
            .filter(|e| e.2.kind == EdgeKind::Imports)
            .map(|e| e.1.as_str())
            .collect();
        assert!(
            imports.contains(&"App.Models"),
            "Imports to App.Models; got {:?}",
            imports
        );
        assert!(
            imports.contains(&"App.Logging"),
            "Imports to App.Logging (group); got {:?}",
            imports
        );

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
}
