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

/// A single `import … from …` statement captured during TypeScript extraction.
///
/// FEAT-026: the walker collects these per-file so the pipeline can walk each
/// specifier through the project-wide re-export graph and emit module-level
/// `Imports` edges that target the canonical declaration module (not the
/// barrel). The raw `source` path is resolved project-wide later because
/// resolution depends on `tsconfig.paths`, workspace aliases, etc.
///
/// Semantics:
/// - Named / default imports populate `specs` with the upstream name to look
///   up in the re-export graph (default imports use the literal name `"default"`).
/// - `import * as ns from '…'` never produces a `NamedImportEntry` — the
///   extractor falls back to the single barrel edge, as documented in the v1
///   policy.
/// - Side-effect imports (`import 'x'`) never produce a `NamedImportEntry`
///   for the same reason.
/// - Type-only imports (`import type { Foo } from '…'`) are captured here
///   with `is_type_only = true`, preserving parity with the pre-FEAT-026
///   behaviour where they contributed a single Imports edge.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamedImportEntry {
    /// The dot-notation id of the module emitting the `import` statement.
    pub from_module: String,
    /// The raw string that followed `from` (before resolution).
    pub raw_target: String,
    /// Line where the statement begins (1-indexed).
    pub line: usize,
    /// Upstream names to look up in the re-export graph, one per specifier.
    ///
    /// - `import { Foo, Bar }` → `["Foo", "Bar"]`
    /// - `import { Foo as MyFoo }` → `["Foo"]` (upstream name; the local
    ///   alias does not affect re-export graph lookup)
    /// - `import X from '…'` → `["default"]`
    /// - `import X, { Foo } from '…'` → `["default", "Foo"]`
    pub specs: Vec<String>,
    /// `true` if the import statement was `import type { … } from '…'`.
    /// Emitted so consumers can preserve parity with the pre-FEAT-026 edge
    /// counting (one edge per statement).
    pub is_type_only: bool,
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
    /// `import { X, Y } from '…'` / `import X from '…'` statements captured
    /// from the file. Used by FEAT-026 to fan module-level `Imports` edges
    /// out to the canonical declaration module of each specifier instead of
    /// keeping a single edge to the barrel.
    ///
    /// `#[serde(default)]` lets older cached extraction results (from before
    /// FEAT-026) deserialize cleanly into an empty vector.
    #[serde(default)]
    pub named_imports: Vec<NamedImportEntry>,
}

impl ExtractionResult {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            reexports: Vec::new(),
            named_imports: Vec::new(),
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
