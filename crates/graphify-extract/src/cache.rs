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
            edges: vec![(
                "app.main".to_string(),
                "os".to_string(),
                Edge::imports(1),
            )],
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
}
