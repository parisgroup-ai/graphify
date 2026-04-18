use graphify_core::types::{Edge, Node};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single `export … from …` statement captured during TypeScript extraction.
///
/// FEAT-021: the walker collects these per-file so a later pass can build a
/// project-wide re-export graph and collapse barrel chains to the canonical
/// declaration source.
///
/// The `raw_target` is the string as written in the source (`./entities`,
/// `@repo/shared`, …) — it is resolved by [`graphify_core`] callers against
/// the module resolver, not here, because resolution depends on project-wide
/// context (`tsconfig.paths`, `go.mod`, PSR-4, …) the extractor does not see.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReExportEntry {
    /// The dot-notation id of the module emitting this `export … from`.
    pub from_module: String,
    /// The raw string that followed `from` (before resolution).
    pub raw_target: String,
    /// Line where the statement begins (1-indexed).
    pub line: usize,
    /// Individual specifiers.
    ///
    /// Empty for `export * from './x'` — use [`is_star`] to distinguish.
    pub specs: Vec<ReExportSpec>,
    /// `true` when the original statement is `export * from …`.
    pub is_star: bool,
}

/// One specifier inside `export { exported as local } from '…'`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReExportSpec {
    /// The name as it lives in the source module (`Course`).
    pub exported_name: String,
    /// The name this barrel publishes, after aliasing.
    ///
    /// For `export { Foo as Bar } from '…'` this is `"Bar"`;
    /// for plain `export { Foo } from '…'` it equals `exported_name`.
    pub local_name: String,
}

/// The result of extracting a single source file.
#[derive(Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    /// Tuples of (source_id, target_id, edge).
    pub edges: Vec<(String, String, Edge)>,
    /// `export … from …` and `export * from …` statements captured from the
    /// file, used by FEAT-021's barrel-collapse pass. Empty for every
    /// non-TypeScript extractor and for TypeScript files that do not re-export.
    ///
    /// `#[serde(default)]` lets older cached extraction results (from before
    /// FEAT-021) deserialize cleanly into an empty vector.
    #[serde(default)]
    pub reexports: Vec<ReExportEntry>,
}

impl ExtractionResult {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            reexports: Vec::new(),
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
