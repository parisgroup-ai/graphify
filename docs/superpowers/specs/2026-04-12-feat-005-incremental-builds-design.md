# FEAT-005: Incremental Builds with SHA256 Cache — Design Spec

**Date:** 2026-04-12
**Status:** Draft
**Priority:** High
**Estimated effort:** 16h

## Problem

Every `graphify run` re-discovers and re-parses ALL source files via tree-sitter, even when nothing has changed. For large monorepos (1000+ files), this is wasteful — tree-sitter parsing dominates wall-clock time while producing identical results for unchanged files.

## Goal

Add SHA256-based file caching so the extraction pipeline skips tree-sitter parsing for files whose contents haven't changed since the last run. Cache is on by default; `--force` bypasses it.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Cache default | On by default, `--force` to bypass | Matches modern build tools (cargo, esbuild). Users get speedup without opt-in. |
| Cache granularity | Per-file `ExtractionResult` | Resolution depends on the full module set and must always re-run. Per-file caching is simple to invalidate. |
| Cache location | Output directory (`<output>/<project>/.graphify-cache.json`) | Discoverable, intuitive. Deleting output directory triggers clean rebuild. |
| Module location | `crates/graphify-extract/src/cache.rs` | Serializes extraction types, uses SHA256 — extraction-adjacent. CLI just calls load/save. |
| Hash crate | `sha2` (pure Rust) | No system deps, ~200KB compile impact, well-maintained. |

## Cache File Format

One JSON file per project at `<output_dir>/<project_name>/.graphify-cache.json`:

```json
{
  "version": 1,
  "local_prefix": "app",
  "entries": {
    "app/main.py": {
      "sha256": "a1b2c3d4e5f6...",
      "nodes": [
        {
          "id": "app.main",
          "kind": "Module",
          "file_path": "app/main.py",
          "language": "Python",
          "line": 1,
          "is_local": true
        }
      ],
      "edges": [
        {
          "source": "app.main",
          "target": "os",
          "kind": "Imports",
          "weight": 1,
          "line": 1,
          "confidence": 1.0,
          "confidence_kind": "Extracted"
        }
      ]
    }
  }
}
```

### Fields

- **`version`** (`u32`): Format version. If the loaded cache has a different version than the current code expects, the entire cache is discarded.
- **`local_prefix`** (`String`): The `local_prefix` config value at cache time. If this differs from the current config, the entire cache is discarded (module names in cached extraction results would be stale).
- **`entries`** (`HashMap<String, CacheEntry>`): Keyed by file path relative to the project repo root.
  - **`sha256`** (`String`): Hex-encoded SHA256 of the raw file bytes.
  - **`nodes`** (`Vec<Node>`): Cached extraction result nodes.
  - **`edges`** (`Vec<(String, String, Edge)>`): Cached extraction result edges (source_id, target_id, edge).

## Pipeline Changes

### Modified `run_extract` Flow

```
discover_files(repo, languages, local_prefix, excludes)
    |
    v
load cache from <output>/<project>/.graphify-cache.json
  - if --force: skip load (use empty cache)
  - if version mismatch: discard
  - if local_prefix mismatch: discard
    |
    v
[parallel] for each discovered file:
  1. read file bytes into Vec<u8>
  2. compute SHA256 of bytes
  3. check cache[rel_path].sha256 == computed_hash
     - CACHE HIT: use cached ExtractionResult
     - CACHE MISS: parse bytes with tree-sitter extractor
  4. record (rel_path, sha256, ExtractionResult) for cache update
    |
    v
merge all ExtractionResults (cached + fresh)
    |
    v
resolve edges (ALWAYS runs — depends on full module set)
    |
    v
build CodeGraph
    |
    v
save updated cache (only entries for currently discovered files)
    |
    v
return CodeGraph
```

### Key Property: Single-Pass Read + Hash + Extract

Each file is read once into `Vec<u8>`. SHA256 is computed on those bytes (~1ms per file). On cache hit, the same bytes are discarded; on cache miss, they are passed directly to tree-sitter. No double-reads.

### File Lifecycle

| Event | Behavior |
|---|---|
| File unchanged | SHA256 matches → use cached ExtractionResult |
| File modified | SHA256 differs → re-extract with tree-sitter |
| File deleted | Not in discovered file list → entry evicted from cache on save |
| File added | No cache entry → extracted fresh |
| Config `local_prefix` changed | Top-level mismatch → entire cache discarded |
| Cache file missing | Full extraction (first run) |
| Cache version mismatch | Entire cache discarded |
| `--force` flag | Cache not loaded; fresh cache saved for next run |

## CLI Changes

Add `--force` flag to all pipeline commands:

```rust
/// Force full rebuild, ignoring extraction cache
#[arg(long)]
force: bool,
```

Commands affected: `Extract`, `Analyze`, `Report`, `Run`.

The `--force` flag is threaded through to `run_extract` as a boolean parameter. When true, `run_extract` skips cache loading but still saves the fresh extraction cache for the next run.

## Cache Module: `crates/graphify-extract/src/cache.rs`

### Public API

```rust
use std::path::Path;
use crate::lang::ExtractionResult;

/// On-disk extraction cache for incremental builds.
pub struct ExtractionCache {
    local_prefix: String,
    entries: HashMap<String, CacheEntry>,
}

struct CacheEntry {
    sha256: String,
    result: ExtractionResult,
}

impl ExtractionCache {
    /// Load cache from disk. Returns None if file doesn't exist,
    /// version mismatches, local_prefix mismatches, or JSON is invalid.
    pub fn load(path: &Path, expected_local_prefix: &str) -> Option<Self>;

    /// Look up a cached extraction result by relative file path and SHA256.
    /// Returns Some(ExtractionResult) on cache hit, None on miss.
    pub fn lookup(&self, rel_path: &str, sha256: &str) -> Option<&ExtractionResult>;

    /// Insert or update a cache entry.
    pub fn insert(&mut self, rel_path: String, sha256: String, result: ExtractionResult);

    /// Create a new empty cache with the given local_prefix.
    pub fn new(local_prefix: &str) -> Self;

    /// Remove entries whose paths are not in the given set of active paths.
    /// Called before save to evict deleted files.
    pub fn retain_paths(&mut self, active_paths: &HashSet<String>);

    /// Save cache to disk as JSON.
    pub fn save(&self, path: &Path);
}
```

### Serialization

`Node` and `Edge` (in `graphify-core/src/types.rs`) already derive `Serialize`/`Deserialize`. `ExtractionResult` (in `graphify-extract/src/lang.rs`) needs `#[derive(Clone, Serialize, Deserialize)]` added — it currently has no serde derives.

Edge serialization in the cache uses a tuple `(String, String, Edge)` — serde handles this natively.

### SHA256 Helper

```rust
use sha2::{Sha256, Digest};

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
```

## Dependency Addition

Add to `crates/graphify-extract/Cargo.toml`:

```toml
sha2 = "0.10"
```

The `sha2` crate is pure Rust, no system dependencies, well-maintained (~80M downloads). Compile impact: ~200KB to binary size.

## Console Output

Report cache statistics on stderr:

```
[ana-service] Cache: 47 hits, 3 misses, 2 evicted
[ana-service] Extracted 52 nodes, 87 edges → report/ana-service/graph.json
```

When `--force` is used:

```
[ana-service] Cache: forced full rebuild
[ana-service] Extracted 52 nodes, 87 edges → report/ana-service/graph.json
```

## MCP Server Impact

The MCP server (`graphify-mcp`) uses its own extraction pipeline that mirrors the CLI. For the initial implementation, the MCP server will NOT use caching — it extracts eagerly on startup and the server is typically short-lived. Cache support can be added to MCP later if needed.

## Performance Expectations

For a 500-file project:

| Scenario | File reads | Tree-sitter parses | Cache I/O | Estimated time |
|---|---|---|---|---|
| Full rebuild (first run) | 500 | 500 | 1 write | ~2s |
| Cached, no changes | 500 | 0 | 1 read + 1 write | ~0.5s |
| Cached, 5 files changed | 500 | 5 | 1 read + 1 write | ~0.6s |

SHA256 computation adds ~1ms per file. Cache JSON parse/serialize adds ~5-10ms for a 500-entry cache.

## Testing Strategy

All tests in `crates/graphify-extract/src/cache.rs`:

1. **Round-trip**: create cache, save, load → entries match
2. **Cache hit**: lookup with matching sha256 → returns ExtractionResult
3. **Cache miss (hash)**: lookup with different sha256 → returns None
4. **Cache miss (path)**: lookup with unknown path → returns None
5. **Eviction**: retain_paths with subset → removed entries gone
6. **Version mismatch**: load cache with wrong version → returns None
7. **Local prefix mismatch**: load cache with wrong prefix → returns None
8. **Invalid JSON**: load corrupt file → returns None (no panic)

Integration test in `crates/graphify-cli` (or `tests/`):

9. **End-to-end cache hit**: extract fixture project twice → second run reports cache hits, same graph output
10. **End-to-end cache miss**: extract, modify fixture file, extract again → modified file re-extracted
11. **End-to-end --force**: extract with cache, run with --force → full re-extraction, cache updated

## Non-Goals

- **Graph-level caching**: resolution always re-runs. Too many invalidation edge cases (tsconfig changes, module set changes).
- **Frontmatter-aware hashing**: Graphify doesn't process Markdown files. Irrelevant.
- **Parallel cache writes**: single JSON file, written once at end. No contention.
- **MCP server caching**: deferred to a future task if needed.
- **Watch mode integration**: FEAT-010 will build on this, but is out of scope here.
