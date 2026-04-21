use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::workspace_reexport::{WorkspaceAliasTarget, WorkspaceReExportGraph};

/// Maximum number of consecutive alias rewrites the resolver is willing to
/// follow before giving up and returning the raw id as non-local (FEAT-031
/// case 9 depth guard, BUG-017 fix).
///
/// Legitimate Rust chains need exactly one rewrite:
///   - bare `Node` → `crate::types::Node` (one rewrite → case 6 `crate::` →
///     canonical local id).
///   - scoped `Node::module` → `crate::types::Node::module` (same shape).
///
/// A depth of 4 leaves headroom for any indirection we haven't seen yet
/// without allowing the self-referential alias loops (`("X", "X::Y")`) that
/// caused v0.11.4's OOM to grow the rewrite string unbounded.
const MAX_ALIAS_REWRITE_DEPTH: u8 = 4;

/// Returns `true` when `full` begins with `root` followed by a `::` boundary
/// (or equals `root` exactly). Used by the alias-rewrite fallback to skip
/// self-amplifying aliases like `("X", "X::Y")` that would otherwise grow
/// the rewritten string on every recursion.
fn full_starts_with_root(full: &str, root: &str) -> bool {
    if full == root {
        return true;
    }
    match full.strip_prefix(root) {
        Some(rest) => rest.starts_with("::"),
        None => false,
    }
}

#[derive(Clone, Debug)]
struct TsAliasContext {
    alias_pattern: String,
    target_pattern: String,
    base_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// ModuleResolver
// ---------------------------------------------------------------------------

/// Resolves raw import strings to canonical dot-notation module identifiers,
/// and determines whether the target is a local (in-project) module.
///
/// Supports:
/// - Python relative imports (`.utils`, `..models`, …)
/// - TypeScript path aliases from `tsconfig.json` `compilerOptions.paths`
/// - TypeScript / generic relative imports (`./foo`, `../bar`)
/// - Direct module names checked against the registered known-modules set
pub struct ModuleResolver {
    /// All module names that are part of the local project, keyed by name.
    known_modules: HashMap<String, String>,
    /// Optional path-based lookup keys for discovered modules.
    module_lookup_paths: HashMap<PathBuf, String>,
    /// TypeScript tsconfig path aliases: `(alias_pattern, target_pattern)`.
    /// Example: `("@/*", "src/*")`.
    ts_aliases: Vec<(String, String)>,
    /// Per-source-module TypeScript alias contexts loaded from the nearest
    /// tsconfig file for that module.
    ts_aliases_by_module: HashMap<String, Vec<TsAliasContext>>,
    /// Go module path from `go.mod` (e.g. `github.com/user/repo`).
    go_module_path: Option<String>,
    /// PSR-4 autoload mappings from `composer.json`: `(namespace_prefix, dir_prefix)`.
    /// Example: `("App\\", "src/")`.
    psr4_mappings: Vec<(String, String)>,
    /// Project-level module prefix applied by the walker (e.g. `"src"` for a
    /// typical Rust crate, `"app"` for a Python project, often empty for TS).
    /// Used by language-specific resolver branches that need to re-prepend the
    /// prefix when stripping a language-rooted prefix like Rust's `crate::`.
    local_prefix: String,
    /// Per-source-module short-name → full-path alias map (FEAT-031). Each
    /// entry corresponds to a `use` declaration captured in the source file
    /// (e.g. `use crate::types::Node;` in `src/graph.rs` registers
    /// `("Node", "crate::types::Node")` under key `"src.graph"`). Consulted
    /// as a final fallback in `resolve()` when the direct-name lookup misses,
    /// so scoped-identifier call targets like `Node::module` can be rewritten
    /// to their canonical local ids.
    use_aliases_by_module: HashMap<String, HashMap<String, String>>,
    /// Workspace / project root (reserved for future use).
    #[allow(dead_code)]
    root: PathBuf,
}

impl ModuleResolver {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create an empty resolver rooted at `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            known_modules: HashMap::new(),
            module_lookup_paths: HashMap::new(),
            ts_aliases: Vec::new(),
            ts_aliases_by_module: HashMap::new(),
            go_module_path: None,
            psr4_mappings: Vec::new(),
            local_prefix: String::new(),
            use_aliases_by_module: HashMap::new(),
            root: normalize_path(root),
        }
    }

    /// Set the project-level module prefix (e.g. `"src"`).
    ///
    /// Required for correct Rust `crate::` resolution when the walker
    /// auto-prefixes module names — without this, `crate::types::Node` from
    /// any module in the crate resolves to `types.Node` instead of
    /// `src.types.Node` and never matches a known local module (BUG-016).
    pub fn set_local_prefix(&mut self, prefix: &str) {
        self.local_prefix = prefix.to_owned();
    }

    /// Prepend `self.local_prefix` to `id` when non-empty and not already
    /// present. Empty `id` collapses to the bare prefix; empty prefix is a
    /// no-op.
    fn apply_local_prefix(&self, id: &str) -> String {
        if self.local_prefix.is_empty() {
            return id.to_owned();
        }
        if id.is_empty() {
            return self.local_prefix.clone();
        }
        if id == self.local_prefix || id.starts_with(&format!("{}.", self.local_prefix)) {
            return id.to_owned();
        }
        format!("{}.{}", self.local_prefix, id)
    }

    /// Register the per-file `use`-alias map captured by the Rust extractor
    /// for the source module `from_module` (FEAT-031).
    ///
    /// The resolver consults this map as a final fallback in `resolve()`
    /// when a bare or scoped call target doesn't match any registered local
    /// module directly — e.g. `Node::module` after `use crate::types::Node;`
    /// becomes `crate::types::Node::module`, which the `crate::` branch
    /// then canonicalizes to `src.types.Node.module`.
    ///
    /// Calling this multiple times for the same module merges entries
    /// (first write wins per short name, matching the extractor's own
    /// `register_use_alias` semantics).
    pub fn register_use_aliases(&mut self, from_module: &str, aliases: &HashMap<String, String>) {
        if aliases.is_empty() {
            return;
        }
        let slot = self
            .use_aliases_by_module
            .entry(from_module.to_owned())
            .or_default();
        for (short, full) in aliases {
            slot.entry(short.clone()).or_insert_with(|| full.clone());
        }
    }

    /// Register a dot-notation module name as a local module.
    pub fn register_module(&mut self, module_name: &str) {
        self.known_modules
            .insert(module_name.to_owned(), module_name.to_owned());
    }

    /// Register a module together with its discovered source path so later
    /// path-based resolution can canonicalize back to the module ID.
    pub fn register_module_path(&mut self, module_name: &str, file_path: &Path, is_package: bool) {
        self.register_module(module_name);

        let lookup_path = path_without_extension(file_path);
        self.module_lookup_paths
            .insert(normalize_path(&lookup_path), module_name.to_owned());

        if is_package {
            if let Some(parent) = file_path.parent() {
                self.module_lookup_paths
                    .insert(normalize_path(parent), module_name.to_owned());
            }
        }
    }

    // -----------------------------------------------------------------------
    // tsconfig loading
    // -----------------------------------------------------------------------

    /// Parse a `tsconfig.json` file and load `compilerOptions.paths` aliases.
    ///
    /// This is a minimal hand-written parser — it looks for `"paths"` inside
    /// `"compilerOptions"` and extracts key→first-value pairs, without pulling
    /// in a full JSON dependency.
    ///
    /// Alias entries look like:
    /// ```json
    /// {
    ///   "compilerOptions": {
    ///     "paths": {
    ///       "@/*": ["src/*"],
    ///       "@lib/*": ["src/lib/*"]
    ///     }
    ///   }
    /// }
    /// ```
    pub fn load_tsconfig(&mut self, tsconfig_path: &Path) {
        for (key, target) in parse_tsconfig_paths(tsconfig_path) {
            self.ts_aliases.push((key, target));
        }
    }

    /// Load a tsconfig alias set for a specific source module. The alias
    /// targets remain path-based and are resolved relative to the tsconfig
    /// directory during `resolve`.
    pub fn load_tsconfig_for_module(&mut self, module_name: &str, tsconfig_path: &Path) {
        let base_dir = tsconfig_path
            .parent()
            .map(normalize_path)
            .unwrap_or_else(|| self.root.clone());
        let aliases: Vec<_> = parse_tsconfig_paths(tsconfig_path)
            .into_iter()
            .map(|(alias_pattern, target_pattern)| TsAliasContext {
                alias_pattern,
                target_pattern,
                base_dir: base_dir.clone(),
            })
            .collect();
        if aliases.is_empty() {
            return;
        }
        self.ts_aliases_by_module
            .entry(module_name.to_owned())
            .or_default()
            .extend(aliases);
    }

    // -----------------------------------------------------------------------
    // go.mod loading
    // -----------------------------------------------------------------------

    /// Parse a `go.mod` file and extract the `module` path.
    ///
    /// The module line looks like:
    /// ```text
    /// module github.com/user/repo
    /// ```
    pub fn load_go_mod(&mut self, go_mod_path: &Path) {
        let text = match std::fs::read_to_string(go_mod_path) {
            Ok(t) => t,
            Err(_) => return,
        };

        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(module_path) = trimmed.strip_prefix("module ") {
                let module_path = module_path.trim();
                if !module_path.is_empty() {
                    self.go_module_path = Some(module_path.to_owned());
                }
                break;
            }
        }
    }

    // -----------------------------------------------------------------------
    // composer.json loading
    // -----------------------------------------------------------------------

    /// Return an immutable view over the parsed PSR-4 mappings. Used by the
    /// walker to translate file paths to namespace-prefixed module names.
    pub fn psr4_mappings(&self) -> &[(String, String)] {
        &self.psr4_mappings
    }

    /// Returns `true` if `module_id` is a registered local (in-project)
    /// module. Used by FEAT-021's barrel-collapse pass, which needs to
    /// decide whether to follow an `export * from …` chain into the
    /// upstream module or stop at the package boundary.
    pub fn is_local_module(&self, module_id: &str) -> bool {
        self.known_modules.contains_key(module_id)
    }

    /// Parse `composer.json` and load `autoload.psr-4` + `autoload-dev.psr-4`
    /// mappings. Tolerates missing files and malformed JSON — failures leave
    /// the mappings empty without panicking.
    pub fn load_composer_json(&mut self, composer_path: &Path) {
        let text = match std::fs::read_to_string(composer_path) {
            Ok(t) => t,
            Err(_) => return,
        };

        for section in ["autoload", "autoload-dev"] {
            if let Some(psr4_block) = find_psr4_block(&text, section) {
                for pair in parse_psr4_pairs(&psr4_block) {
                    self.psr4_mappings.push(pair);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Resolution
    // -----------------------------------------------------------------------

    /// Resolve a raw import string `raw` found in module `from_module` to a
    /// canonical dot-notation identifier, determine whether it is local, and
    /// return a confidence score for the resolution.
    ///
    /// Confidence scores:
    /// - `1.0` — direct module name (exact match or unknown external)
    /// - `0.9` — Python relative import or TS relative import (heuristic)
    /// - `0.85` — TypeScript path alias (depends on tsconfig config)
    ///
    /// Returns `(resolved_id, is_local, confidence)`.
    pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool, f64) {
        self.resolve_with_depth(raw, from_module, is_package, MAX_ALIAS_REWRITE_DEPTH)
    }

    /// Internal recursive form of [`resolve`] with a depth budget for the
    /// FEAT-031 alias-rewrite fallback (case 9).
    ///
    /// BUG-017 regression fix: the original implementation called
    /// `self.resolve(&rewritten, …)` unconditionally on every alias hit.
    /// Self-referential aliases (`("X", "X::Y")`) or cycles between two
    /// aliases would grow the rewritten string unbounded inside repeated
    /// `format!()` calls, burning ~17 GB RSS in the first 10 s of the
    /// graphify-cli dogfood extraction. The depth budget terminates rewrites
    /// after [`MAX_ALIAS_REWRITE_DEPTH`] iterations with a non-local result,
    /// preserving the legitimate one-hop-rewrite case (the common shape
    /// `Node::module` → `crate::types::Node::module` → canonical local id)
    /// while preventing the runaway.
    fn resolve_with_depth(
        &self,
        raw: &str,
        from_module: &str,
        is_package: bool,
        depth_remaining: u8,
    ) -> (String, bool, f64) {
        // 1. Python relative imports (start with one or more dots).
        if raw.starts_with('.') && !raw.starts_with("./") && !raw.starts_with("../") {
            let resolved = resolve_python_relative(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 2. TypeScript path aliases loaded from the source module's nearest
        // tsconfig (e.g. `@/lib/api`).
        if let Some(contexts) = self.ts_aliases_by_module.get(from_module) {
            for ctx in contexts {
                if let Some(resolved) = self.apply_ts_alias_with_context(
                    raw,
                    &ctx.alias_pattern,
                    &ctx.target_pattern,
                    &ctx.base_dir,
                ) {
                    let is_local = self.known_modules.contains_key(&resolved);
                    return (resolved, is_local, 0.85);
                }
            }
        }

        // 3. Global TypeScript path aliases (backward-compatible path-free mode).
        for (alias_pat, target_pat) in &self.ts_aliases {
            if let Some(resolved) = apply_ts_alias(raw, alias_pat, target_pat) {
                let is_local = self.known_modules.contains_key(&resolved);
                return (resolved, is_local, 0.85);
            }
        }

        // 4. TypeScript / generic relative imports (`./foo`, `../bar`).
        if raw.starts_with("./") || raw.starts_with("../") {
            let resolved = resolve_ts_relative(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 5. Go module-path imports (strip go.mod module prefix).
        if let Some(ref go_mod) = self.go_module_path {
            if let Some(rest) = raw.strip_prefix(go_mod.as_str()) {
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                if !rest.is_empty() {
                    let relative = rest.replace('/', ".");
                    let resolved = self
                        .canonicalize_known_module(&relative)
                        .unwrap_or(relative);
                    let is_local = self.known_modules.contains_key(&resolved);
                    return (resolved, is_local, 0.9);
                }
            }
        }

        // 6. Rust `crate::`, `super::`, `self::` imports.
        if let Some(rest) = raw.strip_prefix("crate::") {
            // `crate::` is rooted at the crate's source root, which the walker
            // names with `local_prefix` (e.g. `src` for a Rust crate). Strip
            // `crate::`, normalize separators, then re-prepend the prefix so
            // the lookup matches registered modules. (BUG-016)
            let stripped = rest.replace("::", ".");
            let resolved = self.apply_local_prefix(&stripped);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }
        if raw.starts_with("super::") || raw.starts_with("self::") {
            let resolved = resolve_rust_path(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 7. PHP `use` targets (contain a backslash separator).
        if raw.contains('\\') {
            let normalized = raw.trim_start_matches('\\').replace('\\', ".");
            let is_local = self.known_modules.contains_key(&normalized);
            return (normalized, is_local, 1.0);
        }

        // 8. Direct module name — check against known modules.
        if self.known_modules.contains_key(raw) {
            return (raw.to_owned(), true, 1.0);
        }

        // 9. Rust `use`-alias fallback (FEAT-031). When a scoped or bare call
        //    target isn't a registered local module directly, consult the
        //    per-source-module alias map captured by the Rust extractor and
        //    rewrite the root segment (or the whole name) to its fully-
        //    qualified form. Recurses back through `resolve_with_depth` so
        //    the rewritten path flows through the language-specific branches
        //    (typically the `crate::` branch, case 6). Confidence is governed
        //    at the edge level — Calls edges arrive here at 0.7/Inferred from
        //    the extractor, so the recursive resolver's 0.9 is capped back
        //    down by the pipeline's `min(edge.confidence, resolver_confidence)`.
        //
        //    The depth budget guards against BUG-017: self-referential or
        //    cyclic aliases would otherwise grow the rewritten string without
        //    termination. When the budget is exhausted we fall through to
        //    the no-match return below so the call still produces a bounded
        //    result (non-local, raw id preserved).
        if depth_remaining > 0 {
            if let Some(aliases) = self.use_aliases_by_module.get(from_module) {
                // Bare-name form: the whole raw matches a `use`-imported short
                // name. Skip when the alias value equals the key — a
                // `("X", "X")` self-alias would recurse with identical
                // arguments and do nothing useful even under the depth cap.
                if let Some(full) = aliases.get(raw).filter(|f| f.as_str() != raw) {
                    let full = full.clone();
                    return self.resolve_with_depth(
                        &full,
                        from_module,
                        is_package,
                        depth_remaining - 1,
                    );
                }
                // Scoped form: rewrite only the root segment. Skip when the
                // alias value starts with `root::` — the rewrite would just
                // re-prepend the same root segment and recurse on a longer
                // string (the BUG-017 growth signature).
                if let Some((root, tail)) = raw.split_once("::") {
                    if let Some(full) = aliases
                        .get(root)
                        .filter(|f| !full_starts_with_root(f, root))
                    {
                        let rewritten = format!("{}::{}", full, tail);
                        return self.resolve_with_depth(
                            &rewritten,
                            from_module,
                            is_package,
                            depth_remaining - 1,
                        );
                    }
                }
            }
        }

        // No match — external or unresolved reference.
        (raw.to_owned(), false, 1.0)
    }

    fn apply_ts_alias_with_context(
        &self,
        raw: &str,
        alias_pat: &str,
        target_pat: &str,
        base_dir: &Path,
    ) -> Option<String> {
        let resolved_path = match_alias_target(raw, alias_pat, target_pat)?;
        let candidate = normalize_path(&base_dir.join(resolved_path));

        if let Some(module) = self.lookup_module_by_path(&candidate) {
            return Some(module);
        }

        if !candidate.starts_with(&self.root) {
            return Some(raw.to_owned());
        }

        let relative = candidate.strip_prefix(&self.root).ok()?;
        let clean = relative.to_string_lossy().replace('\\', "/");
        Some(path_to_dot_notation(clean.trim_start_matches('/')))
    }

    /// Workspace-aware tsconfig alias resolver (FEAT-028 step 4).
    ///
    /// Same alias expansion logic as the per-project
    /// [`ModuleResolver::apply_ts_alias_with_context`], but when the
    /// expanded target path falls **outside** this project's `self.root`,
    /// asks `workspace` whether the path lands inside any other registered
    /// project via [`WorkspaceReExportGraph::lookup_module_by_path`].
    ///
    /// Returns:
    /// - `Some(WorkspaceAliasTarget { project, module_id })` when the alias
    ///   resolves into another workspace project. Callers (the FEAT-028
    ///   fan-out loop) use this to emit a cross-project edge instead of
    ///   terminating at the raw alias.
    /// - `None` in every other case — including the alias matching locally
    ///   (let the existing per-project resolver handle it) and the alias
    ///   pointing outside ALL project roots (the caller should fall back to
    ///   the v1 raw-alias contract, preserving the FEAT-027 behaviour for
    ///   non-workspace externals).
    ///
    /// Does NOT modify any existing resolver behaviour — the per-project
    /// `resolve` call path is untouched. Step 5 of FEAT-028 wires this into
    /// the fan-out loop at `graphify-cli::main::run_extract`.
    pub fn apply_ts_alias_workspace(
        &self,
        raw: &str,
        from_module: &str,
        workspace: &WorkspaceReExportGraph,
    ) -> Option<WorkspaceAliasTarget> {
        // Per-module tsconfig contexts first (matches the ordering in
        // `resolve`). Each context carries its own `base_dir`, loaded from
        // the tsconfig file that declared the alias — that's the correct
        // anchor for relative alias targets like `../../packages/*/src`.
        if let Some(contexts) = self.ts_aliases_by_module.get(from_module) {
            for ctx in contexts {
                if let Some(target) = self.lookup_ts_alias_in_workspace(
                    raw,
                    &ctx.alias_pattern,
                    &ctx.target_pattern,
                    &ctx.base_dir,
                    workspace,
                ) {
                    return Some(target);
                }
            }
        }

        // Global aliases fall back to `self.root` as the anchor — same as
        // the per-project resolver's implicit behaviour.
        for (alias_pat, target_pat) in &self.ts_aliases {
            if let Some(target) =
                self.lookup_ts_alias_in_workspace(raw, alias_pat, target_pat, &self.root, workspace)
            {
                return Some(target);
            }
        }

        None
    }

    fn lookup_ts_alias_in_workspace(
        &self,
        raw: &str,
        alias_pat: &str,
        target_pat: &str,
        base_dir: &Path,
        workspace: &WorkspaceReExportGraph,
    ) -> Option<WorkspaceAliasTarget> {
        let resolved_path = match_alias_target(raw, alias_pat, target_pat)?;
        let candidate = normalize_path(&base_dir.join(resolved_path));

        // Only yield when the candidate path is OUTSIDE the current project.
        // Inside-root hits are the per-project resolver's responsibility and
        // returning them here would double-fan-out in the caller (step 5).
        if candidate.starts_with(&self.root) {
            return None;
        }

        let (project, module_id) = workspace.lookup_module_by_path(&candidate)?;
        Some(WorkspaceAliasTarget { project, module_id })
    }

    fn lookup_module_by_path(&self, candidate: &Path) -> Option<String> {
        let key = normalize_path(candidate);
        if let Some(module) = self.module_lookup_paths.get(&key) {
            return Some(module.clone());
        }

        let without_ext = normalize_path(&path_without_extension(&key));
        self.module_lookup_paths.get(&without_ext).cloned()
    }

    fn canonicalize_known_module(&self, relative: &str) -> Option<String> {
        if let Some(module) = self.known_modules.get(relative) {
            return Some(module.clone());
        }

        let suffix = format!(".{relative}");
        let mut matches = self
            .known_modules
            .keys()
            .filter(|module| module.ends_with(&suffix))
            .cloned();
        let first = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(first.to_owned())
    }
}

// ---------------------------------------------------------------------------
// Python relative import resolution
// ---------------------------------------------------------------------------

/// Resolve a Python relative import (starts with `.`) from `from_module`.
///
/// When `is_package` is true, `from_module` represents a package entry point
/// (`__init__.py` or `index.ts`), so the module name already IS the package
/// and the initial leaf-pop is skipped.
///
/// Rules:
/// - One dot  (`.utils`)  → sibling of `from_module` in the same package.
/// - Two dots (`..models`) → sibling in the parent package.
/// - N dots   → walk up N-1 levels from `from_module`'s package.
///
/// Examples:
/// - `.utils`  from `app.services.llm` (non-package) → `app.services.utils`
/// - `.llm`    from `app.errors`       (package)     → `app.errors.llm`
/// - `..models` from `app.services.llm` → `app.models`
fn resolve_python_relative(raw: &str, from_module: &str, is_package: bool) -> String {
    // Count leading dots.
    let dot_count = raw.chars().take_while(|&c| c == '.').count();
    let suffix = &raw[dot_count..]; // the part after the dots (may be empty)

    // Split from_module into parts and walk up (dot_count - 1) times from the
    // package (i.e. strip the leaf module name first, then strip dot_count-1
    // more components).
    let mut parts: Vec<&str> = from_module.split('.').collect();

    // Strip the leaf (the current module's own name) — but only for non-package
    // modules. For package modules (__init__.py), from_module IS the package,
    // so we keep all parts.
    if !is_package && !parts.is_empty() {
        parts.pop();
    }

    // Walk up (dot_count - 1) additional levels.
    for _ in 0..dot_count.saturating_sub(1) {
        if !parts.is_empty() {
            parts.pop();
        }
    }

    // Append the suffix (if any).
    if suffix.is_empty() {
        parts.join(".")
    } else if parts.is_empty() {
        suffix.to_owned()
    } else {
        format!("{}.{}", parts.join("."), suffix)
    }
}

// ---------------------------------------------------------------------------
// TypeScript alias resolution
// ---------------------------------------------------------------------------

/// Try to match `raw` against `alias_pattern` and, if it matches, return the
/// resolved dot-notation module name using `target_pattern`.
///
/// Patterns may end with `/*` (glob wildcard), e.g.:
/// - alias: `"@/*"`, target: `"src/*"`
/// - `"@/lib/api"` → `"src/lib/api"` → `"src.lib.api"`
///
/// If neither pattern contains `/*`, an exact-match alias is attempted.
///
/// When the resolved path contains parent-directory traversal (`..`), the alias
/// points outside the current project (e.g. a workspace package reference like
/// `@repo/* → ../../packages/*`). In that case the original import string is
/// preserved as the node identifier rather than producing a mangled dot-notation
/// name.
fn apply_ts_alias(raw: &str, alias_pat: &str, target_pat: &str) -> Option<String> {
    let resolved_path = match_alias_target(raw, alias_pat, target_pat)?;

    // If the resolved path traverses outside the project, keep the original
    // import string as the node identifier (BUG-007).
    if resolved_path.contains("..") {
        return Some(raw.to_owned());
    }

    // Strip leading "./" before converting to dot notation.
    let clean = resolved_path.strip_prefix("./").unwrap_or(&resolved_path);
    Some(path_to_dot_notation(clean))
}

fn match_alias_target(raw: &str, alias_pat: &str, target_pat: &str) -> Option<String> {
    // Support three alias forms:
    //   1. Exact match:    alias and target contain no `*`.
    //   2. Trailing glob:  alias ends with `*`, target ends with `*`
    //                      (`@/*` → `src/*`).
    //   3. Inner glob:     alias or target contain a single `*` in the
    //                      middle or suffix (`@repo/*` → `../../packages/*/src`).
    //
    // Inner-glob support is required for pnpm-style workspace tsconfigs
    // (FEAT-028): a consumer maps `@repo/*` to a sibling package's inner
    // source directory, not its root. Splitting each pattern on its sole
    // `*` lets us capture the glob from `raw` and re-inject it into the
    // target without hard-coding a trailing-`*` assumption.
    //
    // Keep the separator before `*` intact so `@/*` matches only `@/foo`,
    // not external scoped packages like `@repo/foo` (BUG-011).
    let alias_parts: Vec<&str> = alias_pat.splitn(3, '*').collect();
    let target_parts: Vec<&str> = target_pat.splitn(3, '*').collect();

    // Reject multi-`*` patterns (ambiguous capture). tsconfig-paths in
    // practice uses exactly one `*` per mapping.
    if alias_parts.len() > 2 || target_parts.len() > 2 {
        return None;
    }

    match (alias_parts.as_slice(), target_parts.as_slice()) {
        // Exact match (no `*` in either pattern).
        ([_], [_]) => {
            if raw == alias_pat {
                Some(target_pat.to_owned())
            } else {
                None
            }
        }
        // Glob alias, exact target — unusual but treat as exact match if the
        // glob captures nothing meaningful. Not supported: skip.
        ([_, _], [_]) => None,
        // Exact alias, glob target — alias has no capture to substitute into
        // the target's `*`. Not supported: skip.
        ([_], [_, _]) => None,
        // Glob alias, glob target. Both have exactly one `*`; capture the
        // wildcard slice from `raw` and splice it into the target.
        ([ap_prefix, ap_suffix], [tp_prefix, tp_suffix]) => {
            // `raw` must start with `ap_prefix` and end with `ap_suffix`,
            // with the captured glob in between.
            let tail = raw.strip_prefix(ap_prefix)?;
            let captured = tail.strip_suffix(ap_suffix)?;
            Some(format!("{}{}{}", tp_prefix, captured, tp_suffix))
        }
        // Unreachable — `splitn(3, '*')` with the earlier length guard pins
        // each slice to either 1 or 2 elements.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// TypeScript relative import resolution
// ---------------------------------------------------------------------------

/// Resolve a TypeScript relative import (`./foo` or `../bar`) from `from_module`.
///
/// Examples:
/// - `"./services/user"` from `"src.index"` → `"src.services.user"`
/// - `"../lib/api"` from `"src.services.user"` → `"src.lib.api"`
fn resolve_ts_relative(raw: &str, from_module: &str, is_package: bool) -> String {
    // Split from_module and drop the leaf (current file) — UNLESS `from_module`
    // is a package entry point (`index.ts`), in which case the module id
    // already collapses to the containing directory. Popping the leaf a second
    // time would over-climb and send `./entities` from `src/domain/index.ts`
    // to `src.entities` instead of `src.domain.entities` (FEAT-021 discovered
    // this hiding as a symptom of the barrel-collapse pass failing to resolve
    // re-export chains into the same-directory canonical target).
    let mut parts: Vec<&str> = from_module.split('.').collect();
    if !is_package && !parts.is_empty() {
        parts.pop();
    }

    // Normalise raw by converting `/` separators to components.
    // Process each component of the raw path.
    let mut remaining = raw;

    // Handle leading `./` and `../` sequences.
    loop {
        if let Some(rest) = remaining.strip_prefix("./") {
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("../") {
            if !parts.is_empty() {
                parts.pop();
            }
            remaining = rest;
        } else {
            break;
        }
    }

    // `remaining` is now the relative path without leading `./` or `../`.
    // Strip TS/JS file extension: TS-ESM and NodeNext imports carry the `.js`
    // suffix (which resolves to `.ts` source). Without stripping, `./foo.js`
    // produces the literal id `parent.foo.js` and never matches `known_modules`,
    // inflating the ambiguous-edge count on bundler-style projects.
    // Longer extensions come first so `.tsx` matches before `.ts`.
    const TS_EXTENSIONS: &[&str] = &[".mjs", ".cjs", ".mts", ".cts", ".jsx", ".tsx", ".js", ".ts"];
    let remaining = TS_EXTENSIONS
        .iter()
        .find_map(|ext| remaining.strip_suffix(ext))
        .unwrap_or(remaining);

    // Convert it to dot notation and append.
    let suffix = remaining.replace('/', ".");

    if suffix.is_empty() {
        parts.join(".")
    } else if parts.is_empty() {
        suffix
    } else {
        format!("{}.{}", parts.join("."), suffix)
    }
}

// ---------------------------------------------------------------------------
// Rust path resolution
// ---------------------------------------------------------------------------

/// Resolve a Rust `super::` or `self::` path from `from_module`.
///
/// Unlike Python relative imports (which always pop the leaf first), Rust
/// paths start from the current module itself:
/// - `self::x`  → child of current module (e.g. from `db` → `db.x`)
/// - `super::x` → sibling in parent module (e.g. from `db` → `services.x`)
/// - `super::super::x` → in grandparent (e.g. from `db` → `src.x`)
///
/// Each `super::` pops one level from the full module path.
fn resolve_rust_path(raw: &str, from_module: &str, _is_package: bool) -> String {
    let mut parts: Vec<&str> = from_module.split('.').collect();

    let mut remaining = raw;

    // `self::` stays at current module level — just strip the prefix.
    if let Some(rest) = remaining.strip_prefix("self::") {
        remaining = rest;
    }

    // Each `super::` walks up one level from the current module.
    while let Some(rest) = remaining.strip_prefix("super::") {
        if !parts.is_empty() {
            parts.pop();
        }
        remaining = rest;
    }

    // Convert remaining `::` to `.` notation.
    let suffix = remaining.replace("::", ".");

    if suffix.is_empty() {
        parts.join(".")
    } else if parts.is_empty() {
        suffix
    } else {
        format!("{}.{}", parts.join("."), suffix)
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Convert a slash-separated path string to dot notation.
fn path_to_dot_notation(path: &str) -> String {
    path.replace('/', ".")
}

fn path_without_extension(path: &Path) -> PathBuf {
    match (path.parent(), path.file_stem()) {
        (Some(parent), Some(stem)) => parent.join(stem),
        _ => path.to_path_buf(),
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn parse_tsconfig_paths(tsconfig_path: &Path) -> Vec<(String, String)> {
    let text = match std::fs::read_to_string(tsconfig_path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let paths_start = match find_paths_section(&text) {
        Some(pos) => pos,
        None => return Vec::new(),
    };

    let slice = &text[paths_start..];
    let block = match extract_brace_block(slice) {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut aliases = Vec::new();
    let mut pos = 0;
    while pos < block.len() {
        let key = match extract_quoted_string(&block[pos..]) {
            Some((k, end)) => {
                pos += end;
                k
            }
            None => break,
        };

        if let Some(colon) = block[pos..].find(':') {
            pos += colon + 1;
        } else {
            break;
        }

        while pos < block.len() && block.as_bytes()[pos].is_ascii_whitespace() {
            pos += 1;
        }

        if pos >= block.len() || block.as_bytes()[pos] != b'[' {
            continue;
        }
        pos += 1;

        let target = match extract_quoted_string(&block[pos..]) {
            Some((t, end)) => {
                pos += end;
                t
            }
            None => continue,
        };

        if !key.is_empty() && !target.is_empty() {
            aliases.push((key, target));
        }
    }

    aliases
}

/// Find the position of the opening `{` that begins the `"paths"` value
/// inside a `compilerOptions` block.  Returns the index into `text`.
fn find_paths_section(text: &str) -> Option<usize> {
    // Find "compilerOptions" first to scope the search.
    let co_pos = text.find("\"compilerOptions\"")?;
    let after_co = &text[co_pos..];

    // Within compilerOptions block, find "paths".
    let paths_key_offset = after_co.find("\"paths\"")?;
    let after_paths_key = &after_co[paths_key_offset + "\"paths\"".len()..];

    // Skip ':' and whitespace to find '{'.
    let brace_offset = after_paths_key.find('{')?;

    Some(co_pos + paths_key_offset + "\"paths\"".len() + brace_offset)
}

/// Given a string slice that starts at (or before) a `{`, extract the
/// content between the first `{` and its matching `}`.
fn extract_brace_block(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let chars: Vec<char> = s[start + 1..].chars().collect();
    let mut depth = 1usize;
    let mut result = String::new();

    for ch in &chars {
        match ch {
            '{' => {
                depth += 1;
                result.push(*ch);
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                result.push(*ch);
            }
            _ => result.push(*ch),
        }
    }

    if depth == 0 {
        Some(result)
    } else {
        None
    }
}

/// Find the next double-quoted string in `s` and return `(content, end_pos)`
/// where `end_pos` is the byte index in `s` just after the closing `"`.
fn extract_quoted_string(s: &str) -> Option<(String, usize)> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let mut content = String::new();
    let mut escaped = false;

    for (i, ch) in rest.char_indices() {
        if escaped {
            content.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            let end = start + 1 + i + 1;
            return Some((content, end));
        } else {
            content.push(ch);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Composer.json mini-parser (PSR-4 only, no serde_json dep)
// ---------------------------------------------------------------------------

/// Find the body of `<section>.psr-4` as a raw substring between its outer
/// `{` and the matching `}`. Returns None if not found.
fn find_psr4_block(text: &str, section: &str) -> Option<String> {
    let section_key = format!("\"{}\"", section);
    let section_pos = text.find(&section_key)?;
    let after_section = &text[section_pos + section_key.len()..];
    let psr4_pos = after_section.find("\"psr-4\"")?;
    let after_psr4 = &after_section[psr4_pos + "\"psr-4\"".len()..];
    let open_brace = after_psr4.find('{')?;

    let body_start = open_brace + 1;
    let body_bytes = &after_psr4.as_bytes()[body_start..];
    let mut depth: i32 = 1;
    let mut end: Option<usize> = None;
    for (i, &b) in body_bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;

    Some(after_psr4[body_start..body_start + end].to_owned())
}

/// Parse the body of a PSR-4 block, returning `(namespace_prefix, dir_prefix)`
/// pairs. Accepts both `"App\\": "src/"` and `"App\\": ["src/"]` (first entry
/// of an array wins). Tolerates whitespace and unknown keys.
fn parse_psr4_pairs(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let key_start = i + 1;
        let key_end = match find_unescaped_quote(bytes, key_start) {
            Some(p) => p,
            None => break,
        };
        let key = &body[key_start..key_end];
        i = key_end + 1;

        while i < bytes.len() && bytes[i] != b':' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        i += 1;

        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        let value = if i < bytes.len() && bytes[i] == b'"' {
            let v_start = i + 1;
            let v_end = match find_unescaped_quote(bytes, v_start) {
                Some(p) => p,
                None => break,
            };
            let v = body[v_start..v_end].to_owned();
            i = v_end + 1;
            Some(v)
        } else if i < bytes.len() && bytes[i] == b'[' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b']' {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'"' {
                let v_start = i + 1;
                let v_end = match find_unescaped_quote(bytes, v_start) {
                    Some(p) => p,
                    None => break,
                };
                let v = body[v_start..v_end].to_owned();
                i = v_end + 1;
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                Some(v)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(v) = value {
            let key_unescaped = unescape_backslashes(key);
            out.push((key_unescaped, v));
        }
    }

    out
}

fn find_unescaped_quote(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn unescape_backslashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '\\' {
                    out.push('\\');
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resolver() -> ModuleResolver {
        let mut r = ModuleResolver::new(Path::new("/repo"));
        // Register a set of local modules.
        for m in &[
            "app",
            "app.main",
            "app.services",
            "app.services.llm",
            "app.models",
            "app.models.user",
            "src.index",
            "src.services",
            "src.services.user",
            "src.lib.api",
        ] {
            r.register_module(m);
        }
        r
    }

    // -----------------------------------------------------------------------
    // Direct module names
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_direct_known_module() {
        let r = make_resolver();
        let (id, is_local, _) = r.resolve("app.services.llm", "app.main", false);
        assert_eq!(id, "app.services.llm");
        assert!(is_local, "registered module should be local");
    }

    #[test]
    fn resolve_direct_unknown_module() {
        let r = make_resolver();
        let (id, is_local, _) = r.resolve("os", "app.main", false);
        assert_eq!(id, "os");
        assert!(!is_local, "'os' is not a local module");
    }

    // -----------------------------------------------------------------------
    // Python relative imports (non-package modules)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_python_relative_single_dot() {
        // `.utils` from `app.services.llm` → `app.services.utils`
        let r = make_resolver();
        let (id, _, _) = r.resolve(".utils", "app.services.llm", false);
        assert_eq!(id, "app.services.utils");
    }

    #[test]
    fn resolve_python_relative_double_dot() {
        // `..models` from `app.services.llm` → `app.models`
        let r = make_resolver();
        let (id, _, _) = r.resolve("..models", "app.services.llm", false);
        assert_eq!(id, "app.models");
    }

    #[test]
    fn resolve_python_relative_known_module_is_local() {
        // `.models` from `app.services.llm` → `app.services.models`
        // Register it so it's local.
        let mut r = make_resolver();
        r.register_module("app.services.models");
        let (id, is_local, _) = r.resolve(".models", "app.services.llm", false);
        assert_eq!(id, "app.services.models");
        assert!(is_local);
    }

    #[test]
    fn resolve_python_relative_bare_dot() {
        // `.` (bare relative) from `app.services.llm` → `app.services`
        let r = make_resolver();
        let (id, _, _) = r.resolve(".", "app.services.llm", false);
        assert_eq!(id, "app.services");
    }

    // -----------------------------------------------------------------------
    // Python relative imports (package / __init__.py modules)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_python_relative_from_init_single_dot() {
        // `.llm` from `app.errors` (__init__.py) → `app.errors.llm`
        let mut r = make_resolver();
        r.register_module("app.errors");
        r.register_module("app.errors.llm");
        let (id, _, _) = r.resolve(".llm", "app.errors", true);
        assert_eq!(id, "app.errors.llm");
    }

    #[test]
    fn resolve_python_relative_from_init_double_dot() {
        // `..models` from `app.errors` (__init__.py) → `app.models`
        let r = make_resolver();
        let (id, _, _) = r.resolve("..models", "app.errors", true);
        assert_eq!(id, "app.models");
    }

    #[test]
    fn resolve_python_relative_from_init_bare_dot() {
        // `.` from `app.errors` (__init__.py) → `app.errors`
        let r = make_resolver();
        let (id, _, _) = r.resolve(".", "app.errors", true);
        assert_eq!(id, "app.errors");
    }

    #[test]
    fn resolve_python_relative_from_init_no_false_walk() {
        // This is the exact BUG-001 scenario:
        // `.llm` from `app.errors` (is_package=true) should NOT resolve to `app.llm`.
        let mut r = make_resolver();
        r.register_module("app.errors");
        r.register_module("app.errors.llm");
        r.register_module("app.llm");
        let (id, _, _) = r.resolve(".llm", "app.errors", true);
        assert_eq!(id, "app.errors.llm", "BUG-001: must NOT resolve to app.llm");
    }

    // -----------------------------------------------------------------------
    // TypeScript path aliases
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_ts_alias() {
        // `@/lib/api` with alias `@/*` → `src/*` → `src.lib.api`
        let mut r = make_resolver();
        r.ts_aliases.push(("@/*".to_owned(), "src/*".to_owned()));
        let (id, is_local, _) = r.resolve("@/lib/api", "src.index", false);
        assert_eq!(id, "src.lib.api");
        assert!(is_local, "src.lib.api is registered as local");
    }

    #[test]
    fn resolve_ts_alias_unknown_target() {
        // Alias resolves to something not registered → is_local=false.
        let mut r = make_resolver();
        r.ts_aliases.push(("@/*".to_owned(), "src/*".to_owned()));
        let (id, is_local, _) = r.resolve("@/unknown/module", "src.index", false);
        assert_eq!(id, "src.unknown.module");
        assert!(!is_local);
    }

    // -----------------------------------------------------------------------
    // BUG-007: Workspace alias resolution
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_ts_workspace_alias_preserves_original_name() {
        // BUG-007: `@repo/validators` with alias pointing outside project
        // should keep the original import string, not produce `..packages.validators`.
        let mut r = make_resolver();
        r.ts_aliases
            .push(("@repo/*".to_owned(), "../../packages/*".to_owned()));
        let (id, is_local, _) = r.resolve("@repo/validators", "src.index", false);
        assert_eq!(
            id, "@repo/validators",
            "BUG-007: workspace alias must preserve original import name"
        );
        assert!(!is_local);
    }

    #[test]
    fn resolve_ts_workspace_alias_with_subpath() {
        // `@repo/validators/mentorship` → should still preserve original name.
        let mut r = make_resolver();
        r.ts_aliases
            .push(("@repo/*".to_owned(), "../../packages/*".to_owned()));
        let (id, is_local, _) = r.resolve("@repo/validators/mentorship", "src.index", false);
        assert_eq!(id, "@repo/validators/mentorship");
        assert!(!is_local);
    }

    #[test]
    fn resolve_ts_workspace_exact_alias_preserves_name() {
        // Exact alias to external path: `@repo/validators` → `../../packages/validators/src`
        let mut r = make_resolver();
        r.ts_aliases.push((
            "@repo/validators".to_owned(),
            "../../packages/validators/src".to_owned(),
        ));
        let (id, is_local, _) = r.resolve("@repo/validators", "src.index", false);
        assert_eq!(id, "@repo/validators");
        assert!(!is_local);
    }

    #[test]
    fn resolve_ts_alias_with_dot_slash_prefix() {
        // Alias target with `./` prefix should be stripped before dot conversion.
        let mut r = make_resolver();
        r.ts_aliases.push(("@/*".to_owned(), "./src/*".to_owned()));
        r.register_module("src.lib.api");
        let (id, is_local, _) = r.resolve("@/lib/api", "src.index", false);
        assert_eq!(id, "src.lib.api");
        assert!(is_local);
    }

    #[test]
    fn resolve_ts_internal_alias_does_not_capture_scoped_package_imports() {
        // BUG-011: `@/*` must only match imports that start with `@/`.
        // It must NOT rewrite external scoped packages like `@repo/logger`.
        let mut r = make_resolver();
        r.ts_aliases.push(("@/*".to_owned(), "src/*".to_owned()));
        let (id, is_local, _) = r.resolve("@repo/logger", "src.index", false);
        assert_eq!(id, "@repo/logger");
        assert!(!is_local);
    }

    #[test]
    fn resolve_ts_alias_from_parent_tsconfig_maps_to_registered_module() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path().join("menubar");
        let src_root = repo_root.join("src");
        let types_dir = src_root.join("types");
        let components_dir = src_root.join("components");
        std::fs::create_dir_all(&types_dir).unwrap();
        std::fs::create_dir_all(&components_dir).unwrap();

        let tsconfig_path = repo_root.join("tsconfig.json");
        std::fs::write(
            &tsconfig_path,
            r#"{
  "compilerOptions": {
    "paths": {
      "@/*": ["./src/*"]
    }
  }
}"#,
        )
        .unwrap();

        let mut r = ModuleResolver::new(&src_root);
        r.register_module_path("menubar.types", &types_dir.join("index.ts"), true);
        r.register_module_path(
            "menubar.components.Foo",
            &components_dir.join("Foo.tsx"),
            false,
        );
        r.load_tsconfig_for_module("menubar.components.Foo", &tsconfig_path);

        let (id, is_local, _) = r.resolve("@/types", "menubar.components.Foo", false);
        assert_eq!(id, "menubar.types");
        assert!(is_local);
    }

    // -----------------------------------------------------------------------
    // TypeScript relative imports
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_ts_relative_same_dir() {
        // `./services/user` from `src.index` → `src.services.user`
        let r = make_resolver();
        let (id, is_local, _) = r.resolve("./services/user", "src.index", false);
        assert_eq!(id, "src.services.user");
        assert!(is_local, "src.services.user is registered");
    }

    #[test]
    fn resolve_ts_relative_parent() {
        // `../lib/api` from `src.services.user` → `src.lib.api`
        let r = make_resolver();
        let (id, is_local, _) = r.resolve("../lib/api", "src.services.user", false);
        assert_eq!(id, "src.lib.api");
        assert!(is_local, "src.lib.api is registered");
    }

    #[test]
    fn resolve_ts_relative_strips_js_suffix() {
        // TS-ESM / "moduleResolution": "bundler" imports carry a `.js` suffix
        // that resolves to `.ts` source. `./services/user.js` → `src.services.user`.
        let r = make_resolver();
        let (id, is_local, _) = r.resolve("./services/user.js", "src.index", false);
        assert_eq!(id, "src.services.user");
        assert!(is_local, "suffix-stripped path should match known module");
    }

    #[test]
    fn resolve_ts_relative_strips_tsx_suffix() {
        // Order-sensitive: `.tsx` must strip before `.ts`.
        let r = make_resolver();
        let (id, _, _) = r.resolve("./services/user.tsx", "src.index", false);
        assert_eq!(id, "src.services.user");
    }

    #[test]
    fn resolve_ts_relative_strips_mjs_suffix() {
        let r = make_resolver();
        let (id, _, _) = r.resolve("./services/user.mjs", "src.index", false);
        assert_eq!(id, "src.services.user");
    }

    #[test]
    fn resolve_ts_relative_no_suffix_unchanged() {
        // Regression guard: plain `./foo` without extension must still resolve.
        let r = make_resolver();
        let (id, _, _) = r.resolve("./services/user", "src.index", false);
        assert_eq!(id, "src.services.user");
    }

    #[test]
    fn resolve_ts_relative_unknown_extension_kept() {
        // Non-TS extensions (e.g. `.css`, `.json`) are not stripped.
        // `./styles.css` should remain as-is so it lands as an external node.
        let r = make_resolver();
        let (id, _, _) = r.resolve("./styles.css", "src.index", false);
        assert_eq!(id, "src.styles.css");
    }

    // -----------------------------------------------------------------------
    // load_tsconfig
    // -----------------------------------------------------------------------

    #[test]
    fn load_tsconfig_extracts_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let tsconfig_path = tmp.path().join("tsconfig.json");
        std::fs::write(
            &tsconfig_path,
            r#"{
  "compilerOptions": {
    "target": "ES2020",
    "paths": {
      "@/*": ["src/*"],
      "@lib/*": ["src/lib/*"]
    }
  }
}"#,
        )
        .unwrap();

        let mut r = ModuleResolver::new(tmp.path());
        r.load_tsconfig(&tsconfig_path);

        assert!(
            r.ts_aliases
                .contains(&("@/*".to_owned(), "src/*".to_owned())),
            "expected @/* → src/* alias, got: {:?}",
            r.ts_aliases
        );
        assert!(
            r.ts_aliases
                .contains(&("@lib/*".to_owned(), "src/lib/*".to_owned())),
            "expected @lib/* → src/lib/* alias, got: {:?}",
            r.ts_aliases
        );
    }

    // -----------------------------------------------------------------------
    // Confidence scores
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_direct_known_returns_confidence_1() {
        let r = make_resolver();
        let (_, _, confidence) = r.resolve("app.services.llm", "app.main", false);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn resolve_direct_unknown_returns_confidence_1() {
        let r = make_resolver();
        let (_, _, confidence) = r.resolve("os", "app.main", false);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn resolve_python_relative_returns_confidence_09() {
        let r = make_resolver();
        let (_, _, confidence) = r.resolve(".utils", "app.services.llm", false);
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_ts_alias_returns_confidence_085() {
        let mut r = make_resolver();
        r.ts_aliases.push(("@/*".to_owned(), "src/*".to_owned()));
        let (_, _, confidence) = r.resolve("@/lib/api", "src.index", false);
        assert!((confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_ts_relative_returns_confidence_09() {
        let r = make_resolver();
        let (_, _, confidence) = r.resolve("./services/user", "src.index", false);
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_ts_workspace_alias_returns_confidence_085() {
        let mut r = make_resolver();
        r.ts_aliases
            .push(("@repo/*".to_owned(), "../../packages/*".to_owned()));
        let (_, _, confidence) = r.resolve("@repo/validators", "src.index", false);
        assert!((confidence - 0.85).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Go module-path imports
    // -----------------------------------------------------------------------

    fn make_go_resolver() -> ModuleResolver {
        let mut r = ModuleResolver::new(Path::new("/repo"));
        r.go_module_path = Some("github.com/test/myapp".to_owned());
        for m in &["myapp.cmd.server", "myapp.pkg.handler", "myapp.pkg.db"] {
            r.register_module(m);
        }
        r
    }

    #[test]
    fn resolve_go_local_import() {
        let r = make_go_resolver();
        let (id, is_local, confidence) =
            r.resolve("github.com/test/myapp/pkg/handler", "cmd.server", false);
        assert_eq!(id, "myapp.pkg.handler");
        assert!(is_local);
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_go_local_nested_import() {
        let r = make_go_resolver();
        let (id, is_local, _) = r.resolve("github.com/test/myapp/pkg/db", "cmd.server", false);
        assert_eq!(id, "myapp.pkg.db");
        assert!(is_local);
    }

    #[test]
    fn resolve_go_external_import() {
        let r = make_go_resolver();
        let (id, is_local, confidence) = r.resolve("fmt", "cmd.server", false);
        assert_eq!(id, "fmt");
        assert!(!is_local);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn resolve_go_external_third_party() {
        let r = make_go_resolver();
        let (id, is_local, _) = r.resolve("github.com/other/lib/pkg", "cmd.server", false);
        assert_eq!(id, "github.com/other/lib/pkg");
        assert!(!is_local);
    }

    #[test]
    fn load_go_mod_extracts_module_path() {
        let tmp = tempfile::tempdir().unwrap();
        let go_mod_path = tmp.path().join("go.mod");
        std::fs::write(
            &go_mod_path,
            "module github.com/test/goproject\n\ngo 1.21\n\nrequire (\n\tgithub.com/lib/pq v1.10.0\n)\n",
        )
        .unwrap();

        let mut r = ModuleResolver::new(tmp.path());
        r.load_go_mod(&go_mod_path);
        assert_eq!(
            r.go_module_path,
            Some("github.com/test/goproject".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Rust crate/super/self imports
    // -----------------------------------------------------------------------

    fn make_rust_resolver() -> ModuleResolver {
        // Mirrors a real Rust crate after walker auto-detects local_prefix="src":
        // every registered module name carries the "src" prefix, so the
        // resolver must know about the prefix to canonicalize `crate::` paths.
        let mut r = ModuleResolver::new(Path::new("/repo"));
        r.set_local_prefix("src");
        for m in &[
            "src",
            "src.handler",
            "src.services",
            "src.services.db",
            "src.models",
            "src.models.user",
        ] {
            r.register_module(m);
        }
        r
    }

    #[test]
    fn resolve_rust_crate_import() {
        let r = make_rust_resolver();
        let (id, is_local, confidence) = r.resolve("crate::handler", "src.services.db", false);
        // BUG-016: `crate::` is rooted at the crate's source root, which the
        // walker names with `local_prefix`. Pre-fix this returned "handler"
        // (non-local placeholder) — now correctly canonicalized to "src.handler".
        assert_eq!(id, "src.handler");
        assert!(is_local);
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_rust_crate_nested_import() {
        let r = make_rust_resolver();
        let (id, is_local, confidence) = r.resolve("crate::models::user", "src.handler", false);
        assert_eq!(id, "src.models.user");
        assert!(is_local);
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_rust_crate_import_smoking_gun_bug_016() {
        // The exact case observed when dogfooding graphify on graphify-core:
        // `crate::types::Node` from any module in the crate must resolve to
        // `src.types.Node` (the canonical local id) so intra-crate edges land
        // on the local node and contribute to in-degree / hotspot scoring.
        let mut r = ModuleResolver::new(Path::new("/repo"));
        r.set_local_prefix("src");
        r.register_module("src.types");
        r.register_module("src.types.Node");
        r.register_module("src.graph");

        let (id, is_local, _) = r.resolve("crate::types::Node", "src.graph", false);
        assert_eq!(id, "src.types.Node");
        assert!(is_local, "intra-crate `crate::` reference must be local");
    }

    #[test]
    fn resolve_rust_crate_import_no_prefix_unchanged() {
        // Empty local_prefix (legacy/unset) must keep the pre-fix behaviour:
        // strip `crate::` and replace `::` with `.`, no prepend.
        let mut r = ModuleResolver::new(Path::new("/repo"));
        r.register_module("handler");

        let (id, is_local, _) = r.resolve("crate::handler", "services.db", false);
        assert_eq!(id, "handler");
        assert!(is_local);
    }

    #[test]
    fn resolve_rust_super_import() {
        // super:: from db.rs goes up to services level
        let r = make_rust_resolver();
        let (id, _, confidence) = r.resolve("super::handler", "src.services.db", false);
        assert_eq!(id, "src.services.handler");
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_rust_self_import() {
        let r = make_rust_resolver();
        let (id, _, confidence) = r.resolve("self::db", "src.services", true);
        assert_eq!(id, "src.services.db");
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_rust_super_super_import() {
        let r = make_rust_resolver();
        let (id, _, _) = r.resolve("super::super::models", "src.services.db", false);
        assert_eq!(id, "src.models");
    }

    #[test]
    fn resolve_rust_external_crate() {
        let r = make_rust_resolver();
        let (id, is_local, confidence) = r.resolve("serde", "src.handler", false);
        assert_eq!(id, "serde");
        assert!(!is_local);
        assert_eq!(confidence, 1.0);
    }

    // -----------------------------------------------------------------------
    // load_composer_json
    // -----------------------------------------------------------------------

    #[test]
    fn load_composer_json_parses_psr4_mappings() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("composer.json");
        std::fs::write(
            &path,
            r#"{
  "name": "vendor/pkg",
  "autoload": {
    "psr-4": {
      "App\\": "src/",
      "App\\Legacy\\": "src/legacy/"
    }
  }
}"#,
        )
        .unwrap();

        let mut resolver = ModuleResolver::new(tmp.path());
        resolver.load_composer_json(&path);

        let mappings = resolver.psr4_mappings();
        assert!(
            mappings
                .iter()
                .any(|(ns, dir)| ns == "App\\" && dir == "src/"),
            "App\\ → src/ mapping; got {:?}",
            mappings
        );
        assert!(
            mappings
                .iter()
                .any(|(ns, dir)| ns == "App\\Legacy\\" && dir == "src/legacy/"),
            "App\\Legacy\\ → src/legacy/ mapping; got {:?}",
            mappings
        );
    }

    #[test]
    fn load_composer_json_merges_autoload_dev() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("composer.json");
        std::fs::write(
            &path,
            r#"{
  "autoload": { "psr-4": { "App\\": "src/" } },
  "autoload-dev": { "psr-4": { "Tests\\": "tests/" } }
}"#,
        )
        .unwrap();

        let mut resolver = ModuleResolver::new(tmp.path());
        resolver.load_composer_json(&path);

        let mappings = resolver.psr4_mappings();
        assert!(mappings.iter().any(|(ns, _)| ns == "App\\"));
        assert!(mappings.iter().any(|(ns, _)| ns == "Tests\\"));
    }

    #[test]
    fn load_composer_json_handles_malformed_without_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("composer.json");
        std::fs::write(&path, "{ this is not valid json").unwrap();

        let mut resolver = ModuleResolver::new(tmp.path());
        resolver.load_composer_json(&path); // must not panic

        let mappings = resolver.psr4_mappings();
        assert!(mappings.is_empty(), "malformed file leaves mappings empty");
    }

    #[test]
    fn load_composer_json_missing_file_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let mut resolver = ModuleResolver::new(tmp.path());
        resolver.load_composer_json(&path); // must not panic

        assert!(resolver.psr4_mappings().is_empty());
    }

    // -----------------------------------------------------------------------
    // PHP `use` targets (backslash-separated namespaces)
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_php_use_matches_known_module() {
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.register_module("App.Services.Llm");

        let (resolved, is_local, confidence) =
            resolver.resolve("App\\Services\\Llm", "App.Main", false);
        assert_eq!(resolved, "App.Services.Llm");
        assert!(is_local);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn resolve_php_use_nonlocal_still_extracted_confidence() {
        let resolver = ModuleResolver::new(Path::new("/repo"));
        let (resolved, is_local, confidence) =
            resolver.resolve("Symfony\\HttpFoundation\\Request", "App.Main", false);
        assert_eq!(resolved, "Symfony.HttpFoundation.Request");
        assert!(!is_local);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn resolve_php_use_strips_leading_backslash() {
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.register_module("App.Models.User");

        let (resolved, is_local, _) = resolver.resolve("\\App\\Models\\User", "App.Main", false);
        assert_eq!(resolved, "App.Models.User");
        assert!(is_local);
    }

    // -----------------------------------------------------------------------
    // FEAT-028 step 4 — workspace-aware tsconfig alias resolution
    // -----------------------------------------------------------------------

    use crate::workspace_reexport::{
        ProjectReExportContext, WorkspaceAliasTarget, WorkspaceReExportGraph,
    };

    /// Build a workspace containing a `core` project whose `src/index.ts`
    /// (package entry) and `src/foo.ts` (regular) are registered. Used by
    /// the three cross-project tests below.
    fn core_only_workspace() -> WorkspaceReExportGraph {
        let mut core = ProjectReExportContext::new(
            "core",
            "/abs/packages/core",
            vec!["core.src.index".to_owned(), "core.src.foo".to_owned()],
            vec![],
        );
        core.add_module_path(
            "core.src.index",
            Path::new("/abs/packages/core/src/index.ts"),
            /* is_package */ true,
        );
        core.add_module_path(
            "core.src.foo",
            Path::new("/abs/packages/core/src/foo.ts"),
            false,
        );

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(core);
        ws
    }

    #[test]
    fn apply_ts_alias_workspace_returns_none_for_inside_root_target() {
        // Alias target falls inside the current project's root — the
        // existing per-project resolver handles this locally. The
        // workspace variant must return `None` so the caller (step 5 fan-
        // out) doesn't double-process.
        let mut r = ModuleResolver::new(Path::new("/abs/apps/consumer"));
        r.ts_aliases_by_module.insert(
            "src.main".to_owned(),
            vec![TsAliasContext {
                alias_pattern: "@/*".to_owned(),
                target_pattern: "src/*".to_owned(),
                base_dir: PathBuf::from("/abs/apps/consumer"),
            }],
        );
        let ws = WorkspaceReExportGraph::new();

        assert_eq!(
            r.apply_ts_alias_workspace("@/lib/api", "src.main", &ws),
            None
        );
    }

    #[test]
    fn apply_ts_alias_workspace_crosses_boundary_to_sibling_project() {
        // The cross-project scenario that FEAT-027 pinned as v1-unsupported.
        // Consumer's exact alias `@repo/core` → `../../packages/core/src`
        // points at the sibling core project's `src` package-entry
        // directory. Step 4's workspace resolver should return
        // `(core, core.src.index)` — the package-entry variant registered
        // via `add_module_path(is_package = true)`.
        //
        // (The in-tree fixture's tsconfig uses an inner-glob form
        // `"@repo/*": ["../../packages/*/src"]` that `match_alias_target`
        // does not handle today — pre-existing limitation, orthogonal to
        // FEAT-028. Step 5's fan-out will inherit whatever alias forms the
        // underlying matcher supports; broadening that matcher is
        // out-of-scope here.)
        let mut r = ModuleResolver::new(Path::new("/abs/apps/consumer"));
        r.ts_aliases_by_module.insert(
            "consumer.src.main".to_owned(),
            vec![TsAliasContext {
                alias_pattern: "@repo/core".to_owned(),
                target_pattern: "../../packages/core/src".to_owned(),
                base_dir: PathBuf::from("/abs/apps/consumer"),
            }],
        );
        let ws = core_only_workspace();

        let got = r.apply_ts_alias_workspace("@repo/core", "consumer.src.main", &ws);
        assert_eq!(
            got,
            Some(WorkspaceAliasTarget {
                project: "core".to_owned(),
                module_id: "core.src.index".to_owned(),
            }),
            "workspace alias must resolve to core's package-entry module id"
        );
    }

    #[test]
    fn apply_ts_alias_workspace_returns_none_for_outside_all_projects() {
        // Alias target lands in a directory that no workspace project owns
        // (e.g. pointing into `node_modules` or an unrelated filesystem
        // location). The caller should fall back to the v1 raw-alias
        // behaviour — `None` is the signal for that.
        let mut r = ModuleResolver::new(Path::new("/abs/apps/consumer"));
        r.ts_aliases_by_module.insert(
            "consumer.src.main".to_owned(),
            vec![TsAliasContext {
                alias_pattern: "@vendor/*".to_owned(),
                target_pattern: "../../vendor/*".to_owned(),
                base_dir: PathBuf::from("/abs/apps/consumer"),
            }],
        );
        let ws = core_only_workspace();

        assert_eq!(
            r.apply_ts_alias_workspace("@vendor/something", "consumer.src.main", &ws),
            None,
        );
    }

    #[test]
    fn apply_ts_alias_workspace_glob_expansion_matches_sibling_submodule() {
        // Multi-level glob: `@repo/*` → `../../packages/*/src`. Consumer
        // imports `@repo/core/foo` — the `*` captures `core/foo`, appended
        // to `../../packages/` and then `/src` is... wait, actually the
        // alias is `../../packages/*/src` so `@repo/core/foo` expands to
        // `../../packages/core/foo/src`, which is NOT what we want. The
        // real test case here is the single-level form (used in practice):
        // `@repo/core` → `packages/core/src`. A deeper import would hit a
        // submodule path. Cover the submodule-direct-match variant using a
        // simpler alias form — `@repo/*` → `../../packages/*` — which
        // naturally expands `@repo/core/foo` to `../../packages/core/foo`.
        let mut r = ModuleResolver::new(Path::new("/abs/apps/consumer"));
        r.ts_aliases_by_module.insert(
            "consumer.src.main".to_owned(),
            vec![TsAliasContext {
                alias_pattern: "@repo/*".to_owned(),
                target_pattern: "../../packages/*".to_owned(),
                base_dir: PathBuf::from("/abs/apps/consumer"),
            }],
        );
        let ws = core_only_workspace();

        let got = r.apply_ts_alias_workspace("@repo/core/src/foo", "consumer.src.main", &ws);
        assert_eq!(
            got,
            Some(WorkspaceAliasTarget {
                project: "core".to_owned(),
                module_id: "core.src.foo".to_owned(),
            }),
            "deep glob expansion must match the sibling project's submodule"
        );
    }

    #[test]
    fn apply_ts_alias_workspace_uses_global_aliases_with_self_root_anchor() {
        // When no per-module tsconfig context is registered, the
        // workspace-aware resolver falls back to the resolver's global
        // `ts_aliases` using `self.root` as the base dir — same behaviour
        // as the per-project resolver. Verifies parity with the global
        // alias path and the `self.root` anchor. Uses an exact-match alias
        // since `match_alias_target` only handles trailing-`*` globs.
        let mut r = ModuleResolver::new(Path::new("/abs/apps/consumer"));
        r.ts_aliases.push((
            "@repo/core".to_owned(),
            "../../packages/core/src".to_owned(),
        ));
        let ws = core_only_workspace();

        let got = r.apply_ts_alias_workspace("@repo/core", "consumer.src.main", &ws);
        assert_eq!(
            got,
            Some(WorkspaceAliasTarget {
                project: "core".to_owned(),
                module_id: "core.src.index".to_owned(),
            }),
        );
    }

    // ---- match_alias_target: inner-glob support (FEAT-028 slice 4) ----

    #[test]
    fn match_alias_target_inner_glob_expands_capture_into_middle_of_target() {
        // pnpm-style workspace: `"@repo/*": ["../../packages/*/src"]`.
        // Consumer imports `@repo/core` — the glob captures `core`, which
        // is spliced into the target's inner `*` position, yielding
        // `../../packages/core/src`.
        let got = match_alias_target("@repo/core", "@repo/*", "../../packages/*/src");
        assert_eq!(got, Some("../../packages/core/src".to_owned()));
    }

    #[test]
    fn match_alias_target_inner_glob_with_deep_import_captures_full_tail() {
        // `@repo/core/foo` with `"@repo/*": ["../../packages/*/src"]` —
        // capture is `core/foo`, result is `../../packages/core/foo/src`.
        // Not a typical tsconfig target shape (inner-glob with trailing
        // segment usually targets a package root, not a nested path), but
        // the matcher should still honour the literal pattern.
        let got = match_alias_target("@repo/core/foo", "@repo/*", "../../packages/*/src");
        assert_eq!(got, Some("../../packages/core/foo/src".to_owned()));
    }

    #[test]
    fn match_alias_target_trailing_glob_still_works() {
        // Regression: the classic trailing-`*` form used by
        // `"@/*": ["src/*"]` must keep resolving unchanged after the
        // inner-glob rewrite.
        let got = match_alias_target("@/lib/api", "@/*", "src/*");
        assert_eq!(got, Some("src/lib/api".to_owned()));
    }

    #[test]
    fn match_alias_target_inner_glob_rejects_non_matching_prefix() {
        // Consumer imports `@vendor/core`, but the alias is `@repo/*` →
        // the prefix doesn't match, so we return `None` instead of
        // mis-capturing.
        let got = match_alias_target("@vendor/core", "@repo/*", "../../packages/*/src");
        assert_eq!(got, None);
    }

    // -----------------------------------------------------------------------
    // FEAT-031: use_aliases fallback for Rust scoped and bare-name calls
    // -----------------------------------------------------------------------

    #[test]
    fn feat_031_use_alias_scoped_call_resolves_to_local_method() {
        // Setup: `src.graph` has `use crate::types::Node;` and calls
        // `Node::module(...)`. The resolver should rewrite `Node::module`
        // through the alias map to `crate::types::Node::module`, then the
        // `crate::` branch canonicalizes to `src.types.Node.module`.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.types");
        resolver.register_module("src.types.Node");
        resolver.register_module("src.types.Node.module");
        resolver.register_module("src.graph");

        let mut aliases = HashMap::new();
        aliases.insert("Node".to_owned(), "crate::types::Node".to_owned());
        resolver.register_use_aliases("src.graph", &aliases);

        let (resolved, is_local, _conf) = resolver.resolve("Node::module", "src.graph", false);
        assert_eq!(resolved, "src.types.Node.module");
        assert!(is_local);
    }

    #[test]
    fn feat_031_use_alias_bare_call_resolves_to_local_function() {
        // `use crate::validator::validate;` + bare `validate()` in body.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.validator");
        resolver.register_module("src.validator.validate");
        resolver.register_module("src.main");

        let mut aliases = HashMap::new();
        aliases.insert(
            "validate".to_owned(),
            "crate::validator::validate".to_owned(),
        );
        resolver.register_use_aliases("src.main", &aliases);

        let (resolved, is_local, _conf) = resolver.resolve("validate", "src.main", false);
        assert_eq!(resolved, "src.validator.validate");
        assert!(is_local);
    }

    #[test]
    fn feat_031_use_alias_no_match_leaves_bare_name_nonlocal() {
        // Negative: `Vec::new()` with no `use Vec` must not be spuriously
        // promoted to a local symbol.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.graph");

        let (resolved, is_local, _conf) = resolver.resolve("Vec::new", "src.graph", false);
        assert_eq!(resolved, "Vec::new");
        assert!(!is_local, "no alias → stays external");
    }

    #[test]
    fn feat_031_use_alias_scoped_prefix_but_unknown_tail_stays_resolved() {
        // `use crate::types::Node;` exists but the call is `Node::unknown_fn`
        // (not a registered symbol). The rewrite still happens; `is_local`
        // reflects whether the rewritten id is in known_modules.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.types");
        resolver.register_module("src.types.Node");
        resolver.register_module("src.graph");

        let mut aliases = HashMap::new();
        aliases.insert("Node".to_owned(), "crate::types::Node".to_owned());
        resolver.register_use_aliases("src.graph", &aliases);

        let (resolved, is_local, _conf) = resolver.resolve("Node::unknown_fn", "src.graph", false);
        assert_eq!(resolved, "src.types.Node.unknown_fn");
        assert!(
            !is_local,
            "rewritten id not in known_modules → not marked local"
        );
    }

    #[test]
    fn feat_031_use_alias_rewrite_is_bounded_against_self_referential_alias() {
        // Regression guard for the v0.11.4 OOM (BUG-017): if an alias value
        // starts with the alias key, the naive recursive rewrite at case 9
        // grows the string forever (`X::foo` → `X::Y::foo` → `X::Y::Y::foo`
        // → …). The depth cap must return a finite, non-local result even
        // for this pathological input, without allocating unbounded memory.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.main");

        let mut aliases = HashMap::new();
        // Pathological: the alias value's first segment equals the key.
        aliases.insert("X".to_owned(), "X::Y".to_owned());
        resolver.register_use_aliases("src.main", &aliases);

        // Must return in bounded time with a finite resolved id; exact value
        // is less important than the finite-time contract. The id should
        // reflect the last rewrite attempt before the cap fired (non-local).
        let (resolved, is_local, _conf) = resolver.resolve("X::foo", "src.main", false);
        assert!(
            !is_local,
            "self-referential alias chain must not be treated as local"
        );
        assert!(
            !resolved.is_empty(),
            "resolved id must be a non-empty string"
        );
        // String should not have grown to any huge length. Even a few
        // rewrite iterations with an allowed depth of 4 can produce at
        // most 4 `::Y` insertions, so length stays well under 200 chars.
        assert!(
            resolved.len() < 256,
            "resolved id length {} suggests unbounded rewrite (expected bounded by depth cap)",
            resolved.len(),
        );
    }

    #[test]
    fn feat_031_use_alias_bare_name_self_reference_is_bounded() {
        // Even simpler pathology: bare `use X;` (single-segment) registered
        // as `("X", "X")`. Bare-name rewrite must not infinite-loop.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.main");

        let mut aliases = HashMap::new();
        aliases.insert("X".to_owned(), "X".to_owned());
        resolver.register_use_aliases("src.main", &aliases);

        let (resolved, is_local, _conf) = resolver.resolve("X", "src.main", false);
        assert!(!is_local);
        assert_eq!(
            resolved, "X",
            "self-referential bare alias collapses to the raw name"
        );
    }

    #[test]
    fn feat_031_use_alias_per_module_scope_does_not_leak() {
        // An alias registered on module A must NOT fire for a call from
        // module B. Two modules, only A has the `Node` alias.
        let mut resolver = ModuleResolver::new(Path::new("/repo"));
        resolver.set_local_prefix("src");
        resolver.register_module("src.types");
        resolver.register_module("src.types.Node");
        resolver.register_module("src.a");
        resolver.register_module("src.b");

        let mut aliases_a = HashMap::new();
        aliases_a.insert("Node".to_owned(), "crate::types::Node".to_owned());
        resolver.register_use_aliases("src.a", &aliases_a);

        // From `src.b` — no alias registered → Node stays external.
        let (resolved, is_local, _) = resolver.resolve("Node", "src.b", false);
        assert_eq!(resolved, "Node");
        assert!(!is_local, "alias must be per-source-module, not global");

        // From `src.a` — alias fires.
        let (resolved, is_local, _) = resolver.resolve("Node", "src.a", false);
        assert_eq!(resolved, "src.types.Node");
        assert!(is_local);
    }
}
