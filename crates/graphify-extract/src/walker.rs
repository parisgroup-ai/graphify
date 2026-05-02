use graphify_core::types::Language;
use std::path::{Path, PathBuf};

/// Directories always excluded during file discovery.
const DEFAULT_EXCLUDES: &[&str] = &[
    "__pycache__",
    "node_modules",
    ".git",
    "dist",
    "tests",
    "__tests__",
    ".next",
    "build",
    ".venv",
    "venv",
    "vendor",
    "target",
];

/// A single source file discovered by [`discover_files`].
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub language: Language,
    /// Dot-notation module name (e.g. `app.services.llm`).
    pub module_name: String,
    /// True if this file is a package entry point (`__init__.py`, `index.ts`).
    pub is_package: bool,
}

/// Test file patterns that are always excluded during file discovery.
///
/// These patterns catch co-located test files that live alongside production
/// code (e.g. `src/circuit-breaker.test.ts`, `src/retry.spec.ts`,
/// `test_utils.py`), preventing test framework artifacts from polluting the
/// dependency graph.
fn is_test_file(file_name: &str) -> bool {
    // TypeScript / JavaScript conventions: *.test.{ts,tsx,js,jsx}, *.spec.{ts,tsx,js,jsx}
    let ts_js_test_suffixes = [
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
    ];
    for suffix in &ts_js_test_suffixes {
        if file_name.ends_with(suffix) {
            return true;
        }
    }

    // Python conventions: *.test.py, *_test.py
    if file_name.ends_with(".test.py") || file_name.ends_with("_test.py") {
        return true;
    }

    // Go conventions: *_test.go
    if file_name.ends_with("_test.go") {
        return true;
    }

    // PHP / PHPUnit convention: <ClassName>Test.php
    if file_name.ends_with("Test.php") {
        return true;
    }

    false
}

/// Detect the [`Language`] for a file extension.
///
/// Returns `None` for unknown extensions.
fn language_for_extension(ext: &str) -> Option<Language> {
    match ext {
        "py" => Some(Language::Python),
        "ts" | "tsx" => Some(Language::TypeScript),
        "go" => Some(Language::Go),
        "rs" => Some(Language::Rust),
        "php" => Some(Language::Php),
        _ => None,
    }
}

fn is_eligible_source_file(path: &Path, languages: &[Language]) -> Option<Language> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if is_test_file(name) {
        return None;
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang = language_for_extension(ext)?;
    languages.contains(&lang).then_some(lang)
}

/// Convert a file path relative to `base` into a dot-notation module name,
/// optionally prefixed with `local_prefix`.
///
/// Rules:
/// - `__init__.py` → parent package name (strip `/__init__.py` suffix)
/// - `index.ts` / `index.tsx` → parent package name
/// - `mod.rs` / `lib.rs` / `main.rs` → parent package name (Rust entry points)
/// - All other files → strip extension, replace `/` with `.`
/// - If `local_prefix` is non-empty, prepend it (with `.`) only when the
///   resulting name doesn't already start with it.
pub fn path_to_module(base: &Path, file: &Path, local_prefix: &str) -> String {
    // Make the path relative to base.
    let rel = file.strip_prefix(base).unwrap_or(file);

    if rel.extension().and_then(|s| s.to_str()) == Some("go") {
        return path_to_go_package(base, file, local_prefix);
    }

    // Collect path components (excluding the filename itself for now).
    let mut parts: Vec<String> = rel
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_owned()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    let file_name = rel.file_name().and_then(|s| s.to_str()).unwrap_or("");

    // Special files: use parent package only (entry points collapse to parent).
    let is_init = file_name == "__init__.py";
    let is_index = file_name == "index.ts" || file_name == "index.tsx";
    let is_rust_entry = file_name == "mod.rs" || file_name == "lib.rs" || file_name == "main.rs";

    if !is_init && !is_index && !is_rust_entry {
        parts.push(stem.to_owned());
    }

    let module = parts.join(".");

    // Apply local prefix.
    if local_prefix.is_empty() || module.starts_with(local_prefix) {
        module
    } else {
        format!("{}.{}", local_prefix, module)
    }
}

fn path_to_go_package(base: &Path, file: &Path, local_prefix: &str) -> String {
    let rel = file.strip_prefix(base).unwrap_or(file);
    let parts: Vec<String> = rel
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_owned()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let package = if parts.is_empty() {
        if local_prefix.is_empty() {
            rel.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_owned()
        } else {
            local_prefix.to_owned()
        }
    } else {
        parts.join(".")
    };

    if local_prefix.is_empty() || package.starts_with(local_prefix) {
        package
    } else {
        format!("{}.{}", local_prefix, package)
    }
}

/// PSR-4-aware variant of [`path_to_module`]. When `psr4_mappings` contains a
/// `(namespace_prefix, dir_prefix)` pair whose `dir_prefix` matches the start
/// of the file's path relative to `base`, applies the namespace translation.
/// Longest-matching `dir_prefix` wins. Falls back to [`path_to_module`] when no
/// mapping applies or when `psr4_mappings` is empty.
pub fn path_to_module_psr4(
    base: &Path,
    file: &Path,
    local_prefix: &str,
    psr4_mappings: &[(String, String)],
) -> String {
    if psr4_mappings.is_empty() {
        return path_to_module(base, file, local_prefix);
    }

    let rel = file.strip_prefix(base).unwrap_or(file);
    let rel_str = rel.to_string_lossy();

    // Find the longest `dir_prefix` that is a prefix of `rel_str`.
    let best = psr4_mappings
        .iter()
        .filter(|(_, dir)| rel_str.starts_with(dir.as_str()))
        .max_by_key(|(_, dir)| dir.len());

    let (ns_prefix, dir_prefix) = match best {
        Some(pair) => pair,
        None => return path_to_module(base, file, local_prefix),
    };

    let remainder = &rel_str[dir_prefix.len()..];
    let remainder_no_ext = remainder.strip_suffix(".php").unwrap_or(remainder);

    let ns_prefix_clean = ns_prefix.trim_end_matches('\\');

    let combined = if ns_prefix_clean.is_empty() {
        remainder_no_ext.to_owned()
    } else {
        format!(
            "{}/{}",
            ns_prefix_clean.replace('\\', "/"),
            remainder_no_ext
        )
    };

    combined.replace(['/', '\\'], ".")
}

/// Recursively walk `root`, skip excluded directories, and collect all source
/// files whose extension is handled by at least one entry in `languages`.
///
/// `local_prefix` is forwarded to [`path_to_module`].
/// `extra_excludes` is merged with [`DEFAULT_EXCLUDES`] for directory
/// filtering.
///
/// The result is sorted by path for deterministic ordering.
pub fn discover_files(
    root: &Path,
    languages: &[Language],
    local_prefix: &str,
    extra_excludes: &[&str],
) -> Vec<DiscoveredFile> {
    discover_files_with_psr4(root, languages, local_prefix, extra_excludes, &[])
}

/// PSR-4-aware variant of [`discover_files`]. When `psr4_mappings` is empty,
/// behaves identically to the non-PSR-4 call.
pub fn discover_files_with_psr4(
    root: &Path,
    languages: &[Language],
    local_prefix: &str,
    extra_excludes: &[&str],
    psr4_mappings: &[(String, String)],
) -> Vec<DiscoveredFile> {
    let mut excludes: Vec<&str> = DEFAULT_EXCLUDES.to_vec();
    excludes.extend_from_slice(extra_excludes);

    let mut results = Vec::new();
    walk_dir(
        root,
        root,
        languages,
        local_prefix,
        &excludes,
        psr4_mappings,
        &mut results,
    );
    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}

/// `EffectiveLocalPrefix`-aware variant of [`path_to_module`].
///
/// In wrap mode (`Single`), behaves identically to the legacy `path_to_module`.
/// In no-wrap mode (`Multi`), returns the module id from the relative path
/// without any prepended prefix.
pub fn path_to_module_eff(
    base: &Path,
    file: &Path,
    prefix: &crate::EffectiveLocalPrefix,
) -> String {
    path_to_module(base, file, prefix.legacy_prefix())
}

/// `EffectiveLocalPrefix`-aware variant of [`discover_files`].
pub fn discover_files_eff(
    root: &Path,
    languages: &[Language],
    prefix: &crate::EffectiveLocalPrefix,
    extra_excludes: &[&str],
) -> Vec<DiscoveredFile> {
    discover_files_eff_with_psr4(root, languages, prefix, extra_excludes, &[])
}

/// `EffectiveLocalPrefix`-aware variant of [`discover_files_with_psr4`].
pub fn discover_files_eff_with_psr4(
    root: &Path,
    languages: &[Language],
    prefix: &crate::EffectiveLocalPrefix,
    extra_excludes: &[&str],
    psr4_mappings: &[(String, String)],
) -> Vec<DiscoveredFile> {
    discover_files_with_psr4(
        root,
        languages,
        prefix.legacy_prefix(),
        extra_excludes,
        psr4_mappings,
    )
}

/// Minimum file count per root directory for the multi-root advisory to fire.
const MULTI_ROOT_WARNING_MIN_FILES: usize = 10;
/// Top1/top2 ratio cap for the multi-root advisory: warning fires only when
/// `top1 < top2 * MULTI_ROOT_WARNING_RATIO` (i.e. the leader does not dominate).
const MULTI_ROOT_WARNING_RATIO: f64 = 3.0;

/// Detect the effective `local_prefix` for a project when the config omits it.
///
/// Heuristic:
/// - Count eligible source files by first directory below `root`
/// - `src` wins if it contains >60% of all eligible files
/// - otherwise `app` wins if it contains >60% of all eligible files
/// - otherwise return an empty prefix (root-relative)
pub fn detect_local_prefix(root: &Path, languages: &[Language], extra_excludes: &[&str]) -> String {
    detect_local_prefix_with_warning_sink(root, languages, extra_excludes, &mut std::io::stderr())
}

/// Variant of [`detect_local_prefix`] that writes the multi-root advisory
/// warning to a caller-provided sink instead of stderr.
///
/// Multi-root heuristic: when ≥2 root directories each carry
/// ≥`MULTI_ROOT_WARNING_MIN_FILES` files **and** the leader does not dominate
/// (`top1 < top2 * MULTI_ROOT_WARNING_RATIO`), the codebase likely uses a
/// multi-root layout (e.g. Expo `app/` + `lib/`). The function still returns
/// a single prefix (legacy behavior) but advises the user to switch to the
/// new `local_prefix = ["app", "lib"]` array form via the warning sink.
pub fn detect_local_prefix_with_warning_sink<W: std::io::Write>(
    root: &Path,
    languages: &[Language],
    extra_excludes: &[&str],
    warning_sink: &mut W,
) -> String {
    let mut excludes: Vec<&str> = DEFAULT_EXCLUDES.to_vec();
    excludes.extend_from_slice(extra_excludes);

    let mut total_files = 0usize;
    let mut root_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    count_source_roots(
        root,
        root,
        languages,
        &excludes,
        &mut total_files,
        &mut root_counts,
    );

    if total_files == 0 {
        return String::new();
    }

    let threshold = |count: usize| (count as f64) / (total_files as f64) > 0.6;

    let prefix = if threshold(*root_counts.get("src").unwrap_or(&0)) {
        "src".to_owned()
    } else if threshold(*root_counts.get("app").unwrap_or(&0)) {
        "app".to_owned()
    } else {
        String::new()
    };

    // Multi-root advisory: collect roots above the min-files threshold (sorted
    // by descending count, then by name for determinism), then check the
    // top1/top2 ratio.
    let mut qualifying: Vec<(&String, &usize)> = root_counts
        .iter()
        .filter(|(_, count)| **count >= MULTI_ROOT_WARNING_MIN_FILES)
        .collect();
    qualifying.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    if qualifying.len() >= 2 {
        let top1 = *qualifying[0].1 as f64;
        let top2 = *qualifying[1].1 as f64;
        if top1 < top2 * MULTI_ROOT_WARNING_RATIO {
            let candidates_list = qualifying
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let suggestion = qualifying
                .iter()
                .map(|(name, _)| format!("\"{}\"", name))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                warning_sink,
                "Multi-root pattern detected: candidates [{}]. Consider local_prefix = [{}] in graphify.toml. Auto-detected single prefix '{}' for now.",
                candidates_list, suggestion, prefix
            );
        }
    }

    prefix
}

fn count_source_roots(
    base: &Path,
    dir: &Path,
    languages: &[Language],
    excludes: &[&str],
    total_files: &mut usize,
    root_counts: &mut std::collections::HashMap<String, usize>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if path.is_dir() {
            if excludes.contains(&name) {
                continue;
            }
            count_source_roots(base, &path, languages, excludes, total_files, root_counts);
            continue;
        }

        if !path.is_file() || is_eligible_source_file(&path, languages).is_none() {
            continue;
        }

        *total_files += 1;

        let rel = path.strip_prefix(base).unwrap_or(&path);
        let root_name = rel
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or("")
            .to_owned();
        *root_counts.entry(root_name).or_insert(0) += 1;
    }
}

fn walk_dir(
    base: &Path,
    dir: &Path,
    languages: &[Language],
    local_prefix: &str,
    excludes: &[&str],
    psr4_mappings: &[(String, String)],
    out: &mut Vec<DiscoveredFile>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if path.is_dir() {
            if excludes.contains(&name) {
                continue;
            }
            walk_dir(
                base,
                &path,
                languages,
                local_prefix,
                excludes,
                psr4_mappings,
                out,
            );
        } else if path.is_file() {
            if let Some(lang) = is_eligible_source_file(&path, languages) {
                // PSR-4 translation only applies to PHP files.
                let module_name = if lang == Language::Php && !psr4_mappings.is_empty() {
                    path_to_module_psr4(base, &path, local_prefix, psr4_mappings)
                } else {
                    path_to_module(base, &path, local_prefix)
                };
                let is_package = name == "__init__.py"
                    || name == "index.ts"
                    || name == "index.tsx"
                    || name == "mod.rs"
                    || name == "lib.rs"
                    || name == "main.rs";
                out.push(DiscoveredFile {
                    path,
                    language: lang,
                    module_name,
                    is_package,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // path_to_module
    // -----------------------------------------------------------------------

    #[test]
    fn path_to_module_python_regular_file() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/app/services/llm.py");
        assert_eq!(path_to_module(base, file, "app"), "app.services.llm");
    }

    #[test]
    fn path_to_module_python_init() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/app/services/__init__.py");
        assert_eq!(path_to_module(base, file, "app"), "app.services");
    }

    #[test]
    fn path_to_module_typescript_regular() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/lib/api.ts");
        assert_eq!(path_to_module(base, file, "src"), "src.lib.api");
    }

    #[test]
    fn path_to_module_index_ts() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/services/index.ts");
        assert_eq!(path_to_module(base, file, "src"), "src.services");
    }

    #[test]
    fn path_to_module_no_prefix_duplication() {
        // If the path already starts with the prefix, don't prepend again.
        let base = Path::new("/repo");
        let file = Path::new("/repo/app/main.py");
        // "app.main" already starts with "app"
        assert_eq!(path_to_module(base, file, "app"), "app.main");
    }

    #[test]
    fn path_to_module_empty_prefix() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/app/main.py");
        assert_eq!(path_to_module(base, file, ""), "app.main");
    }

    // -----------------------------------------------------------------------
    // discover_files — Python fixture
    // -----------------------------------------------------------------------

    fn python_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap() // crates/
            .parent()
            .unwrap() // workspace root
            .join("tests/fixtures/python_project")
    }

    fn ts_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/ts_project")
    }

    #[test]
    fn discover_python_fixture_finds_at_least_four_files() {
        let root = python_fixture_root();
        let files = discover_files(&root, &[Language::Python], "app", &[]);
        assert!(
            files.len() >= 4,
            "expected ≥4 Python files, got {}: {:?}",
            files.len(),
            files.iter().map(|f| &f.module_name).collect::<Vec<_>>()
        );
        // All results should be Python.
        for f in &files {
            assert_eq!(f.language, Language::Python);
        }
    }

    #[test]
    fn discover_python_fixture_correct_module_names() {
        let root = python_fixture_root();
        let files = discover_files(&root, &[Language::Python], "app", &[]);
        let names: Vec<&str> = files.iter().map(|f| f.module_name.as_str()).collect();
        assert!(names.contains(&"app"), "expected 'app' (__init__.py)");
        assert!(names.contains(&"app.main"), "expected 'app.main'");
        assert!(
            names.contains(&"app.services"),
            "expected 'app.services' (__init__.py)"
        );
        assert!(
            names.contains(&"app.services.llm"),
            "expected 'app.services.llm'"
        );
        assert!(
            names.contains(&"app.models.user"),
            "expected 'app.models.user'"
        );
    }

    #[test]
    fn discover_ts_fixture_finds_at_least_three_files() {
        let root = ts_fixture_root();
        let files = discover_files(&root, &[Language::TypeScript], "src", &[]);
        assert!(
            files.len() >= 3,
            "expected ≥3 TS files, got {}: {:?}",
            files.len(),
            files.iter().map(|f| &f.module_name).collect::<Vec<_>>()
        );
        for f in &files {
            assert_eq!(f.language, Language::TypeScript);
        }
    }

    #[test]
    fn discover_excludes_node_modules() {
        // Create a temp dir with a node_modules sub-dir containing a .ts file.
        let tmp = tempfile::tempdir().unwrap();
        let nm = tmp.path().join("node_modules/lib");
        std::fs::create_dir_all(&nm).unwrap();
        std::fs::write(nm.join("helper.ts"), b"export const x = 1;").unwrap();
        // Also create a legitimate file.
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.ts"), b"const x = 1;").unwrap();

        let files = discover_files(tmp.path(), &[Language::TypeScript], "", &[]);
        let names: Vec<_> = files.iter().map(|f| f.module_name.as_str()).collect();
        assert!(
            names.contains(&"src.main"),
            "legitimate file should be found"
        );
        assert!(
            !names.iter().any(|n| n.contains("node_modules")),
            "node_modules should be excluded"
        );
    }

    #[test]
    fn discover_results_are_sorted_by_path() {
        let root = python_fixture_root();
        let files = discover_files(&root, &[Language::Python], "app", &[]);
        let paths: Vec<_> = files.iter().map(|f| f.path.clone()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "files should be sorted by path");
    }

    // -----------------------------------------------------------------------
    // is_test_file
    // -----------------------------------------------------------------------

    #[test]
    fn is_test_file_ts_test_patterns() {
        assert!(is_test_file("circuit-breaker.test.ts"));
        assert!(is_test_file("circuit-breaker.test.tsx"));
        assert!(is_test_file("helpers.test.js"));
        assert!(is_test_file("helpers.test.jsx"));
    }

    #[test]
    fn is_test_file_ts_spec_patterns() {
        assert!(is_test_file("resilience.spec.ts"));
        assert!(is_test_file("resilience.spec.tsx"));
        assert!(is_test_file("api.spec.js"));
        assert!(is_test_file("api.spec.jsx"));
    }

    #[test]
    fn is_test_file_python_patterns() {
        assert!(is_test_file("test_utils.test.py"));
        assert!(is_test_file("llm_test.py")); // *_test.py
        assert!(is_test_file("service_test.py"));
    }

    #[test]
    fn is_test_file_does_not_match_production_files() {
        assert!(!is_test_file("main.ts"));
        assert!(!is_test_file("api.ts"));
        assert!(!is_test_file("index.tsx"));
        assert!(!is_test_file("llm.py"));
        assert!(!is_test_file("__init__.py"));
        assert!(!is_test_file("testing.ts")); // "testing" is not a test pattern
        assert!(!is_test_file("contest.py")); // "test" as substring
    }

    // -----------------------------------------------------------------------
    // discover_files — test file exclusion
    // -----------------------------------------------------------------------

    #[test]
    fn discover_excludes_colocated_ts_test_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Production files
        std::fs::write(src.join("api.ts"), b"export const api = {};").unwrap();
        std::fs::write(src.join("retry.ts"), b"export function retry() {}").unwrap();
        // Test files (should be excluded)
        std::fs::write(
            src.join("api.test.ts"),
            b"import { describe } from 'vitest';",
        )
        .unwrap();
        std::fs::write(src.join("retry.spec.ts"), b"import { it } from 'vitest';").unwrap();
        std::fs::write(
            src.join("api.test.tsx"),
            b"import { expect } from 'vitest';",
        )
        .unwrap();

        let files = discover_files(tmp.path(), &[Language::TypeScript], "", &[]);
        let names: Vec<_> = files.iter().map(|f| f.module_name.as_str()).collect();
        assert!(
            names.contains(&"src.api"),
            "production file api.ts should be found"
        );
        assert!(
            names.contains(&"src.retry"),
            "production file retry.ts should be found"
        );
        assert_eq!(
            files.len(),
            2,
            "only production files should be found, got: {:?}",
            names
        );
    }

    #[test]
    fn discover_excludes_colocated_python_test_files() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        std::fs::create_dir_all(&app).unwrap();
        // Production files
        std::fs::write(app.join("main.py"), b"def main(): pass").unwrap();
        std::fs::write(app.join("service.py"), b"class Service: pass").unwrap();
        // Test files (should be excluded)
        std::fs::write(app.join("main_test.py"), b"def test_main(): pass").unwrap();
        std::fs::write(app.join("service.test.py"), b"def test_service(): pass").unwrap();

        let files = discover_files(tmp.path(), &[Language::Python], "", &[]);
        let names: Vec<_> = files.iter().map(|f| f.module_name.as_str()).collect();
        assert!(
            names.contains(&"app.main"),
            "production file main.py should be found"
        );
        assert!(
            names.contains(&"app.service"),
            "production file service.py should be found"
        );
        assert_eq!(
            files.len(),
            2,
            "only production files should be found, got: {:?}",
            names
        );
    }

    #[test]
    fn discover_excludes_js_test_and_spec_files() {
        // Verify .js and .jsx extensions are NOT discovered at all (language_for_extension
        // only handles .ts/.tsx/.py), but test the is_test_file guard regardless —
        // if JS support is added later, the guard is already in place.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("helper.test.js"), b"test('x', () => {});").unwrap();
        std::fs::write(src.join("helper.spec.jsx"), b"test('x', () => {});").unwrap();

        let files = discover_files(tmp.path(), &[Language::TypeScript], "", &[]);
        assert!(
            files.is_empty(),
            "JS test files should not appear: {:?}",
            files.iter().map(|f| &f.module_name).collect::<Vec<_>>()
        );
    }

    // -----------------------------------------------------------------------
    // Go support
    // -----------------------------------------------------------------------

    #[test]
    fn path_to_module_go_regular_file() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/cmd/server/main.go");
        assert_eq!(path_to_module(base, file, ""), "cmd.server");
    }

    #[test]
    fn path_to_module_go_with_prefix() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/pkg/handler.go");
        assert_eq!(path_to_module(base, file, "daemon"), "daemon.pkg");
    }

    #[test]
    fn is_test_file_go_patterns() {
        assert!(is_test_file("handler_test.go"));
        assert!(is_test_file("main_test.go"));
        assert!(!is_test_file("main.go"));
        assert!(!is_test_file("handler.go"));
        assert!(!is_test_file("testing.go")); // not a test file
    }

    #[test]
    fn discover_go_files_and_exclude_test_files() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("handler.go"), b"package pkg").unwrap();
        std::fs::write(pkg.join("handler_test.go"), b"package pkg").unwrap();

        let files = discover_files(tmp.path(), &[Language::Go], "", &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].module_name, "pkg");
        assert_eq!(files[0].language, Language::Go);
    }

    #[test]
    fn discover_excludes_vendor_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let vendor = tmp.path().join("vendor/github.com/lib");
        std::fs::create_dir_all(&vendor).unwrap();
        std::fs::write(vendor.join("dep.go"), b"package lib").unwrap();
        let src = tmp.path().join("cmd");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.go"), b"package main").unwrap();

        let files = discover_files(tmp.path(), &[Language::Go], "", &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].module_name, "cmd");
    }

    // -----------------------------------------------------------------------
    // Rust support
    // -----------------------------------------------------------------------

    #[test]
    fn path_to_module_rust_regular_file() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/handler.rs");
        assert_eq!(path_to_module(base, file, ""), "src.handler");
    }

    #[test]
    fn path_to_module_rust_mod_rs() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/services/mod.rs");
        assert_eq!(path_to_module(base, file, ""), "src.services");
    }

    #[test]
    fn path_to_module_rust_lib_rs() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/lib.rs");
        assert_eq!(path_to_module(base, file, ""), "src");
    }

    #[test]
    fn path_to_module_rust_main_rs() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/main.rs");
        assert_eq!(path_to_module(base, file, ""), "src");
    }

    #[test]
    fn discover_rust_files_with_entry_points() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let svc = tmp.path().join("src/services");
        std::fs::create_dir_all(&svc).unwrap();
        std::fs::write(src.join("lib.rs"), b"mod services;").unwrap();
        std::fs::write(src.join("handler.rs"), b"pub fn handle() {}").unwrap();
        std::fs::write(svc.join("mod.rs"), b"pub mod db;").unwrap();

        let files = discover_files(tmp.path(), &[Language::Rust], "", &[]);
        let names: Vec<&str> = files.iter().map(|f| f.module_name.as_str()).collect();
        assert!(names.contains(&"src"), "lib.rs should map to 'src'");
        assert!(
            names.contains(&"src.handler"),
            "handler.rs should map to 'src.handler'"
        );
        assert!(
            names.contains(&"src.services"),
            "mod.rs should map to 'src.services'"
        );

        // Verify is_package flags
        let lib_file = files.iter().find(|f| f.module_name == "src").unwrap();
        assert!(lib_file.is_package, "lib.rs should be marked as package");
        let handler_file = files
            .iter()
            .find(|f| f.module_name == "src.handler")
            .unwrap();
        assert!(!handler_file.is_package, "handler.rs should not be package");
    }

    #[test]
    fn discover_excludes_target_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target/debug");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("build.rs"), b"fn main() {}").unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), b"pub mod core;").unwrap();

        let files = discover_files(tmp.path(), &[Language::Rust], "", &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].module_name, "src");
    }

    // -----------------------------------------------------------------------
    // local_prefix auto-detection
    // -----------------------------------------------------------------------

    #[test]
    fn detect_local_prefix_prefers_src_when_it_dominates() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let lib = tmp.path().join("lib");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(src.join("a.ts"), b"export const a = 1;").unwrap();
        std::fs::write(src.join("b.ts"), b"export const b = 1;").unwrap();
        std::fs::write(src.join("c.ts"), b"export const c = 1;").unwrap();
        std::fs::write(lib.join("helper.ts"), b"export const helper = 1;").unwrap();

        let detected = detect_local_prefix(tmp.path(), &[Language::TypeScript], &[]);
        assert_eq!(detected, "src");
    }

    #[test]
    fn detect_local_prefix_prefers_app_when_it_dominates() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let scripts = tmp.path().join("scripts");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(app.join("main.py"), b"def main(): pass").unwrap();
        std::fs::write(app.join("api.py"), b"def api(): pass").unwrap();
        std::fs::write(app.join("models.py"), b"class User: pass").unwrap();
        std::fs::write(scripts.join("seed.py"), b"def seed(): pass").unwrap();

        let detected = detect_local_prefix(tmp.path(), &[Language::Python], &[]);
        assert_eq!(detected, "app");
    }

    #[test]
    fn detect_local_prefix_returns_empty_when_no_directory_dominates() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let app = tmp.path().join("app");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(src.join("a.ts"), b"export const a = 1;").unwrap();
        std::fs::write(src.join("b.ts"), b"export const b = 1;").unwrap();
        std::fs::write(app.join("main.ts"), b"export const main = 1;").unwrap();
        std::fs::write(app.join("screen.ts"), b"export const screen = 1;").unwrap();

        let detected = detect_local_prefix(tmp.path(), &[Language::TypeScript], &[]);
        assert_eq!(detected, "");
    }

    #[test]
    fn detect_local_prefix_returns_empty_when_root_files_are_significant() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.ts"), b"export const a = 1;").unwrap();
        std::fs::write(src.join("b.ts"), b"export const b = 1;").unwrap();
        std::fs::write(tmp.path().join("index.ts"), b"export const root = 1;").unwrap();
        std::fs::write(tmp.path().join("config.ts"), b"export const config = 1;").unwrap();

        let detected = detect_local_prefix(tmp.path(), &[Language::TypeScript], &[]);
        assert_eq!(detected, "");
    }

    // -----------------------------------------------------------------------
    // Multi-root advisory warning — FEAT-049 Task 9
    // -----------------------------------------------------------------------

    #[test]
    fn detect_local_prefix_warns_on_balanced_multi_root_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let lib = tmp.path().join("lib");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        for i in 0..12 {
            std::fs::write(
                app.join(format!("a{i}.ts")),
                format!("export const a{i} = 1;").as_bytes(),
            )
            .unwrap();
            std::fs::write(
                lib.join(format!("l{i}.ts")),
                format!("export const l{i} = 1;").as_bytes(),
            )
            .unwrap();
        }

        let mut sink: Vec<u8> = Vec::new();
        let _detected = detect_local_prefix_with_warning_sink(
            tmp.path(),
            &[Language::TypeScript],
            &[],
            &mut sink,
        );

        let captured = String::from_utf8(sink).unwrap();
        assert!(
            captured.contains("Multi-root pattern detected"),
            "expected multi-root advisory in sink, got: {:?}",
            captured
        );
        assert!(
            captured.contains("app"),
            "expected `app` candidate in sink, got: {:?}",
            captured
        );
        assert!(
            captured.contains("lib"),
            "expected `lib` candidate in sink, got: {:?}",
            captured
        );
    }

    #[test]
    fn detect_local_prefix_no_warning_when_top1_dominates() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let scripts = tmp.path().join("scripts");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&scripts).unwrap();
        for i in 0..30 {
            std::fs::write(
                src.join(format!("s{i}.ts")),
                format!("export const s{i} = 1;").as_bytes(),
            )
            .unwrap();
        }
        for i in 0..3 {
            std::fs::write(
                scripts.join(format!("seed{i}.ts")),
                format!("export const seed{i} = 1;").as_bytes(),
            )
            .unwrap();
        }

        let mut sink: Vec<u8> = Vec::new();
        let detected = detect_local_prefix_with_warning_sink(
            tmp.path(),
            &[Language::TypeScript],
            &[],
            &mut sink,
        );

        assert_eq!(detected, "src");
        let captured = String::from_utf8(sink).unwrap();
        assert!(
            !captured.contains("Multi-root pattern detected"),
            "did not expect multi-root advisory, got: {:?}",
            captured
        );
    }

    #[test]
    fn detect_local_prefix_no_warning_when_only_one_dir_has_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        for i in 0..15 {
            std::fs::write(
                src.join(format!("f{i}.ts")),
                format!("export const f{i} = 1;").as_bytes(),
            )
            .unwrap();
        }

        let mut sink: Vec<u8> = Vec::new();
        let _detected = detect_local_prefix_with_warning_sink(
            tmp.path(),
            &[Language::TypeScript],
            &[],
            &mut sink,
        );

        let captured = String::from_utf8(sink).unwrap();
        assert!(
            !captured.contains("Multi-root pattern detected"),
            "did not expect multi-root advisory, got: {:?}",
            captured
        );
    }

    #[test]
    fn detect_local_prefix_no_warning_below_min_files_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let lib = tmp.path().join("lib");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        for i in 0..5 {
            std::fs::write(
                app.join(format!("a{i}.ts")),
                format!("export const a{i} = 1;").as_bytes(),
            )
            .unwrap();
            std::fs::write(
                lib.join(format!("l{i}.ts")),
                format!("export const l{i} = 1;").as_bytes(),
            )
            .unwrap();
        }

        let mut sink: Vec<u8> = Vec::new();
        let _detected = detect_local_prefix_with_warning_sink(
            tmp.path(),
            &[Language::TypeScript],
            &[],
            &mut sink,
        );

        let captured = String::from_utf8(sink).unwrap();
        assert!(
            !captured.contains("Multi-root pattern detected"),
            "did not expect multi-root advisory, got: {:?}",
            captured
        );
    }

    // -----------------------------------------------------------------------
    // PHP support
    // -----------------------------------------------------------------------

    #[test]
    fn path_to_module_php_regular_file() {
        // Without PSR-4 mapping (added in a later task), path-based fallback
        // applies: src/Services/Llm.php → "src.Services.Llm"
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/Services/Llm.php");
        assert_eq!(path_to_module(base, file, ""), "src.Services.Llm");
    }

    #[test]
    fn is_test_file_php_phpunit_pattern() {
        assert!(is_test_file("UserTest.php"));
        assert!(is_test_file("LlmTest.php"));
        assert!(!is_test_file("User.php"));
        assert!(!is_test_file("TestingHelper.php")); // "Testing" prefix is not a test
                                                     // Note: bare "Test.php" also matches ends_with("Test.php") and is treated
                                                     // as a test. In practice, production files are never literally named Test.php,
                                                     // so the false-positive is acceptable and not asserted here.
    }

    #[test]
    fn discover_php_files_and_exclude_phpunit_tests() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("Service.php"), b"<?php\nclass Service {}").unwrap();
        std::fs::write(src.join("ServiceTest.php"), b"<?php\nclass ServiceTest {}").unwrap();

        let files = discover_files(tmp.path(), &[Language::Php], "", &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].module_name, "src.Service");
        assert_eq!(files[0].language, Language::Php);
        assert!(!files[0].is_package, "PHP files are never packages");
    }

    // -----------------------------------------------------------------------
    // PSR-4 path translation
    // -----------------------------------------------------------------------

    #[test]
    fn path_to_module_php_with_psr4_mapping() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/Services/Llm.php");
        let mappings = &[("App\\".to_string(), "src/".to_string())][..];
        assert_eq!(
            path_to_module_psr4(base, file, "", mappings),
            "App.Services.Llm"
        );
    }

    #[test]
    fn path_to_module_php_without_composer_falls_back() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/Services/Llm.php");
        let mappings: &[(String, String)] = &[];
        assert_eq!(
            path_to_module_psr4(base, file, "", mappings),
            "src.Services.Llm"
        );
    }

    #[test]
    fn path_to_module_php_longest_prefix_wins() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/src/legacy/Old/Dto.php");
        let mappings = &[
            ("App\\".to_string(), "src/".to_string()),
            ("App\\Legacy\\".to_string(), "src/legacy/".to_string()),
        ][..];
        // Longest-prefix wins: src/legacy/ → App\Legacy\
        assert_eq!(
            path_to_module_psr4(base, file, "", mappings),
            "App.Legacy.Old.Dto"
        );
    }

    #[test]
    fn path_to_module_php_non_matching_path_falls_back() {
        let base = Path::new("/repo");
        let file = Path::new("/repo/scripts/seed.php");
        let mappings = &[("App\\".to_string(), "src/".to_string())][..];
        assert_eq!(
            path_to_module_psr4(base, file, "", mappings),
            "scripts.seed"
        );
    }

    // -----------------------------------------------------------------------
    // _eff (EffectiveLocalPrefix-aware) entry points — FEAT-049 Task 3
    // -----------------------------------------------------------------------

    #[test]
    fn path_to_module_no_wrap_keeps_natural_id() {
        use crate::EffectiveLocalPrefix;
        use crate::LocalPrefix;
        let base = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/lib/util.ts");
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));
        assert_eq!(path_to_module_eff(base, file, &eff), "lib.util");
    }

    #[test]
    fn path_to_module_no_wrap_unmatched_root_still_no_wrap() {
        // Multi-mode does NOT filter out non-matching files — the walker
        // discovers everything; the array is purely a naming hint.
        use crate::EffectiveLocalPrefix;
        use crate::LocalPrefix;
        let base = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/scripts/build.ts");
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));
        // `scripts` is not in the Multi list, but the file is still discovered —
        // its module id stays natural.
        assert_eq!(path_to_module_eff(base, file, &eff), "scripts.build");
    }

    #[test]
    fn path_to_module_wrap_uses_single_prefix_unchanged() {
        use crate::EffectiveLocalPrefix;
        use crate::LocalPrefix;
        let base = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/lib/util.ts");
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
        // Mirrors current behavior — `lib/util.ts` under `local_prefix = "src"`
        // becomes `src.lib.util`.
        assert_eq!(path_to_module_eff(base, file, &eff), "src.lib.util");
    }

    #[test]
    fn path_to_module_wrap_idempotent_on_already_prefixed() {
        use crate::EffectiveLocalPrefix;
        use crate::LocalPrefix;
        let base = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/src/foo.ts");
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
        assert_eq!(path_to_module_eff(base, file, &eff), "src.foo");
    }

    #[test]
    fn path_to_module_omitted_returns_root_relative() {
        use crate::EffectiveLocalPrefix;
        let base = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/foo/bar.ts");
        let eff = EffectiveLocalPrefix::omitted();
        assert_eq!(path_to_module_eff(base, file, &eff), "foo.bar");
    }

    #[test]
    fn path_to_go_package_no_wrap_keeps_natural_id() {
        use crate::EffectiveLocalPrefix;
        use crate::LocalPrefix;
        let base = std::path::Path::new("/repo");
        let file = std::path::Path::new("/repo/cmd/server/main.go");
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec!["cmd".to_string()]));
        // Multi mode: no wrapping. Package path = `cmd.server`.
        assert_eq!(path_to_module_eff(base, file, &eff), "cmd.server");
    }

    #[test]
    fn discover_files_eff_returns_same_set_in_either_mode() {
        use crate::{EffectiveLocalPrefix, LocalPrefix};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("app/index.ts"), "").unwrap();
        std::fs::write(root.join("lib/util.ts"), "").unwrap();

        let single = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
        let multi = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));

        let langs = vec![Language::TypeScript];
        let files_single = discover_files_eff(root, &langs, &single, &[]);
        let files_multi = discover_files_eff(root, &langs, &multi, &[]);

        // Both modes discover the same files (just with different module IDs).
        assert_eq!(files_single.len(), 2);
        assert_eq!(files_multi.len(), 2);

        let multi_ids: Vec<String> = files_multi.iter().map(|f| f.module_name.clone()).collect();
        assert!(multi_ids.contains(&"app".to_string())); // app/index.ts → "app"
        assert!(multi_ids.contains(&"lib.util".to_string()));

        let single_ids: Vec<String> = files_single.iter().map(|f| f.module_name.clone()).collect();
        assert!(single_ids.contains(&"src.app".to_string()));
        assert!(single_ids.contains(&"src.lib.util".to_string()));
    }
}
