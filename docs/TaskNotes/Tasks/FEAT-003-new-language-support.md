---
uid: feat-003
status: done
completed: 2026-04-13
priority: low
timeEstimate: 960
tags:
  - task
  - feature
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - extraction
  - languages
---

# New language support (Go, Rust)

Add tree-sitter extractors for Go and Rust to broaden Graphify's reach.

## Goals

- Go extractor: imports, package declarations, function definitions
- Rust extractor: use/mod statements, function/struct definitions
- Reuse existing resolver patterns where applicable
- Dogfood: run Graphify on itself (Rust)

## Notes

Each language is ~2-3 days of work. Lower priority than improving output quality for existing languages.

## Verification (2026-04-13)

- Verified Go and Rust extractor support exists in `crates/graphify-extract`
- Verified Go `go.mod` resolution and Rust `crate::` / `super::` / `self::` resolution are present
- Verified CLI wires `GoExtractor` and `RustExtractor` into extraction pipeline
