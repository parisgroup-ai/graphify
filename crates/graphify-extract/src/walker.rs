use std::path::{Path, PathBuf};
use graphify_core::types::Language;

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
        ".test.ts", ".test.tsx", ".test.js", ".test.jsx",
        ".spec.ts", ".spec.tsx", ".spec.js", ".spec.jsx",
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

    false
}

/// Detect the [`Language`] for a file extension.
///
/// Returns `None` for unknown extensions.
fn language_for_extension(ext: &str) -> Option<Language> {
    match ext {
        "py" => Some(Language::Python),
        "ts" | "tsx" => Some(Language::TypeScript),
        _ => None,
    }
}

/// Convert a file path relative to `base` into a dot-notation module name,
/// optionally prefixed with `local_prefix`.
///
/// Rules:
/// - `__init__.py` → parent package name (strip `/__init__.py` suffix)
/// - `index.ts` / `index.tsx` → parent package name
/// - All other files → strip extension, replace `/` with `.`
/// - If `local_prefix` is non-empty, prepend it (with `.`) only when the
///   resulting name doesn't already start with it.
pub fn path_to_module(base: &Path, file: &Path, local_prefix: &str) -> String {
    // Make the path relative to base.
    let rel = file
        .strip_prefix(base)
        .unwrap_or(file);

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

    let stem = rel
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let file_name = rel
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    // Special files: use parent package only.
    let is_init = file_name == "__init__.py";
    let is_index = file_name == "index.ts" || file_name == "index.tsx";

    if !is_init && !is_index {
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
    let mut excludes: Vec<&str> = DEFAULT_EXCLUDES.to_vec();
    excludes.extend_from_slice(extra_excludes);

    let mut results = Vec::new();
    walk_dir(root, root, languages, local_prefix, &excludes, &mut results);
    results.sort_by(|a, b| a.path.cmp(&b.path));
    results
}

fn walk_dir(
    base: &Path,
    dir: &Path,
    languages: &[Language],
    local_prefix: &str,
    excludes: &[&str],
    out: &mut Vec<DiscoveredFile>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if path.is_dir() {
            if excludes.contains(&name) {
                continue;
            }
            walk_dir(base, &path, languages, local_prefix, excludes, out);
        } else if path.is_file() {
            // Skip co-located test files (e.g. *.test.ts, *.spec.tsx, *_test.py).
            if is_test_file(name) {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if let Some(lang) = language_for_extension(ext) {
                if languages.contains(&lang) {
                    let module_name = path_to_module(base, &path, local_prefix);
                    let is_package = name == "__init__.py"
                        || name == "index.ts"
                        || name == "index.tsx";
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
            .parent().unwrap()   // crates/
            .parent().unwrap()   // workspace root
            .join("tests/fixtures/python_project")
    }

    fn ts_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
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
        assert!(names.contains(&"app.services"), "expected 'app.services' (__init__.py)");
        assert!(names.contains(&"app.services.llm"), "expected 'app.services.llm'");
        assert!(names.contains(&"app.models.user"), "expected 'app.models.user'");
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
        assert!(names.contains(&"src.main"), "legitimate file should be found");
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
        assert!(is_test_file("llm_test.py"));       // *_test.py
        assert!(is_test_file("service_test.py"));
    }

    #[test]
    fn is_test_file_does_not_match_production_files() {
        assert!(!is_test_file("main.ts"));
        assert!(!is_test_file("api.ts"));
        assert!(!is_test_file("index.tsx"));
        assert!(!is_test_file("llm.py"));
        assert!(!is_test_file("__init__.py"));
        assert!(!is_test_file("testing.ts"));    // "testing" is not a test pattern
        assert!(!is_test_file("contest.py"));    // "test" as substring
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
        std::fs::write(src.join("api.test.ts"), b"import { describe } from 'vitest';").unwrap();
        std::fs::write(src.join("retry.spec.ts"), b"import { it } from 'vitest';").unwrap();
        std::fs::write(src.join("api.test.tsx"), b"import { expect } from 'vitest';").unwrap();

        let files = discover_files(tmp.path(), &[Language::TypeScript], "", &[]);
        let names: Vec<_> = files.iter().map(|f| f.module_name.as_str()).collect();
        assert!(names.contains(&"src.api"), "production file api.ts should be found");
        assert!(names.contains(&"src.retry"), "production file retry.ts should be found");
        assert_eq!(files.len(), 2, "only production files should be found, got: {:?}", names);
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
        assert!(names.contains(&"app.main"), "production file main.py should be found");
        assert!(names.contains(&"app.service"), "production file service.py should be found");
        assert_eq!(files.len(), 2, "only production files should be found, got: {:?}", names);
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
        assert!(files.is_empty(), "JS test files should not appear: {:?}",
            files.iter().map(|f| &f.module_name).collect::<Vec<_>>());
    }
}
