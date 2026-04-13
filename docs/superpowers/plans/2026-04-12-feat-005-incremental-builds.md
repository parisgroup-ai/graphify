# FEAT-005: Incremental Builds with SHA256 Cache — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SHA256-based file caching so the extraction pipeline skips tree-sitter parsing for unchanged files, with cache on by default and `--force` to bypass.

**Architecture:** New `cache.rs` module in `graphify-extract` provides `ExtractionCache` (load/save/lookup per-file results keyed by SHA256). The CLI's `run_extract` gains optional cache integration — pipeline commands (Extract, Analyze, Report, Run) use caching; query commands (Query, Explain, Path, Shell) skip it. Cache lives in the output directory per project.

**Tech Stack:** `sha2` crate for hashing, serde JSON for cache persistence, existing rayon parallelism for file reading + hashing.

---

### File Map

| File | Action | Responsibility |
|---|---|---|
| `crates/graphify-extract/Cargo.toml` | Modify | Add `sha2` and `serde`+`serde_json` dependencies |
| `crates/graphify-extract/src/lang.rs` | Modify | Add `Clone, Serialize, Deserialize` derives to `ExtractionResult` |
| `crates/graphify-extract/src/cache.rs` | Create | `ExtractionCache` struct, `sha256_hex`, load/save/lookup/insert/retain |
| `crates/graphify-extract/src/lib.rs` | Modify | Add `pub mod cache;` |
| `crates/graphify-cli/src/main.rs` | Modify | Integrate cache into `run_extract`, add `--force` flag to 4 commands |
| `CLAUDE.md` | Modify | Document cache conventions |
| `docs/TaskNotes/Tasks/sprint.md` | Modify | Mark FEAT-005 done |

---

### Task 1: Foundation — Dependencies and Serde Derives

**Files:**
- Modify: `crates/graphify-extract/Cargo.toml`
- Modify: `crates/graphify-extract/src/lang.rs`
- Modify: `crates/graphify-extract/src/lib.rs`
- Create: `crates/graphify-extract/src/cache.rs`

- [ ] **Step 1: Add `sha2` and `serde` dependencies to graphify-extract**

In `crates/graphify-extract/Cargo.toml`, add under `[dependencies]`:

```toml
sha2 = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Add serde derives to `ExtractionResult`**

In `crates/graphify-extract/src/lang.rs`, change the struct definition:

```rust
use serde::{Deserialize, Serialize};

/// The result of extracting a single source file.
#[derive(Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    /// Tuples of (source_id, target_id, edge).
    pub edges: Vec<(String, String, Edge)>,
}
```

Note: `Node` and `Edge` already derive `Serialize, Deserialize` in `graphify-core/src/types.rs`.

- [ ] **Step 3: Create empty cache module**

Create `crates/graphify-extract/src/cache.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::lang::ExtractionResult;

const CACHE_VERSION: u32 = 1;
```

- [ ] **Step 4: Register the cache module**

In `crates/graphify-extract/src/lib.rs`, add:

```rust
pub mod cache;
```

(Add it after `pub mod lang;`)

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p graphify-extract`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-extract/Cargo.toml crates/graphify-extract/src/lang.rs crates/graphify-extract/src/cache.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(extract): foundation for FEAT-005 — sha2 dep, serde derives, cache module skeleton"
```

---

### Task 2: SHA256 Helper (TDD)

**Files:**
- Modify: `crates/graphify-extract/src/cache.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/graphify-extract/src/cache.rs`:

```rust
/// Compute the hex-encoded SHA256 digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    todo!()
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
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p graphify-extract sha256_hex -- --no-capture`
Expected: FAIL with "not yet implemented"

- [ ] **Step 3: Implement sha256_hex**

Replace the `todo!()` body of `sha256_hex`:

```rust
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-extract sha256_hex`
Expected: 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/cache.rs
git commit -m "feat(extract): implement sha256_hex helper with tests (FEAT-005)"
```

---

### Task 3: ExtractionCache Core — new, insert, lookup (TDD)

**Files:**
- Modify: `crates/graphify-extract/src/cache.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `cache.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-extract cache::tests -- --no-capture`
Expected: FAIL — `ExtractionCache` not defined yet

- [ ] **Step 3: Implement ExtractionCache struct with new, insert, lookup**

Add to `crates/graphify-extract/src/cache.rs` (above the `sha256_hex` function):

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-extract cache::tests`
Expected: all cache tests pass (4 new + 4 sha256 = 8 total)

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/cache.rs
git commit -m "feat(extract): ExtractionCache core — new, insert, lookup with tests (FEAT-005)"
```

---

### Task 4: ExtractionCache Persistence — save, load, retain_paths (TDD)

**Files:**
- Modify: `crates/graphify-extract/src/cache.rs`

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `cache.rs`:

```rust
    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join(".graphify-cache.json");

        let mut cache = ExtractionCache::new("app");
        cache.insert("app/main.py".to_string(), "hash1".to_string(), make_result());

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
        cache.insert("app/main.py".to_string(), "hash1".to_string(), make_result());
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-extract cache::tests -- --no-capture`
Expected: FAIL — `save`, `load`, `retain_paths` not defined

- [ ] **Step 3: Implement save, load, retain_paths**

Add these methods to the `impl ExtractionCache` block in `cache.rs`:

```rust
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
            entries: self.entries.iter().map(|(k, v)| {
                (k.clone(), CacheEntry {
                    sha256: v.sha256.clone(),
                    result: v.result.clone(),
                })
            }).collect(),
        };
        let json = serde_json::to_string_pretty(&file).expect("serialize cache");
        std::fs::write(path, json).expect("write cache file");
    }
```

Also add `use std::fs;` if not already present (it's used via `std::fs::read_to_string` and `std::fs::write` with full paths, so no extra import needed).

Also add to `[dev-dependencies]` in `crates/graphify-extract/Cargo.toml` if not already there:

```toml
tempfile = "3"
```

(It's already there from the walker tests.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-extract cache::tests`
Expected: all 14 cache tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/cache.rs
git commit -m "feat(extract): ExtractionCache persistence — save, load, retain_paths with tests (FEAT-005)"
```

---

### Task 5: Add `--force` flag to CLI and a `CacheStats` return type

**Files:**
- Modify: `crates/graphify-extract/src/cache.rs`
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Add `CacheStats` struct to cache.rs**

Add to `crates/graphify-extract/src/cache.rs` (above the `ExtractionCache` struct):

```rust
/// Statistics from a cache-aware extraction run.
#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub evicted: usize,
    pub forced: bool,
}
```

- [ ] **Step 2: Add `--force` flag to all four pipeline commands**

In `crates/graphify-cli/src/main.rs`, add to `Extract`, `Analyze`, `Report`, and `Run` variants:

```rust
    Extract {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
    },

    Analyze {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        weights: Option<String>,
        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
    },

    Report {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        weights: Option<String>,
        #[arg(long)]
        format: Option<String>,
        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
    },

    Run {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
        /// Force full rebuild, ignoring extraction cache
        #[arg(long)]
        force: bool,
    },
```

- [ ] **Step 3: Update match arms to destructure `force`**

Update each match arm in `main()` to destructure the new `force` field. For now, just capture it (integration comes in Task 6):

```rust
Commands::Extract { config, output, force } => { ... }
Commands::Analyze { config, output, weights, force } => { ... }
Commands::Report { config, output, weights, format, force } => { ... }
Commands::Run { config, output, force } => { ... }
```

The `force` variable is unused until Task 6, so prefix with `_force` temporarily to avoid warnings:

```rust
Commands::Extract { config, output, force: _force } => { ... }
```

(Repeat for all four commands.)

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p graphify-cli`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/cache.rs crates/graphify-cli/src/main.rs
git commit -m "feat(cli): add --force flag to pipeline commands + CacheStats struct (FEAT-005)"
```

---

### Task 6: Pipeline Integration — Cache-Aware `run_extract`

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

This is the core integration. `run_extract` gains two new parameters: `cache_dir: Option<&Path>` (where to read/write cache) and `force: bool` (skip cache load). Returns a third value: `CacheStats`.

- [ ] **Step 1: Add import for cache module**

Add to the `use graphify_extract::{...}` block at the top of `main.rs`:

```rust
use graphify_extract::cache::{sha256_hex, CacheStats, ExtractionCache};
```

- [ ] **Step 2: Update `run_extract` signature**

Change the function signature from:

```rust
fn run_extract(project: &ProjectConfig, settings: &Settings) -> (CodeGraph, Vec<String>) {
```

To:

```rust
fn run_extract(
    project: &ProjectConfig,
    settings: &Settings,
    cache_dir: Option<&Path>,
    force: bool,
) -> (CodeGraph, Vec<String>, CacheStats) {
```

- [ ] **Step 3: Add cache loading after file discovery**

After the `discover_files` call and `files.len() <= 1` warning (around line 723), add cache loading:

```rust
    let local_prefix = project.local_prefix.as_deref().unwrap_or("");

    // Load extraction cache (unless --force or no cache dir).
    let mut cache = if force || cache_dir.is_none() {
        ExtractionCache::new(local_prefix)
    } else {
        let cache_path = cache_dir.unwrap().join(".graphify-cache.json");
        ExtractionCache::load(&cache_path, local_prefix)
            .unwrap_or_else(|| ExtractionCache::new(local_prefix))
    };

    let mut stats = CacheStats {
        forced: force,
        ..Default::default()
    };
```

Note: the `local_prefix` variable already exists earlier in the function — remove the duplicate `let local_prefix = ...` line that's already there (around line 704) and keep this one, or just reuse the existing one. The existing line is:

```rust
let local_prefix = project.local_prefix.as_deref().unwrap_or("");
```

So just add the cache loading code after the existing `local_prefix` line and after the `files.len()` warning block.

- [ ] **Step 4: Replace the parallel extraction loop with cache-aware version**

Replace the current extraction loop (lines ~757-775):

```rust
    // Extract each file in parallel via rayon, then collect results.
    let results: Vec<ExtractionResult> = files
        .par_iter()
        .filter_map(|file| {
            let source = match std::fs::read(&file.path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Warning: cannot read {:?}: {e}", file.path);
                    return None;
                }
            };

            let extractor: &dyn LanguageExtractor = match file.language {
                Language::Python => &python_extractor,
                Language::TypeScript => &typescript_extractor,
            };

            Some(extractor.extract_file(&file.path, &source, &file.module_name))
        })
        .collect();
```

With this cache-aware version:

```rust
    let repo_path_ref = &repo_path;

    // Extract each file in parallel: read → hash → cache check → parse on miss.
    let extraction_with_meta: Vec<(String, String, ExtractionResult, bool)> = files
        .par_iter()
        .filter_map(|file| {
            let source = match std::fs::read(&file.path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    eprintln!("Warning: cannot read {:?}: {e}", file.path);
                    return None;
                }
            };

            let rel_path = file
                .path
                .strip_prefix(repo_path_ref)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .to_string();

            let hash = sha256_hex(&source);

            // Cache hit: reuse previous extraction.
            if let Some(cached) = cache.lookup(&rel_path, &hash) {
                return Some((rel_path, hash, cached.clone(), true));
            }

            // Cache miss: parse with tree-sitter.
            let extractor: &dyn LanguageExtractor = match file.language {
                Language::Python => &python_extractor,
                Language::TypeScript => &typescript_extractor,
            };

            let result = extractor.extract_file(&file.path, &source, &file.module_name);
            Some((rel_path, hash, result, false))
        })
        .collect();

    // Build new cache from extraction results and count stats.
    let mut new_cache = ExtractionCache::new(local_prefix);
    let mut results: Vec<ExtractionResult> = Vec::with_capacity(extraction_with_meta.len());

    for (rel_path, hash, result, was_hit) in extraction_with_meta {
        if was_hit {
            stats.hits += 1;
        } else {
            stats.misses += 1;
        }
        new_cache.insert(rel_path, hash, result.clone());
        results.push(result);
    }

    // Count evictions: old cache entries whose paths aren't in the current discovered file set.
    let current_paths: HashSet<String> = new_cache.paths().cloned().collect();
    stats.evicted = cache.paths().filter(|p| !current_paths.contains(*p)).count();
```

This requires adding `paths()` and `entry_count()` helpers to `ExtractionCache`.

- [ ] **Step 5: Add `paths()` and `entry_count()` helpers to ExtractionCache**

In `crates/graphify-extract/src/cache.rs`, add to `impl ExtractionCache`:

```rust
    /// Returns an iterator over all cached relative paths.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// Returns the number of cached entries.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
```

- [ ] **Step 6: Save the new cache after extraction**

After the graph is built (after the `graph.add_edge` loop, before the `return`), add:

```rust
    // Save updated cache.
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(dir).ok();
        new_cache.save(&dir.join(".graphify-cache.json"));
    }

    (graph, extra_owned, stats)
```

- [ ] **Step 7: Update all `run_extract` call sites**

Update each call site to pass the new parameters and handle the return:

**Extract command** (~line 231):
```rust
Commands::Extract { config, output, force } => {
    let cfg = load_config(&config);
    let out_dir = resolve_output(&cfg, output.as_deref());
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let (graph, _excludes, stats) = run_extract(project, &cfg.settings, Some(&proj_out), force);
        write_graph_json(&graph, &proj_out.join("graph.json"));
        print_cache_stats(&project.name, &stats);
        println!(
            "[{}] Extracted {} nodes, {} edges → {}",
            project.name,
            graph.node_count(),
            graph.edge_count(),
            proj_out.join("graph.json").display()
        );
    }
}
```

**Analyze command** (~line 254):
```rust
Commands::Analyze { config, output, weights, force } => {
    let cfg = load_config(&config);
    let out_dir = resolve_output(&cfg, output.as_deref());
    let w = resolve_weights(&cfg, weights.as_deref());
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let (graph, _, stats) = run_extract(project, &cfg.settings, Some(&proj_out), force);
        print_cache_stats(&project.name, &stats);
        // ... rest unchanged ...
```

**Report command** (~line 291):
```rust
Commands::Report { config, output, weights, format, force } => {
    // ...
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let (graph, _, stats) = run_extract(project, &cfg.settings, Some(&proj_out), force);
        print_cache_stats(&project.name, &stats);
        // ... rest unchanged ...
```

**Run command** (~line 330):
```rust
Commands::Run { config, output, force } => {
    // ...
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let (graph, _, stats) = run_extract(project, &cfg.settings, Some(&proj_out), force);
        print_cache_stats(&project.name, &stats);
        // ... rest unchanged ...
```

**`build_query_engine`** (~line 871):
```rust
fn build_query_engine(project: &ProjectConfig, settings: &Settings) -> QueryEngine {
    let (graph, _, _stats) = run_extract(project, settings, None, false);
    // ... rest unchanged ...
```

- [ ] **Step 8: Add `print_cache_stats` helper**

Add this helper function near the other helpers in `main.rs`:

```rust
fn print_cache_stats(project_name: &str, stats: &CacheStats) {
    if stats.forced {
        eprintln!("[{}] Cache: forced full rebuild", project_name);
    } else if stats.hits > 0 || stats.evicted > 0 {
        eprintln!(
            "[{}] Cache: {} hits, {} misses, {} evicted",
            project_name, stats.hits, stats.misses, stats.evicted
        );
    }
}
```

- [ ] **Step 9: Verify it compiles and tests pass**

Run: `cargo build -p graphify-cli && cargo test --workspace`
Expected: compiles and all existing tests pass

- [ ] **Step 10: Commit**

```bash
git add crates/graphify-extract/src/cache.rs crates/graphify-cli/src/main.rs
git commit -m "feat(cli,extract): integrate extraction cache into pipeline (FEAT-005)"
```

---

### Task 7: Integration Tests

**Files:**
- Modify: `crates/graphify-extract/src/cache.rs`

- [ ] **Step 1: Write integration test — full round-trip with Python fixture**

Add to the `tests` module in `cache.rs`:

```rust
    #[test]
    fn integration_cache_with_python_extraction() {
        use crate::{PythonExtractor, LanguageExtractor};
        use graphify_core::types::Language;

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
        let src_dir = tempfile::tempdir().unwrap();
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
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p graphify-extract cache::tests::integration`
Expected: both pass

- [ ] **Step 3: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass (220 existing + ~16 new cache tests)

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/src/cache.rs
git commit -m "test(extract): integration tests for extraction cache (FEAT-005)"
```

---

### Task 8: Documentation and Sprint Board Update

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`

- [ ] **Step 1: Update CLAUDE.md conventions section**

Add to the "Conventions" section in `CLAUDE.md`:

```markdown
- Extraction cache: `.graphify-cache.json` in each project's output directory, keyed by SHA256 of file contents
- Cache is on by default; `--force` flag bypasses it (full rebuild, fresh cache saved)
- Cache invalidation: version mismatch or `local_prefix` change → full discard
- Query commands (query, explain, path, shell) don't use cache — always fresh extraction
```

- [ ] **Step 2: Update sprint board**

In `docs/TaskNotes/Tasks/sprint.md`, change FEAT-005 status:

```markdown
| FEAT-005 | **done** | high     | 16h    | Incremental builds with SHA256 cache                 |
```

And add to the Done section:

```markdown
- [[FEAT-005-incremental-builds]] - Implemented: SHA256-based extraction cache, per-file caching, --force flag, cache stats output (2026-04-12)
```

- [ ] **Step 3: Verify clippy is clean**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Run full test suite one final time**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md docs/TaskNotes/Tasks/sprint.md
git commit -m "docs: update CLAUDE.md and sprint board for FEAT-005 incremental builds"
```
