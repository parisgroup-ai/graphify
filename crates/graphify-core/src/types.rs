use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Python,
    TypeScript,
    Go,
    Rust,
    Php,
}

// ---------------------------------------------------------------------------
// NodeKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Module,
    Function,
    Class,
    Method,
    Trait,
    Enum,
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub language: Language,
    pub line: usize,
    pub is_local: bool,
    /// Alternative module paths through which this node is reachable.
    ///
    /// FEAT-021: populated for TypeScript nodes that are re-exported through
    /// one or more barrel chains (e.g. `export { Course } from './entities'`
    /// inside `./domain/index.ts`). The canonical declaration's `id` is
    /// kept; each extra path the consumer could have imported via is recorded
    /// here so `analysis.json` consumers can see the aliases without the
    /// graph counting them as distinct nodes.
    ///
    /// Empty for other extractors (Python/Go/Rust/PHP) and for TypeScript
    /// nodes that are not reached through any barrel re-export. The field is
    /// `#[serde(skip_serializing_if = "Vec::is_empty", default)]` so the
    /// legacy JSON shape is preserved when no alternatives exist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternative_paths: Vec<String>,
}

impl Node {
    /// Convenience constructor for a module-level node.
    pub fn module(
        id: impl Into<String>,
        file_path: impl Into<PathBuf>,
        language: Language,
        line: usize,
        is_local: bool,
    ) -> Self {
        Self {
            id: id.into(),
            kind: NodeKind::Module,
            file_path: file_path.into(),
            language,
            line,
            is_local,
            alternative_paths: Vec::new(),
        }
    }

    /// Convenience constructor for a symbol node (Function, Class, or Method).
    pub fn symbol(
        id: impl Into<String>,
        kind: NodeKind,
        file_path: impl Into<PathBuf>,
        language: Language,
        line: usize,
        is_local: bool,
    ) -> Self {
        assert!(
            !matches!(kind, NodeKind::Module),
            "Node::symbol() must not be called with NodeKind::Module; use Node::module() instead"
        );
        Self {
            id: id.into(),
            kind,
            file_path: file_path.into(),
            language,
            line,
            is_local,
            alternative_paths: Vec::new(),
        }
    }

    /// Builder-style setter: attach one or more alternative import paths.
    ///
    /// Duplicates are deduplicated in insertion order so the list stays
    /// stable across reruns. Intended for the TypeScript barrel-collapse
    /// pass (FEAT-021); other extractors leave this empty.
    pub fn with_alternative_paths<I, S>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for p in paths {
            let s = p.into();
            if !self.alternative_paths.contains(&s) {
                self.alternative_paths.push(s);
            }
        }
        self
    }
}

// ---------------------------------------------------------------------------
// EdgeKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Imports,
    Defines,
    Calls,
}

// ---------------------------------------------------------------------------
// ConfidenceKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceKind {
    Extracted,
    Inferred,
    Ambiguous,
    /// Target resolves to a package the project declares as intentionally
    /// external (e.g. `drizzle-orm`, `zod`). Semantically it would be
    /// `Ambiguous`, but the consumer has opted it out of the ambiguity
    /// signal via `[[project]].external_stubs` in `graphify.toml`.
    ExpectedExternal,
}

// ---------------------------------------------------------------------------
// Edge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub kind: EdgeKind,
    pub weight: u32,
    pub line: usize,
    pub confidence: f64,
    pub confidence_kind: ConfidenceKind,
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.weight == other.weight
            && self.line == other.line
            && self.confidence.to_bits() == other.confidence.to_bits()
            && self.confidence_kind == other.confidence_kind
    }
}

impl Eq for Edge {}

impl Edge {
    /// Convenience constructor for an `Imports` edge.
    pub fn imports(line: usize) -> Self {
        Self {
            kind: EdgeKind::Imports,
            weight: 1,
            line,
            confidence: 1.0,
            confidence_kind: ConfidenceKind::Extracted,
        }
    }

    /// Convenience constructor for a `Defines` edge.
    pub fn defines(line: usize) -> Self {
        Self {
            kind: EdgeKind::Defines,
            weight: 1,
            line,
            confidence: 1.0,
            confidence_kind: ConfidenceKind::Extracted,
        }
    }

    /// Convenience constructor for a `Calls` edge.
    pub fn calls(line: usize) -> Self {
        Self {
            kind: EdgeKind::Calls,
            weight: 1,
            line,
            confidence: 1.0,
            confidence_kind: ConfidenceKind::Extracted,
        }
    }

    /// Builder method to set confidence score and kind.
    pub fn with_confidence(mut self, score: f64, kind: ConfidenceKind) -> Self {
        self.confidence = score;
        self.confidence_kind = kind;
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_module_node() {
        let node = Node::module(
            "app.services.llm",
            "app/services/llm.py",
            Language::Python,
            1,
            true,
        );
        assert_eq!(node.id, "app.services.llm");
        assert_eq!(node.kind, NodeKind::Module);
        assert_eq!(node.file_path, PathBuf::from("app/services/llm.py"));
        assert_eq!(node.language, Language::Python);
        assert_eq!(node.line, 1);
        assert!(node.is_local);
    }

    #[test]
    fn create_symbol_node_function() {
        let node = Node::symbol(
            "app.utils.helpers.parse",
            NodeKind::Function,
            "app/utils/helpers.py",
            Language::Python,
            42,
            true,
        );
        assert_eq!(node.id, "app.utils.helpers.parse");
        assert_eq!(node.kind, NodeKind::Function);
        assert_eq!(node.line, 42);
        assert!(node.is_local);
    }

    #[test]
    fn create_symbol_node_class() {
        let node = Node::symbol(
            "app.models.User",
            NodeKind::Class,
            "app/models.py",
            Language::Python,
            10,
            true,
        );
        assert_eq!(node.kind, NodeKind::Class);
    }

    #[test]
    fn create_symbol_node_method() {
        let node = Node::symbol(
            "app.models.User.save",
            NodeKind::Method,
            "app/models.py",
            Language::Python,
            20,
            true,
        );
        assert_eq!(node.kind, NodeKind::Method);
    }

    #[test]
    fn create_symbol_node_non_local() {
        let node = Node::module("os", "", Language::Python, 0, false);
        assert!(!node.is_local);
    }

    #[test]
    #[should_panic(expected = "Node::symbol() must not be called with NodeKind::Module")]
    fn symbol_constructor_rejects_module_kind() {
        Node::symbol("bad", NodeKind::Module, "", Language::Python, 0, false);
    }

    #[test]
    fn edge_constructors() {
        let imp = Edge::imports(5);
        assert_eq!(imp.kind, EdgeKind::Imports);
        assert_eq!(imp.weight, 1);
        assert_eq!(imp.line, 5);

        let def = Edge::defines(10);
        assert_eq!(def.kind, EdgeKind::Defines);
        assert_eq!(def.weight, 1);
        assert_eq!(def.line, 10);

        let call = Edge::calls(20);
        assert_eq!(call.kind, EdgeKind::Calls);
        assert_eq!(call.weight, 1);
        assert_eq!(call.line, 20);
    }

    #[test]
    fn node_serialization_roundtrip() {
        let node = Node::module("app.main", "app/main.py", Language::Python, 1, true);
        let json = serde_json::to_string(&node).expect("serialize");
        let restored: Node = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(node, restored);
    }

    #[test]
    fn node_json_contains_expected_fields() {
        let node = Node::symbol(
            "app.services.llm.call",
            NodeKind::Function,
            "app/services/llm.py",
            Language::TypeScript,
            99,
            false,
        );
        let json = serde_json::to_string(&node).expect("serialize");
        assert!(json.contains("\"id\":\"app.services.llm.call\""));
        assert!(json.contains("\"kind\":\"Function\""));
        assert!(json.contains("\"language\":\"TypeScript\""));
        assert!(json.contains("\"line\":99"));
        assert!(json.contains("\"is_local\":false"));
    }

    #[test]
    fn edge_serialization_roundtrip() {
        let edge = Edge::calls(77);
        let json = serde_json::to_string(&edge).expect("serialize");
        let restored: Edge = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(edge, restored);
    }

    #[test]
    fn edge_constructors_default_to_extracted_confidence() {
        let imp = Edge::imports(5);
        assert_eq!(imp.confidence, 1.0);
        assert_eq!(imp.confidence_kind, ConfidenceKind::Extracted);

        let def = Edge::defines(10);
        assert_eq!(def.confidence, 1.0);
        assert_eq!(def.confidence_kind, ConfidenceKind::Extracted);

        let call = Edge::calls(20);
        assert_eq!(call.confidence, 1.0);
        assert_eq!(call.confidence_kind, ConfidenceKind::Extracted);
    }

    #[test]
    fn edge_with_confidence_builder() {
        let edge = Edge::calls(5).with_confidence(0.7, ConfidenceKind::Inferred);
        assert_eq!(edge.confidence, 0.7);
        assert_eq!(edge.confidence_kind, ConfidenceKind::Inferred);
        assert_eq!(edge.kind, EdgeKind::Calls);
        assert_eq!(edge.weight, 1);
        assert_eq!(edge.line, 5);
    }

    #[test]
    fn edge_eq_with_confidence() {
        let a = Edge::imports(1).with_confidence(0.9, ConfidenceKind::Inferred);
        let b = Edge::imports(1).with_confidence(0.9, ConfidenceKind::Inferred);
        assert_eq!(a, b);

        let c = Edge::imports(1).with_confidence(0.8, ConfidenceKind::Inferred);
        assert_ne!(a, c);
    }

    #[test]
    fn edge_serialization_roundtrip_with_confidence() {
        let edge = Edge::calls(77).with_confidence(0.85, ConfidenceKind::Inferred);
        let json = serde_json::to_string(&edge).expect("serialize");
        let restored: Edge = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(edge, restored);
    }

    #[test]
    fn edge_json_contains_confidence_fields() {
        let edge = Edge::imports(1).with_confidence(0.5, ConfidenceKind::Ambiguous);
        let json = serde_json::to_string(&edge).expect("serialize");
        assert!(json.contains("\"confidence\":0.5"));
        assert!(json.contains("\"confidence_kind\":\"Ambiguous\""));
    }

    #[test]
    fn confidence_kind_variants() {
        let kinds = vec![
            (ConfidenceKind::Extracted, "\"Extracted\""),
            (ConfidenceKind::Inferred, "\"Inferred\""),
            (ConfidenceKind::Ambiguous, "\"Ambiguous\""),
            (ConfidenceKind::ExpectedExternal, "\"ExpectedExternal\""),
        ];
        for (kind, expected) in kinds {
            let json = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn confidence_kind_expected_external_roundtrips() {
        let json = serde_json::to_string(&ConfidenceKind::ExpectedExternal).expect("serialize");
        assert_eq!(json, "\"ExpectedExternal\"");
        let back: ConfidenceKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ConfidenceKind::ExpectedExternal);
    }

    #[test]
    fn language_go_and_rust_serialization() {
        let go_json = serde_json::to_string(&Language::Go).expect("serialize");
        assert_eq!(go_json, "\"Go\"");
        let go_back: Language = serde_json::from_str(&go_json).expect("deserialize");
        assert_eq!(go_back, Language::Go);

        let rust_json = serde_json::to_string(&Language::Rust).expect("serialize");
        assert_eq!(rust_json, "\"Rust\"");
        let rust_back: Language = serde_json::from_str(&rust_json).expect("deserialize");
        assert_eq!(rust_back, Language::Rust);
    }

    #[test]
    fn node_kind_trait_and_enum_serialization() {
        let trait_json = serde_json::to_string(&NodeKind::Trait).expect("serialize");
        assert_eq!(trait_json, "\"Trait\"");
        let trait_back: NodeKind = serde_json::from_str(&trait_json).expect("deserialize");
        assert_eq!(trait_back, NodeKind::Trait);

        let enum_json = serde_json::to_string(&NodeKind::Enum).expect("serialize");
        assert_eq!(enum_json, "\"Enum\"");
        let enum_back: NodeKind = serde_json::from_str(&enum_json).expect("deserialize");
        assert_eq!(enum_back, NodeKind::Enum);
    }

    #[test]
    fn create_go_module_node() {
        let node = Node::module(
            "cmd.server.main",
            "cmd/server/main.go",
            Language::Go,
            1,
            true,
        );
        assert_eq!(node.language, Language::Go);
        assert_eq!(node.kind, NodeKind::Module);
    }

    #[test]
    fn create_rust_trait_node() {
        let node = Node::symbol(
            "crate.handler.Handler",
            NodeKind::Trait,
            "src/handler.rs",
            Language::Rust,
            5,
            true,
        );
        assert_eq!(node.kind, NodeKind::Trait);
        assert_eq!(node.language, Language::Rust);
    }

    #[test]
    fn create_rust_enum_node() {
        let node = Node::symbol(
            "crate.error.AppError",
            NodeKind::Enum,
            "src/error.rs",
            Language::Rust,
            10,
            true,
        );
        assert_eq!(node.kind, NodeKind::Enum);
    }

    // -----------------------------------------------------------------------
    // FEAT-021: alternative_paths
    // -----------------------------------------------------------------------

    #[test]
    fn node_default_alternative_paths_is_empty() {
        let node = Node::module("app.main", "app/main.py", Language::Python, 1, true);
        assert!(node.alternative_paths.is_empty());
    }

    #[test]
    fn node_with_alternative_paths_preserves_insertion_order() {
        let node = Node::symbol(
            "src.entities.Course",
            NodeKind::Class,
            "src/entities/course.ts",
            Language::TypeScript,
            1,
            true,
        )
        .with_alternative_paths(["src.domain.Course", "src.presentation.Course"]);
        assert_eq!(
            node.alternative_paths,
            vec!["src.domain.Course", "src.presentation.Course"]
        );
    }

    #[test]
    fn node_with_alternative_paths_deduplicates() {
        let node = Node::symbol(
            "src.entities.Course",
            NodeKind::Class,
            "src/entities/course.ts",
            Language::TypeScript,
            1,
            true,
        )
        .with_alternative_paths(["a", "b", "a", "c", "b"]);
        assert_eq!(node.alternative_paths, vec!["a", "b", "c"]);
    }

    #[test]
    fn node_without_alternatives_does_not_serialize_field() {
        // Legacy shape — writers consuming analysis.json must continue to
        // parse snapshots produced before FEAT-021.
        let node = Node::module("app.main", "app/main.py", Language::Python, 1, true);
        let json = serde_json::to_string(&node).expect("serialize");
        assert!(
            !json.contains("alternative_paths"),
            "empty alternative_paths must not appear in JSON, got {json}"
        );
    }

    #[test]
    fn node_with_alternatives_serializes_field() {
        let node = Node::symbol(
            "src.entities.Course",
            NodeKind::Class,
            "src/entities/course.ts",
            Language::TypeScript,
            5,
            true,
        )
        .with_alternative_paths(["src.domain.Course"]);
        let json = serde_json::to_string(&node).expect("serialize");
        assert!(json.contains("\"alternative_paths\":[\"src.domain.Course\"]"));
    }

    #[test]
    fn node_alternatives_roundtrip() {
        let node = Node::symbol(
            "src.entities.Course",
            NodeKind::Class,
            "src/entities/course.ts",
            Language::TypeScript,
            5,
            true,
        )
        .with_alternative_paths(["a", "b"]);
        let json = serde_json::to_string(&node).expect("serialize");
        let restored: Node = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.alternative_paths, vec!["a", "b"]);
    }

    #[test]
    fn node_deserializes_legacy_shape_without_alternatives() {
        // Older snapshots (pre-FEAT-021) lack the field entirely — must still
        // deserialize into an empty Vec.
        let legacy = r#"{
            "id": "app.main",
            "kind": "Module",
            "file_path": "app/main.py",
            "language": "Python",
            "line": 1,
            "is_local": true
        }"#;
        let node: Node = serde_json::from_str(legacy).expect("legacy node must parse");
        assert_eq!(node.id, "app.main");
        assert!(node.alternative_paths.is_empty());
    }

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
}
