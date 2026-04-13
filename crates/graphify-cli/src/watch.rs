use std::path::{Path, PathBuf};

/// Filters file-system events to only relevant source file changes.
///
/// Checks:
/// 1. File extension matches configured languages (.py, .ts, .tsx)
/// 2. Path is not inside an excluded directory
/// 3. Path is not inside the output directory
#[allow(dead_code)]
pub struct WatchFilter {
    extensions: Vec<String>,
    exclude_dirs: Vec<String>,
    output_dir: PathBuf,
}

#[allow(dead_code)]
impl WatchFilter {
    pub fn new(
        languages: &[String],
        exclude_dirs: &[String],
        output_dir: &Path,
    ) -> Self {
        let extensions: Vec<String> = languages
            .iter()
            .flat_map(|lang| match lang.to_lowercase().as_str() {
                "python" => vec!["py".to_string()],
                "typescript" => vec!["ts".to_string(), "tsx".to_string()],
                _ => vec![],
            })
            .collect();

        Self {
            extensions,
            exclude_dirs: exclude_dirs.to_vec(),
            output_dir: output_dir.to_path_buf(),
        }
    }

    /// Returns `true` if the path represents a relevant source file change.
    pub fn should_rebuild(&self, path: &Path) -> bool {
        // Check extension
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => return false,
        };
        if !self.extensions.iter().any(|allowed| allowed == ext) {
            return false;
        }

        // Check excluded directories
        let path_str = path.to_string_lossy();
        for exclude in &self.exclude_dirs {
            if path_str.contains(&format!("/{exclude}/")) || path_str.contains(&format!("\\{exclude}\\")) {
                return false;
            }
        }

        // Check output directory
        if path.starts_with(&self.output_dir) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter() -> WatchFilter {
        WatchFilter::new(
            &["python".to_string(), "typescript".to_string()],
            &["node_modules".to_string(), "__pycache__".to_string(), ".git".to_string()],
            Path::new("/project/report"),
        )
    }

    #[test]
    fn accepts_python_files() {
        let filter = make_filter();
        assert!(filter.should_rebuild(Path::new("/project/app/main.py")));
    }

    #[test]
    fn accepts_typescript_files() {
        let filter = make_filter();
        assert!(filter.should_rebuild(Path::new("/project/src/index.ts")));
        assert!(filter.should_rebuild(Path::new("/project/src/App.tsx")));
    }

    #[test]
    fn rejects_non_source_files() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/README.md")));
        assert!(!filter.should_rebuild(Path::new("/project/Cargo.toml")));
        assert!(!filter.should_rebuild(Path::new("/project/image.png")));
    }

    #[test]
    fn rejects_files_without_extension() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/Makefile")));
    }

    #[test]
    fn rejects_excluded_directories() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/node_modules/dep/index.ts")));
        assert!(!filter.should_rebuild(Path::new("/project/app/__pycache__/module.py")));
        assert!(!filter.should_rebuild(Path::new("/project/.git/hooks/pre-commit.py")));
    }

    #[test]
    fn rejects_output_directory() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/report/proj/graph.json")));
        assert!(!filter.should_rebuild(Path::new("/project/report/anything.py")));
    }

    #[test]
    fn empty_languages_rejects_all() {
        let filter = WatchFilter::new(&[], &[], Path::new("/out"));
        assert!(!filter.should_rebuild(Path::new("/project/main.py")));
    }
}
