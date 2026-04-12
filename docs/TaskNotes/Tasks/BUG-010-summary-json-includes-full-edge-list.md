---
status: done
priority: low
timeEstimate: 60
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - cli
  - report
tags:
  - task
  - bug
  - performance
uid: bug-010
---

# fix(cli): graphify-summary.json includes full edge list (9.6MB)

## Description

The cross-project summary file includes the complete `edges` array in `cross_dependencies`, making the file ~9.6MB for a 17-project monorepo. A summary should be compact — <100KB.

## Evidence

```
File: report/graphify-summary.json
Size: 9.6 MB (~276,000 lines)
Structure:
  - cross_dependencies[].edges[]  ← full edge list (the bloat)
  - per-project stats             ← small
  - aggregates                    ← small
```

For ToStudy monorepo (17 projects, 45K+ cross-project edges), the summary is larger than most individual project `analysis.json` files.

## Root Cause

In `crates/graphify-cli/src/main.rs`, `write_summary()` (around lines 578-660) serializes the full `cross_dependencies` structure including every edge. It should only include aggregate statistics.

## Fix Approach

1. **Summary mode:** Only include aggregate cross-project stats:
   ```json
   {
     "total_projects": 17,
     "total_cross_edges": 45306,
     "total_shared_modules": 420,
     "per_project": [{ "name": "...", "nodes": ..., "edges": ..., "cycles": ..., "top_hotspot": ... }],
     "coupling_pairs": [{ "from": "pkg-api", "to": "pkg-jobs", "shared_modules": 50 }]
   }
   ```
2. **Full edges in separate file:** If the full cross-dep edge list is needed, write it to `graphify-cross-deps.json` separately.

## Affected Code

- `crates/graphify-cli/src/main.rs` — `write_summary()`

## Impact

- 9.6MB file is impractical to read or pipe through tools
- Defeats the purpose of a "summary" (quick overview)
- `cat report/graphify-summary.json | python3 -m json.tool` takes seconds and floods terminals
- The data itself is correct, just too verbose for a summary
