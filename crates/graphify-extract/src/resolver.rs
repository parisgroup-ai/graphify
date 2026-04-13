use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    /// TypeScript tsconfig path aliases: `(alias_pattern, target_pattern)`.
    /// Example: `("@/*", "src/*")`.
    ts_aliases: Vec<(String, String)>,
    /// Go module path from `go.mod` (e.g. `github.com/user/repo`).
    go_module_path: Option<String>,
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
            ts_aliases: Vec::new(),
            go_module_path: None,
            root: root.to_path_buf(),
        }
    }

    /// Register a dot-notation module name as a local module.
    pub fn register_module(&mut self, module_name: &str) {
        self.known_modules
            .insert(module_name.to_owned(), module_name.to_owned());
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
        let text = match std::fs::read_to_string(tsconfig_path) {
            Ok(t) => t,
            Err(_) => return,
        };

        // Find "paths" section — locate the key then capture between { }.
        let paths_start = match find_paths_section(&text) {
            Some(pos) => pos,
            None => return,
        };

        // Extract the brace-delimited block that follows "paths": {…}.
        let slice = &text[paths_start..];
        let block = match extract_brace_block(slice) {
            Some(b) => b,
            None => return,
        };

        // Parse key-value pairs inside the block.
        // Each pair looks like:  "alias": ["target", ...]
        let mut pos = 0;
        while pos < block.len() {
            // Find the next quoted key.
            let key = match extract_quoted_string(&block[pos..]) {
                Some((k, end)) => {
                    pos += end;
                    k
                }
                None => break,
            };

            // Skip past ':' then whitespace.
            if let Some(colon) = block[pos..].find(':') {
                pos += colon + 1;
            } else {
                break;
            }

            // Skip optional whitespace.
            while pos < block.len() && block.as_bytes()[pos].is_ascii_whitespace() {
                pos += 1;
            }

            // Expect '[' for the array of targets.
            if pos >= block.len() || block.as_bytes()[pos] != b'[' {
                continue;
            }
            pos += 1; // skip '['

            // Extract the first quoted string inside the array.
            let target = match extract_quoted_string(&block[pos..]) {
                Some((t, end)) => {
                    pos += end;
                    t
                }
                None => continue,
            };

            if !key.is_empty() && !target.is_empty() {
                self.ts_aliases.push((key, target));
            }
        }
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
        // 1. Python relative imports (start with one or more dots).
        if raw.starts_with('.') && !raw.starts_with("./") && !raw.starts_with("../") {
            let resolved = resolve_python_relative(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 2. TypeScript path aliases (e.g. `@/lib/api`).
        for (alias_pat, target_pat) in &self.ts_aliases {
            if let Some(resolved) = apply_ts_alias(raw, alias_pat, target_pat) {
                let is_local = self.known_modules.contains_key(&resolved);
                return (resolved, is_local, 0.85);
            }
        }

        // 3. TypeScript / generic relative imports (`./foo`, `../bar`).
        if raw.starts_with("./") || raw.starts_with("../") {
            let resolved = resolve_ts_relative(raw, from_module);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 4. Go module-path imports (strip go.mod module prefix).
        if let Some(ref go_mod) = self.go_module_path {
            if let Some(rest) = raw.strip_prefix(go_mod.as_str()) {
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                if !rest.is_empty() {
                    let resolved = rest.replace('/', ".");
                    let is_local = self.known_modules.contains_key(&resolved);
                    return (resolved, is_local, 0.9);
                }
            }
        }

        // 5. Rust `crate::`, `super::`, `self::` imports.
        if let Some(rest) = raw.strip_prefix("crate::") {
            let resolved = rest.replace("::", ".");
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }
        if raw.starts_with("super::") || raw.starts_with("self::") {
            let resolved = resolve_rust_path(raw, from_module, is_package);
            let is_local = self.known_modules.contains_key(&resolved);
            return (resolved, is_local, 0.9);
        }

        // 6. Direct module name — check against known modules.
        let is_local = self.known_modules.contains_key(raw);
        (raw.to_owned(), is_local, 1.0)
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
    // Keep the separator before `*` intact so `@/*` matches only `@/foo`,
    // not external scoped packages like `@repo/foo` (BUG-011).
    let alias_prefix = alias_pat.strip_suffix('*');
    let target_prefix = target_pat.strip_suffix('*');

    let resolved_path = match (alias_prefix, target_prefix) {
        (Some(ap), Some(tp)) => {
            // Wildcard alias: raw must start with ap.
            raw.strip_prefix(ap).map(|rest| format!("{}{}", tp, rest))
        }
        _ => {
            // Exact alias.
            if raw == alias_pat {
                Some(target_pat.to_owned())
            } else {
                None
            }
        }
    }?;

    // If the resolved path traverses outside the project, keep the original
    // import string as the node identifier (BUG-007).
    if resolved_path.contains("..") {
        return Some(raw.to_owned());
    }

    // Strip leading "./" before converting to dot notation.
    let clean = resolved_path.strip_prefix("./").unwrap_or(&resolved_path);
    Some(path_to_dot_notation(clean))
}

// ---------------------------------------------------------------------------
// TypeScript relative import resolution
// ---------------------------------------------------------------------------

/// Resolve a TypeScript relative import (`./foo` or `../bar`) from `from_module`.
///
/// Examples:
/// - `"./services/user"` from `"src.index"` → `"src.services.user"`
/// - `"../lib/api"` from `"src.services.user"` → `"src.lib.api"`
fn resolve_ts_relative(raw: &str, from_module: &str) -> String {
    // Split from_module and drop the leaf (current file).
    let mut parts: Vec<&str> = from_module.split('.').collect();
    if !parts.is_empty() {
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
        for m in &["cmd.server", "pkg.handler", "pkg.db"] {
            r.register_module(m);
        }
        r
    }

    #[test]
    fn resolve_go_local_import() {
        let r = make_go_resolver();
        let (id, is_local, confidence) =
            r.resolve("github.com/test/myapp/pkg/handler", "cmd.server", false);
        assert_eq!(id, "pkg.handler");
        assert!(is_local);
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_go_local_nested_import() {
        let r = make_go_resolver();
        let (id, is_local, _) = r.resolve("github.com/test/myapp/pkg/db", "cmd.server", false);
        assert_eq!(id, "pkg.db");
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
        let mut r = ModuleResolver::new(Path::new("/repo"));
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
        assert_eq!(id, "handler");
        // Note: "handler" is not in known_modules (it's "src.handler").
        // The crate:: prefix strips to root-relative, which is just "handler".
        // In practice, the registered module might be prefixed differently.
        assert!((confidence - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_rust_crate_nested_import() {
        let r = make_rust_resolver();
        let (id, _, confidence) = r.resolve("crate::models::user", "src.handler", false);
        assert_eq!(id, "models.user");
        assert!((confidence - 0.9).abs() < f64::EPSILON);
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
}
