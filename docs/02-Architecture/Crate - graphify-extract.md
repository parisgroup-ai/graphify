---
title: "Crate: graphify-extract"
created: 2026-04-14
updated: 2026-04-14
status: published
owner: Cleiton Paris
component_status: active
tags:
  - type/component
  - crate
related:
  - "[[Tech Stack]]"
  - "[[Data Flow]]"
  - "[[Crate - graphify-core]]"
---

# `graphify-extract`

AST extraction. Walks the filesystem, parses source files via tree-sitter, resolves module references, and serves the SHA256 cache. **The only crate that knows about tree-sitter.**

## Overview

| Property | Value |
|---|---|
| Path | `crates/graphify-extract/` |
| Binary? | No (library only) |
| Lines of code | ~6700 |
| Modules | 11 |
| Depends on | `graphify-core`, `tree-sitter` (+4 grammars), `rayon`, `sha2`, `serde`, `serde_json` |
| Depended by | `graphify-cli`, `graphify-mcp` |

## Purpose

Turn source files into `ExtractionResult` (nodes + edges) per file. Caches results by SHA256, parallelizes per file, resolves module names into canonical IDs, and parses two non-graph artifacts (Drizzle ORM tables and TS contracts) for [[ADR-011 Contract Drift Detection]].

## Module map

| Module | LOC | Role |
|---|---|---|
| `lang.rs` | 40 | `LanguageExtractor` trait + `ExtractionResult` |
| `walker.rs` | 780 | File discovery, exclusion, `local_prefix` auto-detect, `is_package` flag |
| `python.rs` | 627 | Python extractor (imports, defs, calls) |
| `typescript.rs` | 740 | TS/TSX extractor (imports, exports, require, calls, re-exports) |
| `go.rs` | 625 | Go extractor (FEAT-003) |
| `rust_lang.rs` | 816 | Rust extractor (FEAT-003) — `crate::`/`super::`/`self::` resolution |
| `resolver.rs` | 971 | Cross-extractor module resolver: relative imports, TS path aliases |
| `cache.rs` | 376 | `ExtractionCache` — SHA256-keyed per-file results ([[ADR-003 SHA256 Extraction Cache]]) |
| `drizzle.rs` | 603 | Drizzle ORM table parser (walks the existing TS AST) |
| `ts_contract.rs` | 573 | TS interface/type alias parser for contract pairing |

## Public surface (highlights)

```rust
// lang.rs
pub trait LanguageExtractor {
    fn extensions(&self) -> &[&str];
    fn extract_file(&self, path: &Path, source: &[u8], module_name: &str) -> ExtractionResult;
}
pub struct ExtractionResult { pub nodes: Vec<Node>, pub edges: Vec<(String, String, Edge)> }

// walker.rs
pub struct DiscoveredFile { /* path, language, module_name, is_package */ }
pub fn discover_files(repo: &Path, langs: &[Language], local_prefix: &str, excludes: &[String]) -> Vec<DiscoveredFile>;
pub fn detect_local_prefix(repo: &Path, langs: &[Language], excludes: &[String]) -> String;
pub fn path_to_module(path: &Path, repo: &Path, local_prefix: &str) -> String;

// Per-language extractors (all impl LanguageExtractor)
pub struct PythonExtractor;
pub struct TypeScriptExtractor;
pub struct GoExtractor;
pub struct RustExtractor;

// resolver.rs (used by the CLI pipeline, not exported from lib.rs)
pub struct ModuleResolver { /* ... */ }
impl ModuleResolver {
    pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool, f64);
}

// cache.rs
pub struct ExtractionCache { /* ... */ }
impl ExtractionCache {
    pub fn load(path: &Path, expected_local_prefix: &str) -> Option<Self>;
    pub fn lookup(&self, rel_path: &str, sha256: &str) -> Option<&ExtractionResult>;
    pub fn insert(&mut self, rel_path: String, sha256: String, result: ExtractionResult);
    pub fn save(&self, path: &Path);
}
pub fn sha256_hex(data: &[u8]) -> String;

// Contract parsers (FEAT-016)
pub fn extract_drizzle_contract(...) -> Result<Contract, DrizzleParseError>;
pub fn extract_ts_contract(...) -> Result<Contract, TsContractParseError>;
pub fn parse_all_ts_contracts(...) -> ...;
```

## Design properties

### One fresh `Parser` per file

Tree-sitter `Parser` is `!Send` — it cannot cross thread boundaries. The extraction loop creates a new `Parser` inside each `rayon` worker, paying the construction cost (~µs) to gain full multi-core parallelism. Documented in `CLAUDE.md` as a project convention.

### Resolver returns confidence

`ModuleResolver::resolve()` returns `(canonical_id, is_local, confidence)`. The confidence values are heuristic ([[ADR-006 Edge Confidence Scoring]]):

| Path | Confidence |
|---|---|
| Direct match | 1.0 |
| Python relative (`from . import x` with `is_package`) | 0.9 |
| TS relative (`import './x'`) | 0.9 |
| TS alias (`import '@/lib/x'` from `tsconfig.json`) | 0.85 |
| Unknown / non-local | downgraded to ≤0.5 / `Ambiguous` |

The CLI pipeline applies `min(extractor_confidence, resolver_confidence)` then the non-local downgrade.

### `is_package` correctness

Walker tags `__init__.py` and `index.ts` files with `is_package: true`. The resolver uses this to correctly resolve `from . import x` from a package entry point — fixing the false-cycle bug reported in BUG-001.

### Drizzle and TS contract parsers reuse the TS AST

No new tree-sitter grammar for Drizzle. The Drizzle parser walks the **same** TypeScript AST produced by `typescript.rs`. Same goes for the TS contract extractor. This keeps the grammar surface small and ensures consistency with the rest of the TS pipeline.

### Cache is content-addressable

Cache key = SHA256 of file bytes. Single-pass: read bytes → hash → check cache → either return cached `ExtractionResult` or extract and insert. No double-reads. Cache invalidation: file content change (per-entry) or `local_prefix`/cache-version mismatch (full discard).

## Extractor patterns — quick reference

### Python (`python.rs`)

| Pattern | Edge |
|---|---|
| `import os` | `Imports → os` |
| `from x import y` | `Imports → x` + `Calls → y` |
| `from . import utils` | resolved relative to package |
| `def f():` | `Defines → f` (NodeKind::Function) |
| `class C:` | `Defines → C` (NodeKind::Class) |
| Bare call `f()` | `Calls → f` with confidence 0.7 / Inferred |

### TypeScript (`typescript.rs`)

| Pattern | Edge |
|---|---|
| `import { api } from '@/lib/api'` | `Imports → resolved alias` |
| `import React from 'react'` | `Imports → react` |
| `export function f()` | `Defines → f` |
| `export { foo } from './bar'` | `Imports → bar` + `Defines → foo` |
| `const x = require('./util')` | `Imports → util` |

### Go and Rust (`go.rs`, `rust_lang.rs`)

Added in FEAT-003. Same edge model. Go uses `go.mod` as its resolver hint; Rust resolves `crate::`/`super::`/`self::` against the module tree.

## Testing

```bash
cargo test -p graphify-extract
```

Each language extractor has its own `#[cfg(test)]` block with **inline source strings** as fixtures. The walker has integration tests using `tempfile`. Cache tests cover round-trip, hit, miss, eviction, version mismatch.

## Common gotchas

- **Excludes are directory-level**, not glob-level. Adding `*.test.ts` to `exclude` does nothing — the built-in test glob handles those.
- **`local_prefix` mismatch invalidates the entire cache.** Document this when bumping config fields.
- **Tree-sitter grammar versions** (0.23–0.25) span minor releases — runtime API is stable, but version-bump them in lockstep next time.
- **`Parser::set_language()` failure means the grammar is incompatible with the runtime.** If extraction silently produces 0 nodes for a language, check the grammar version pin.

## Related

- [[Data Flow]] — pipeline stages 2 (walker), 3 (extract), 4 (resolver)
- [[Crate - graphify-core]] — owns `Node`, `Edge`, `Confidence` consumed here
- [[ADR-001 Rust Rewrite]] · [[ADR-003 SHA256 Extraction Cache]] · [[ADR-006 Edge Confidence Scoring]] · [[ADR-010 Auto-Detect Local Prefix]] · [[ADR-011 Contract Drift Detection]]
