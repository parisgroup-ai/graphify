---
title: Sprint
created: 2026-04-12
updated: 2026-04-12
---

# Graphify — Issues

| ID      | Status      | Priority | Est    | Title                                                |
| ------- | ----------- | -------- | ------ | ---------------------------------------------------- |
| BUG-001 | **done**    | high     | 4h     | Python relative import misresolution (false cycles)  |
| BUG-002 | **done**    | normal   | 2h     | TS re-export missing Defines edge                    |
| BUG-003 | **done**    | normal   | 3h     | Cross-project summary is a stub                      |
| BUG-004 | **done**    | low      | 1h     | Placeholder nodes always tagged Language::Python      |
| BUG-005 | **done**    | low      | 30m    | CSV nodes file missing kind, file_path, language cols |

## Open

_(none — all bugs resolved!)_

## Done

- [[BUG-001-python-relative-import-misresolution-creates-false-positive-cycles]] - Fixed `is_package` resolution (2026-04-12)
- [[BUG-002-ts-reexport-missing-defines-edge]] - Already implemented: Defines edges for re-exported symbols (confirmed 2026-04-12)
- [[BUG-004-placeholder-nodes-always-tagged-python]] - Already implemented: `set_default_language` in pipeline (confirmed 2026-04-12)
- [[BUG-003-cross-project-summary-is-stub]] - Implemented full summary: per-project stats, aggregates, top hotspots, cross-deps (2026-04-12)
- [[BUG-005-csv-nodes-missing-columns]] - Already implemented: CSV includes kind, file_path, language (confirmed 2026-04-12)
