---
status: done
completed: 2026-04-12
priority: low
timeEstimate: 30
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - report
tags:
  - task
  - bug
  - report
  - csv
uid: bug-005
---

# fix(report): CSV nodes file missing kind, file_path, language columns

## Description

The `graph_nodes.csv` output file is missing several useful columns that exist on the `Node` struct: `kind` (module/function/class), `file_path`, and `language`. These are available in the JSON output but not in CSV.

## Impact

- CSV consumers (Excel, pandas, R) get incomplete data
- Users must parse JSON for basic node attributes

## Affected Code

- `crates/graphify-report/` — CSV writer for nodes
