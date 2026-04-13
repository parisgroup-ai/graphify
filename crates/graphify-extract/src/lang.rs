use graphify_core::types::{Edge, Node};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The result of extracting a single source file.
#[derive(Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    /// Tuples of (source_id, target_id, edge).
    pub edges: Vec<(String, String, Edge)>,
}

impl ExtractionResult {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl Default for ExtractionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// A language-specific extractor that can process source files.
///
/// Implementations must be `Send + Sync` so they can be shared across Rayon
/// worker threads.
pub trait LanguageExtractor: Send + Sync {
    /// The file extensions handled by this extractor (e.g. `&["py"]`).
    fn extensions(&self) -> &[&str];

    /// Parse `source` (the raw bytes of the file at `path`) and return all
    /// discovered nodes and edges.  `module_name` is the dot-notation module
    /// identifier already computed by the walker (e.g. `app.services.llm`).
    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult;
}
