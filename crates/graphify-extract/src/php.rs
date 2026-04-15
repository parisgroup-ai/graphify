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
