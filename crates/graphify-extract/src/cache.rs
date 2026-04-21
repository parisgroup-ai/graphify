use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::lang::ExtractionResult;

const CACHE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    sha256: String,
    result: ExtractionResult,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    local_prefix: String,
    entries: HashMap<String, CacheEntry>,
}

/// Statistics from a cache-aware extraction run.
#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub evicted: usize,
    pub forced: bool,
}

/// On-disk extraction cache for incremental builds.
///
/// Stores per-file `ExtractionResult` keyed by relative file path and SHA256
/// hash. When a file's content hash matches the cached entry, tree-sitter
/// parsing is skipped entirely.
pub struct ExtractionCache {
    local_prefix: String,
    entries: HashMap<String, CacheEntry>,
}

impl ExtractionCache {
    /// Create a new empty cache for the given `local_prefix`.
    pub fn new(local_prefix: &str) -> Self {
        Self {
            local_prefix: local_prefix.to_string(),
            entries: HashMap::new(),
        }
    }

    /// Look up a cached extraction by relative path and expected SHA256.
    ///
    /// Returns `Some` only if the path exists in the cache AND the stored
    /// hash matches `sha256`. Any mismatch returns `None` (cache miss).
    pub fn lookup(&self, rel_path: &str, sha256: &str) -> Option<&ExtractionResult> {
        self.entries.get(rel_path).and_then(|entry| {
            if entry.sha256 == sha256 {
                Some(&entry.result)
            } else {
                None
            }
        })
    }

    /// Insert or overwrite a cache entry.
    pub fn insert(&mut self, rel_path: String, sha256: String, result: ExtractionResult) {
        self.entries.insert(rel_path, CacheEntry { sha256, result });
    }

    /// Load a cache file from disk.
    ///
    /// Returns `None` if the file doesn't exist, can't be parsed, has a
    /// version mismatch, or has a `local_prefix` mismatch.
    pub fn load(path: &Path, expected_local_prefix: &str) -> Option<Self> {
        let data = std::fs::read_to_string(path).ok()?;
        let file: CacheFile = serde_json::from_str(&data).ok()?;

        if file.version != CACHE_VERSION {
            return None;
        }
        if file.local_prefix != expected_local_prefix {
            return None;
        }

        Some(Self {
            local_prefix: file.local_prefix,
            entries: file.entries,
        })
    }

    /// Remove entries whose keys are not in `active_paths`.
    pub fn retain_paths(&mut self, active_paths: &HashSet<String>) {
        self.entries.retain(|k, _| active_paths.contains(k));
    }

    /// Serialize the cache to disk as pretty-printed JSON.
    pub fn save(&self, path: &Path) {
        let file = CacheFile {
            version: CACHE_VERSION,
            local_prefix: self.local_prefix.clone(),
            entries: self
                .entries
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        CacheEntry {
                            sha256: v.sha256.clone(),
                            result: v.result.clone(),
                        },
                    )
                })
                .collect(),
        };
        let json = serde_json::to_string_pretty(&file).expect("serialize cache");
        std::fs::write(path, json).expect("write cache file");
    }

    /// Returns an iterator over all cached relative paths.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// Returns the number of cached entries.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// Compute the hex-encoded SHA256 digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_empty_input() {
        // SHA256 of empty string is a well-known constant.
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_hello_world() {
        let hash = sha256_hex(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn sha256_hex_deterministic() {
        let a = sha256_hex(b"same input");
        let b = sha256_hex(b"same input");
        assert_eq!(a, b);
    }

    #[test]
    fn sha256_hex_different_inputs_differ() {
        let a = sha256_hex(b"input a");
        let b = sha256_hex(b"input b");
        assert_ne!(a, b);
    }

    fn make_result() -> ExtractionResult {
        use graphify_core::types::{Edge, Language, Node};
        ExtractionResult {
            nodes: vec![Node::module(
                "app.main",
                "app/main.py",
                Language::Python,
                1,
                true,
            )],
            edges: vec![("app.main".to_string(), "os".to_string(), Edge::imports(1))],
            reexports: Vec::new(),
            named_imports: Vec::new(),
            use_aliases: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn new_cache_is_empty() {
        let cache = ExtractionCache::new("app");
        assert!(cache.lookup("any/path.py", "anyhash").is_none());
    }

    #[test]
    fn insert_and_lookup_hit() {
        let mut cache = ExtractionCache::new("app");
        let result = make_result();
        cache.insert("app/main.py".to_string(), "abc123".to_string(), result);

        let found = cache.lookup("app/main.py", "abc123");
        assert!(found.is_some());
        assert_eq!(found.unwrap().nodes.len(), 1);
        assert_eq!(found.unwrap().nodes[0].id, "app.main");
    }

    #[test]
    fn lookup_miss_wrong_hash() {
        let mut cache = ExtractionCache::new("app");
        cache.insert(
            "app/main.py".to_string(),
            "abc123".to_string(),
            make_result(),
        );
        assert!(cache.lookup("app/main.py", "different_hash").is_none());
    }

    #[test]
    fn lookup_miss_unknown_path() {
        let mut cache = ExtractionCache::new("app");
        cache.insert(
            "app/main.py".to_string(),
            "abc123".to_string(),
            make_result(),
        );
        assert!(cache.lookup("unknown/file.py", "abc123").is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join(".graphify-cache.json");

        let mut cache = ExtractionCache::new("app");
        cache.insert(
            "app/main.py".to_string(),
            "hash1".to_string(),
            make_result(),
        );

        cache.save(&cache_path);
        assert!(cache_path.exists());

        let loaded = ExtractionCache::load(&cache_path, "app");
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        let found = loaded.lookup("app/main.py", "hash1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().nodes[0].id, "app.main");
    }

    #[test]
    fn load_returns_none_for_missing_file() {
        let loaded = ExtractionCache::load(Path::new("/nonexistent/.graphify-cache.json"), "app");
        assert!(loaded.is_none());
    }

    #[test]
    fn load_returns_none_for_version_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join(".graphify-cache.json");
        // Write a cache file with version 999.
        let bad = serde_json::json!({
            "version": 999,
            "local_prefix": "app",
            "entries": {}
        });
        std::fs::write(&cache_path, serde_json::to_string(&bad).unwrap()).unwrap();

        let loaded = ExtractionCache::load(&cache_path, "app");
        assert!(loaded.is_none());
    }

    #[test]
    fn load_returns_none_for_prefix_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join(".graphify-cache.json");

        let mut cache = ExtractionCache::new("app");
        cache.insert(
            "app/main.py".to_string(),
            "hash1".to_string(),
            make_result(),
        );
        cache.save(&cache_path);

        // Load with a different prefix.
        let loaded = ExtractionCache::load(&cache_path, "src");
        assert!(loaded.is_none());
    }

    #[test]
    fn load_returns_none_for_corrupt_json() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join(".graphify-cache.json");
        std::fs::write(&cache_path, "not valid json {{{{").unwrap();

        let loaded = ExtractionCache::load(&cache_path, "app");
        assert!(loaded.is_none());
    }

    #[test]
    fn retain_paths_evicts_removed_files() {
        let mut cache = ExtractionCache::new("app");
        cache.insert("a.py".to_string(), "h1".to_string(), make_result());
        cache.insert("b.py".to_string(), "h2".to_string(), make_result());
        cache.insert("c.py".to_string(), "h3".to_string(), make_result());

        let active: HashSet<String> = ["a.py", "c.py"].iter().map(|s| s.to_string()).collect();
        cache.retain_paths(&active);

        assert!(cache.lookup("a.py", "h1").is_some());
        assert!(cache.lookup("b.py", "h2").is_none()); // evicted
        assert!(cache.lookup("c.py", "h3").is_some());
    }

    #[test]
    fn integration_cache_with_python_extraction() {
        use crate::{LanguageExtractor, PythonExtractor};

        // Set up temp dirs.
        let src_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        let cache_path = cache_dir.path().join(".graphify-cache.json");

        // Write a Python source file.
        let py_file = src_dir.path().join("main.py");
        std::fs::write(&py_file, b"import os\ndef hello():\n    pass\n").unwrap();

        // Extract.
        let source = std::fs::read(&py_file).unwrap();
        let hash = sha256_hex(&source);
        let extractor = PythonExtractor::new();
        let result = extractor.extract_file(&py_file, &source, "main");

        // Save to cache.
        let mut cache = ExtractionCache::new("");
        cache.insert("main.py".to_string(), hash.clone(), result);
        cache.save(&cache_path);

        // Reload and verify cache hit.
        let loaded = ExtractionCache::load(&cache_path, "").unwrap();
        let cached = loaded.lookup("main.py", &hash);
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert!(!cached.nodes.is_empty());
        assert!(!cached.edges.is_empty());
    }

    #[test]
    fn integration_cache_miss_after_file_modification() {
        let _src_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        let cache_path = cache_dir.path().join(".graphify-cache.json");

        // Original content.
        let content_v1 = b"import os\n";
        let hash_v1 = sha256_hex(content_v1);

        let mut cache = ExtractionCache::new("");
        cache.insert(
            "mod.py".to_string(),
            hash_v1.clone(),
            ExtractionResult::new(),
        );
        cache.save(&cache_path);

        // Modified content → different hash → cache miss.
        let content_v2 = b"import os\nimport sys\n";
        let hash_v2 = sha256_hex(content_v2);
        assert_ne!(hash_v1, hash_v2);

        let loaded = ExtractionCache::load(&cache_path, "").unwrap();
        assert!(loaded.lookup("mod.py", &hash_v2).is_none());
    }
}
