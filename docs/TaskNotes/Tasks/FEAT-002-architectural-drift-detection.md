---
uid: feat-002
status: done
priority: normal
timeEstimate: 480
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - analysis
  - ci
---

# Architectural drift detection

Compare two analysis snapshots over time to surface what changed: new dependencies, new cycles, hotspot movements, community shifts.

## Goals

- [x] `graphify diff --before analysis-a.json --after analysis-b.json`
- [x] `graphify diff --baseline analysis.json --config graphify.toml` (baseline-vs-live mode)
- [x] Detect new/removed edges, new/resolved cycles
- [x] Track hotspot score changes over time
- [x] Output structured diff (JSON + human-readable summary)
- [x] `--threshold` flag for minimum score delta to report (default: 0.05)
- [x] `--project` flag for single-project baseline mode

## Verification (2026-04-13)

Confirmed working in Graphify v0.2.0:

```
graphify diff --help
  --before <BEFORE>        Path to the "before" analysis.json (file-vs-file mode)
  --after <AFTER>          Path to the "after" analysis.json (file-vs-file mode)
  --baseline <BASELINE>    Path to a baseline analysis.json (baseline-vs-live mode)
  --config <CONFIG>        Path to graphify.toml (for live extraction in baseline mode)
  --project <PROJECT>      Project name (for baseline mode with multi-project configs)
  --output <OUTPUT>        Output directory for drift report files
  --threshold <THRESHOLD>  Minimum score delta to report as significant (default: 0.05)
```

All original goals met. Integrated into ToStudy CLAUDE.md as post-refactor check workflow.

## Notes

Follow-up: FEAT-004 (CI quality gates) can build on this — `graphify diff` in CI pipelines to block PRs that introduce cycles.
