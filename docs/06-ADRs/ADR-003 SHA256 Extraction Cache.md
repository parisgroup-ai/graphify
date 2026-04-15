---
title: "ADR-003: SHA256 Per-File Extraction Cache"
created: 2026-04-12
status: accepted
deciders:
  - Cleiton Paris
requirements:
  - "FEAT-005"
tags:
  - type/adr
  - status/accepted
  - performance
  - extract
supersedes:
superseded_by:
---

# ADR-003: SHA256 Per-File Extraction Cache

## Status

**Accepted** — 2026-04-12

## Context

Every `graphify run` re-parsed every source file via tree-sitter, even when nothing had changed. On a 1000-file monorepo, tree-sitter parsing dominated wall-clock time and produced identical results for unchanged files — pure waste. We wanted "incremental builds" comparable to modern build tools (`cargo`, `esbuild`) where unchanged inputs skip work.

## Decision

**Chosen option:** Cache `ExtractionResult` per file, keyed by **SHA256 of file contents**, on disk at `<project_out>/.graphify-cache.json`. **On by default**; `--force` bypasses it. Cache invalidation: file content change (per-entry) or `local_prefix` / cache version mismatch (full discard).

Resolution and graph-merge phases are **not cached** — they depend on the full module set and must always re-run.

## Consequences

### Positive

- Sub-second runs after first cold build (typical: 0.5s vs 2s for 500-file project)
- Single-pass `read → hash → extract` — no double-reads
- Per-file granularity means small edits cost almost nothing
- Cache file is plain JSON, debuggable and human-readable
- Discoverable: lives next to outputs, deleting `<output>` triggers clean rebuild

### Negative

- `.graphify-cache.json` can grow to several MB on big repos
- SHA256 adds ~1ms per file (negligible, but measurable on 10k-file repos)
- Resolution still re-runs every time — limits the speedup ceiling
- `Edge`/`Node` types must remain serde-stable (cache version bumps on schema break)
- Query commands (`query`, `path`, `explain`, `shell`) bypass the cache by design — fresh extraction matters there

## Options Considered

| Option | Pros | Cons |
|---|---|---|
| **Per-file SHA256 cache** (chosen) | Simple, correct invalidation, big win | Resolution still re-runs |
| Per-file mtime cache | Cheaper than SHA256 | Wrong on `git checkout` (same mtime, different content) |
| Whole-graph cache | Largest possible speedup | Invalidation surface is enormous |
| No cache | Simplest | Wastes time; users would build their own |
| Hash via cheaper algo (xxhash) | Faster | sha2 is fast enough; SHA256 is universally trusted |

## Plan de Rollback

**Triggers:** Cache produces incorrect results (false cache hits) or the JSON file becomes a corruption hazard.

**Steps:**
1. Default `--force` to true (cache loaded but always invalidated)
2. Delete `.graphify-cache.json` files via `find <output> -name '.graphify-cache.json' -delete`
3. If structural: revert `graphify-extract/src/cache.rs` and remove the `cache_stats` line from stderr output

**Validation:** Pipeline produces identical `analysis.json` with and without cache (compare via `graphify diff`). Worst case: extra 1.5s per run on big repos.

## Links

- Spec: `docs/superpowers/specs/2026-04-12-feat-005-incremental-builds-design.md`
- Plan: `docs/superpowers/plans/2026-04-12-feat-005-incremental-builds.md`
- Task: `[[FEAT-005-incremental-builds]]`
- Related ADRs: [[ADR-001 Rust Rewrite]], [[ADR-009 Watch Mode]] (consumer of this cache)
