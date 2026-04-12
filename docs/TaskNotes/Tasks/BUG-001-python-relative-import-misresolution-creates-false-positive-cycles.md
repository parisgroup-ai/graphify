---
status: done
completed: 2026-04-12
priority: high
timeEstimate: 240
pomodoros: 0
projects:
  - "[[sprint.md|Graphify Open Issues]]"
contexts:
  - extract
  - resolver
designDoc: "[[2026-04-12-bug-001-python-relative-import-design]]"
tags:
  - task
  - bug
  - python
  - cycles
  - resolver
uid: bug-001
---

# fix(extract): Python relative import misresolution creates false-positive cycles

## Description

When a Python `__init__.py` uses a relative import like `from .llm import X`, Graphify's resolver incorrectly resolves it as an edge to the top-level `app.llm` package instead of the sibling module `app.errors.llm`.

## Reproduction

Given this file structure:
```
app/
├── errors/
│   ├── __init__.py    # contains: from .llm import LLMError
│   ├── base.py
│   └── llm.py         # defines LLMError
└── llm/
    ├── __init__.py
    └── gateway.py
```

**Expected:** Edge `app.errors` → `app.errors.llm` (sibling module)
**Actual:** Edges `app.errors` → `app.llm.LLMError` (wrong package!)

This produced 7 false-positive circular dependency cycles in the ana-service project of ToStudy monorepo, all rooting at `app.errors → app.llm`.

## Root Cause

The Python relative import resolver in `crates/graphify-extract/src/resolver.rs` likely strips the leading dot and resolves `.llm` as `app.llm` instead of computing the correct parent package path (`app.errors`) and appending `.llm` to get `app.errors.llm`.

## Impact

- False-positive cycles in any Python project where a submodule name matches a sibling package name
- Real-world example: `app/errors/llm.py` vs `app/llm/` in ToStudy's ana-service (7 false cycles)

## Fix Approach

In `resolver.rs`, when resolving Python relative imports (those starting with `.`):
1. Determine the importing file's parent package path (e.g., `app.errors` for `app/errors/__init__.py`)
2. For `from .X import Y`, resolve as `{parent}.X.Y`, NOT as `app.X.Y`
3. For `from ..X import Y`, go up one more level

## Affected Code

- `crates/graphify-extract/src/resolver.rs` — `resolve_python_import()` or equivalent
- `crates/graphify-extract/src/python.rs` — may pass relative import info to resolver

## Notes

Discovered during ToStudy TASK-220 (2026-04-12). The "fix" in the consumer project was to move `app/errors/llm.py` → `app/llm/errors.py` to eliminate the naming collision, but the real fix belongs in Graphify's resolver.
