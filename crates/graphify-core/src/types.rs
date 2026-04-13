use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Python,
    TypeScript,
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
        }
    }
}

// ---------------------------------------------------------------------------
// EdgeKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        ];
        for (kind, expected) in kinds {
            let json = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(json, expected);
        }
    }
}
