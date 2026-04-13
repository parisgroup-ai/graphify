---
uid: feat-005
status: done
completed: 2026-04-12
priority: high
timeEstimate: 960
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - performance
  - pipeline
---

# Incremental builds with SHA256 cache

Add SHA256-based file caching so `graphify run` only re-extracts files that changed since the last run.

## Goals

- SHA256 hash each source file before extraction
- Persist hashes + extracted nodes/edges in a cache file (e.g., `.graphify-cache.json`)
- On subsequent runs, skip files whose hash hasn't changed
- Merge cached extractions with new ones before analysis
- Enable cache by default and add `--force` to bypass it for a full rebuild
- Frontmatter-aware hashing for Markdown files (ignore YAML metadata changes)

## Inspiration

safishamsi/graphify uses SHA256 cache + `--update` mode. On large codebases, only changed files are re-processed, then the graph is merged incrementally. This gives massive speedup on repeat runs.

## Subtasks

- [x] Design cache file format (file path → hash + extracted data)
- [x] Implement SHA256 hashing in walker
- [x] Add cache read/write module in graphify-extract
- [x] Modify extraction pipeline to skip cached files
- [x] Enable cache by default and add `--force` for full rebuilds
- [x] Tests: cache hit, cache miss, file deleted, file modified
- [x] Documentation update

## Notes

Our current pipeline does full rebuild every time. For large monorepos this is wasteful — extraction with rayon is fast, but tree-sitter parsing of 1000+ files still takes seconds.
