# BUG-001: Python Relative Import Misresolution — Design Document

**Task:** [[BUG-001-python-relative-import-misresolution-creates-false-positive-cycles]]
**Status:** Open
**Priority:** High
**Estimated:** 4h (~10 pomodoros)

## Problem Statement

Python relative imports from `__init__.py` files are incorrectly resolved. The resolver always pops the last component of `from_module` as the "leaf module name," but for `__init__.py` files, the `from_module` IS the package — so popping walks up one level too far.

### Concrete Example

```
app/
├── errors/
│   ├── __init__.py    # from .llm import LLMError
│   └── llm.py
└── llm/
    ├── __init__.py
    └── gateway.py
```

- `from_module` for `app/errors/__init__.py` = `app.errors` (init collapsed to parent)
- `.llm` should resolve to `app.errors.llm` (sibling of `__init__.py`)
- **Bug:** resolver pops `errors` → resolves to `app.llm` (wrong package!)
- **Impact:** 7 false-positive cycles in ana-service project

## Root Cause Analysis

### Current Code (`crates/graphify-extract/src/resolver.rs:184-216`)

```rust
fn resolve_python_relative(raw: &str, from_module: &str) -> String {
    let dot_count = raw.chars().take_while(|&c| c == '.').count();
    let suffix = &raw[dot_count..];

    let mut parts: Vec<&str> = from_module.split('.').collect();

    // BUG: Always pops the leaf — wrong for __init__.py modules
    if !parts.is_empty() {
        parts.pop();
    }

    for _ in 0..dot_count.saturating_sub(1) {
        if !parts.is_empty() {
            parts.pop();
        }
    }
    // ...
}
```

The function has no way to distinguish between:
- `from_module = "app.errors"` from `app/errors/__init__.py` (IS the package)
- `from_module = "app.errors.base"` from `app/errors/base.py` (is a module IN the package)

For `__init__.py`, the module IS the package, so `.llm` means "sibling module within me" — no upward navigation needed for the first dot.

### Why This Matters

The `resolve_python_relative` function is called from `ModuleResolver::resolve()` at line 144. The `resolve()` method receives `from_module` as a plain string with no metadata about whether it originated from an `__init__.py`.

## Architecture Decision

### Option A: Add `is_package` parameter to `resolve_python_relative`

Change the function signature to accept a boolean:

```rust
fn resolve_python_relative(raw: &str, from_module: &str, is_package: bool) -> String
```

When `is_package` is true, skip the initial `parts.pop()`.

**Pros:** Minimal change, clear semantics
**Cons:** Must thread `is_package` through `ModuleResolver::resolve()`

### Option B: Pass the original file path and detect `__init__.py`

```rust
fn resolve_python_relative(raw: &str, from_module: &str, file_path: Option<&str>) -> String
```

Auto-detect `is_package` from `file_path.ends_with("__init__.py")`.

**Pros:** Caller doesn't need to compute `is_package`
**Cons:** Leaks file system concern into resolution logic

### Recommendation: **Option A**

The `is_package` flag is clean, testable, and matches Python's own import model where `__init__.py` modules are "package modules."

## Implementation Plan

### Step 1: Modify `resolve_python_relative` signature

**File:** `crates/graphify-extract/src/resolver.rs`

```rust
fn resolve_python_relative(raw: &str, from_module: &str, is_package: bool) -> String {
    let dot_count = raw.chars().take_while(|&c| c == '.').count();
    let suffix = &raw[dot_count..];

    let mut parts: Vec<&str> = from_module.split('.').collect();

    // Only pop the leaf if this is NOT a package (__init__.py).
    // For packages, from_module already IS the package.
    if !is_package && !parts.is_empty() {
        parts.pop();
    }

    // Walk up (dot_count - 1) additional levels.
    for _ in 0..dot_count.saturating_sub(1) {
        if !parts.is_empty() {
            parts.pop();
        }
    }

    if suffix.is_empty() {
        parts.join(".")
    } else if parts.is_empty() {
        suffix.to_owned()
    } else {
        format!("{}.{}", parts.join("."), suffix)
    }
}
```

### Step 2: Update `ModuleResolver::resolve()` API

**File:** `crates/graphify-extract/src/resolver.rs`

Add `is_package` parameter to the public `resolve()` method:

```rust
pub fn resolve(&self, raw: &str, from_module: &str, is_package: bool) -> (String, bool) {
    if raw.starts_with('.') && !raw.starts_with("./") && !raw.starts_with("../") {
        let resolved = resolve_python_relative(raw, from_module, is_package);
        let is_local = self.known_modules.contains_key(&resolved);
        return (resolved, is_local);
    }
    // ... rest unchanged (TS aliases don't use is_package)
}
```

### Step 3: Thread `is_package` from callers

**File:** `crates/graphify-extract/src/python.rs`

The Python extractor already knows the file path. Detect `__init__.py`:

```rust
let is_package = file_path.ends_with("__init__.py");
// ... later, when calling resolver:
let (resolved, is_local) = resolver.resolve(&import_str, &module_name, is_package);
```

**File:** `crates/graphify-extract/src/typescript.rs`

For TypeScript, `index.ts`/`index.tsx` are analogous to `__init__.py`:

```rust
let is_package = file_path.ends_with("index.ts")
    || file_path.ends_with("index.tsx")
    || file_path.ends_with("index.js");
// ... when calling resolver:
let (resolved, is_local) = resolver.resolve(&import_str, &module_name, is_package);
```

### Step 4: Update all existing tests

**File:** `crates/graphify-extract/src/resolver.rs` (tests module)

All existing calls to `r.resolve(...)` need a third argument. Existing tests should pass `false` (they test non-package modules):

```rust
let (id, is_local) = r.resolve("app.services.llm", "app.main", false);
```

### Step 5: Add new tests for `__init__.py` resolution

```rust
#[test]
fn resolve_python_relative_from_init_single_dot() {
    // .llm from app.errors (__init__.py) → app.errors.llm
    let r = make_resolver();
    let (id, _) = r.resolve(".llm", "app.errors", true);
    assert_eq!(id, "app.errors.llm");
}

#[test]
fn resolve_python_relative_from_init_double_dot() {
    // ..models from app.errors (__init__.py) → app.models
    let r = make_resolver();
    let (id, _) = r.resolve("..models", "app.errors", true);
    assert_eq!(id, "app.models");
}

#[test]
fn resolve_python_relative_from_init_bare_dot() {
    // . from app.errors (__init__.py) → app.errors
    let r = make_resolver();
    let (id, _) = r.resolve(".", "app.errors", true);
    assert_eq!(id, "app.errors");
}
```

### Step 6: Verify no regressions

```bash
cargo test --workspace
```

All 122+ existing tests must pass. The false-positive cycles from ana-service should disappear when re-analyzed.

## Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `crates/graphify-extract/src/resolver.rs` | Modify | Add `is_package` param to `resolve()` and `resolve_python_relative()` |
| `crates/graphify-extract/src/python.rs` | Modify | Detect `__init__.py` and pass `is_package=true` |
| `crates/graphify-extract/src/typescript.rs` | Modify | Detect `index.ts/tsx` and pass `is_package=true` |
| `crates/graphify-extract/src/resolver.rs` (tests) | Modify | Update existing tests, add `__init__.py` test cases |

## Validation Criteria

1. `cargo test --workspace` passes (all existing + new tests)
2. `resolve(".llm", "app.errors", true)` returns `"app.errors.llm"`
3. `resolve(".llm", "app.errors.base", false)` returns `"app.errors.llm"` (unchanged behavior)
4. `resolve("..models", "app.errors", true)` returns `"app.models"`
5. Running `graphify run` on ana-service produces 0 false-positive cycles from the `app.errors → app.llm` path

## Risk Assessment

- **Low risk**: The change is additive — existing callers that pass `false` get identical behavior
- **Medium confidence**: The `is_package` heuristic (`__init__.py` / `index.ts`) covers all known cases
- **Edge case**: Namespace packages (no `__init__.py`) are not affected — they don't have package-level imports
