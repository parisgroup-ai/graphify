# FEAT-049 — Multi-root `local_prefix` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Accept `local_prefix` as either a string (current wrapping behavior) or an array of top-level dirs (no-wrap, is-local hint), unblocking Expo Router and similar multi-root projects without breaking any existing config.

**Architecture:** Introduce a `serde(untagged)` enum `LocalPrefix { Single(String), Multi(Vec<String>) }` collapsed at load time into an internal `EffectiveLocalPrefix { prefixes: Vec<String>, wrap: bool }`. Plumb the effective form through the walker, resolver, cache, barrel exclusion, and `suggest stubs` shadow-set. Walker discovery is unchanged — the array is purely a naming + downstream-hint layer.

**Tech Stack:** Rust 2021, serde, clap, tree-sitter (transitively), petgraph (transitively).

**Spec:** `docs/superpowers/specs/2026-05-02-feat-049-multi-root-local-prefix-design.md` (commit `27f81fa`)

---

## File Structure

| File | Role | Change |
|---|---|---|
| `crates/graphify-cli/src/local_prefix.rs` | New module | Holds `LocalPrefix` enum (deserialized from TOML) and `EffectiveLocalPrefix` struct (collapsed runtime form), plus their conversion + helpers |
| `crates/graphify-cli/src/main.rs` | Existing | `ProjectConfig.local_prefix` field type, validation in `load_config`, wire `EffectiveLocalPrefix` through `run_extract*` + `barrel_exclusion_ids` |
| `crates/graphify-extract/src/walker.rs` | Existing | `path_to_module`, `path_to_go_package`, `path_to_module_psr4`, `discover_files*` accept `&EffectiveLocalPrefix`; `detect_local_prefix` adds multi-root advisory warning |
| `crates/graphify-extract/src/resolver.rs` | Existing | `set_local_prefix(&[String])` + internal `wrap_mode: bool`; `apply_local_prefix` no-ops in no-wrap mode |
| `crates/graphify-extract/src/cache.rs` | Existing | Cache key derived from `EffectiveLocalPrefix` (sorted-join with `multi:` prefix in array mode) |
| `crates/graphify-extract/src/lib.rs` | Existing | Re-export `EffectiveLocalPrefix` so callers (CLI, MCP) can construct it without touching internals |
| `crates/graphify-mcp/src/main.rs` | Existing | Mirror `ProjectConfig` change + plumbing to walker/resolver |
| `crates/graphify-report/src/suggest.rs` | Existing | `ProjectInput.local_prefix` becomes `local_prefixes: &[&str]` |
| `Cargo.toml` (workspace) | Existing | Bump `[workspace.package].version` after all tasks pass (closing task) |
| `CHANGELOG.md` | Existing | Add `FEAT-049` entry under unreleased / new minor bump |
| `CLAUDE.md` | Existing | Add convention bullet for multi-prefix semantics + cache key shape |

`graphify-extract` becomes the canonical home for `EffectiveLocalPrefix` (re-exported from `graphify-cli::local_prefix`) so both `graphify-cli` and `graphify-mcp` consume identical types — avoids the trap where the duplicated MCP config drifts. The `LocalPrefix` enum (TOML-facing) stays in `graphify-cli` because both CLI and MCP currently parse their own `Config` struct independently; we'll deserialize via the same enum but it's cheap to define twice if absolutely required (we'll keep one canonical definition in `graphify-extract` and re-export).

---

## Task 1: New `local_prefix` module — types + collapse logic

**Files:**
- Create: `crates/graphify-extract/src/local_prefix.rs`
- Modify: `crates/graphify-extract/src/lib.rs` (add `pub mod local_prefix; pub use local_prefix::{LocalPrefix, EffectiveLocalPrefix};`)

- [ ] **Step 1: Write the failing tests**

Create `crates/graphify-extract/src/local_prefix.rs` with this test module appended to whatever stub:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_string_collapses_to_wrap_mode() {
        let lp = LocalPrefix::Single("src".to_string());
        let eff = EffectiveLocalPrefix::from(&lp);
        assert_eq!(eff.prefixes, vec!["src".to_string()]);
        assert!(eff.wrap);
    }

    #[test]
    fn multi_array_collapses_to_no_wrap_mode() {
        let lp = LocalPrefix::Multi(vec!["app".to_string(), "lib".to_string()]);
        let eff = EffectiveLocalPrefix::from(&lp);
        assert_eq!(eff.prefixes, vec!["app".to_string(), "lib".to_string()]);
        assert!(!eff.wrap);
    }

    #[test]
    fn multi_dedups_duplicates_preserving_first_seen_order() {
        let lp = LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
            "app".to_string(),
        ]);
        let eff = EffectiveLocalPrefix::from(&lp);
        assert_eq!(eff.prefixes, vec!["app".to_string(), "lib".to_string()]);
    }

    #[test]
    fn empty_string_is_legal_and_collapses_to_empty_wrap() {
        // Matches current Single("") semantics, which downstream treats as "no prefix".
        let lp = LocalPrefix::Single(String::new());
        let eff = EffectiveLocalPrefix::from(&lp);
        assert_eq!(eff.prefixes, vec![String::new()]);
        assert!(eff.wrap);
    }

    #[test]
    fn omitted_collapses_to_empty_wrap_mode() {
        let eff = EffectiveLocalPrefix::omitted();
        assert_eq!(eff.prefixes, vec![String::new()]);
        assert!(eff.wrap);
    }

    #[test]
    fn first_prefix_returns_head() {
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));
        assert_eq!(eff.first(), "app");
    }

    #[test]
    fn first_prefix_empty_when_omitted() {
        let eff = EffectiveLocalPrefix::omitted();
        assert_eq!(eff.first(), "");
    }

    #[test]
    fn cache_key_single_returns_bare_prefix() {
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
        assert_eq!(eff.cache_key(), "src");
    }

    #[test]
    fn cache_key_multi_uses_marker_and_sorted_join() {
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "lib".to_string(),
            "app".to_string(),
            "components".to_string(),
        ]));
        assert_eq!(eff.cache_key(), "multi:app|components|lib");
    }

    #[test]
    fn cache_key_stable_under_array_reorder() {
        let a = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));
        let b = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "lib".to_string(),
            "app".to_string(),
        ]));
        assert_eq!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn cache_key_differs_between_single_and_multi_with_same_value() {
        let single = EffectiveLocalPrefix::from(&LocalPrefix::Single("app".to_string()));
        let multi = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec!["app".to_string()]));
        assert_ne!(single.cache_key(), multi.cache_key());
    }

    #[test]
    fn deserialize_string_form() {
        let toml_text = r#"local_prefix = "src""#;
        #[derive(serde::Deserialize)]
        struct Holder {
            local_prefix: LocalPrefix,
        }
        let h: Holder = toml::from_str(toml_text).unwrap();
        match h.local_prefix {
            LocalPrefix::Single(s) => assert_eq!(s, "src"),
            _ => panic!("expected Single"),
        }
    }

    #[test]
    fn deserialize_array_form() {
        let toml_text = r#"local_prefix = ["app", "lib"]"#;
        #[derive(serde::Deserialize)]
        struct Holder {
            local_prefix: LocalPrefix,
        }
        let h: Holder = toml::from_str(toml_text).unwrap();
        match h.local_prefix {
            LocalPrefix::Multi(v) => assert_eq!(v, vec!["app".to_string(), "lib".to_string()]),
            _ => panic!("expected Multi"),
        }
    }

    #[test]
    fn matches_top_segment_single_wrap_mode_always_true() {
        // In wrap mode, every discovered file gets the prefix applied,
        // so the predicate is trivially true (no "non-local" classification
        // happens at this layer).
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
        assert!(eff.matches_top_segment("src.foo"));
        assert!(eff.matches_top_segment("anything.else"));
    }

    #[test]
    fn matches_top_segment_multi_no_wrap_checks_membership() {
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));
        assert!(eff.matches_top_segment("app.foo"));
        assert!(eff.matches_top_segment("lib.bar.baz"));
        assert!(!eff.matches_top_segment("react.useState"));
        assert!(!eff.matches_top_segment("components.Button")); // not in list
    }
}
```

Add `tempfile`, `toml`, and `serde` to dev-dependencies in `crates/graphify-extract/Cargo.toml` if not already present (they likely are; verify with `grep -n 'tempfile\|^toml' crates/graphify-extract/Cargo.toml`).

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-extract local_prefix:: 2>&1 | head -40
```

Expected: compile error — module doesn't exist yet.

- [ ] **Step 3: Implement the module**

```rust
// crates/graphify-extract/src/local_prefix.rs

//! Multi-root `local_prefix` config + runtime representation.
//!
//! `LocalPrefix` is the TOML-facing form (string or array, via `serde(untagged)`).
//! `EffectiveLocalPrefix` is the collapsed runtime form passed through the
//! walker, resolver, cache, and report writers — always non-empty `prefixes`,
//! plus a `wrap` flag indicating Single (true) vs Multi (false) semantics.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LocalPrefix {
    /// Single string — current behavior. Files outside the prefix get it
    /// prepended to their module ID.
    Single(String),
    /// Array of top-level dirs treated as local roots. Files keep their
    /// natural path-as-module-ID; the array drives shadow-set, barrel
    /// exclusion, and `is-local` hints downstream.
    Multi(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct EffectiveLocalPrefix {
    /// One element when `Single`, N when `Multi`. Always non-empty (at minimum
    /// holds an empty string in `omitted()` form).
    pub prefixes: Vec<String>,
    /// `true` for `Single` (apply wrapping). `false` for `Multi` (no wrapping,
    /// paths stand as-is).
    pub wrap: bool,
}

impl EffectiveLocalPrefix {
    /// Construct the form representing an omitted `local_prefix` field —
    /// equivalent to `Single("")`. Lets callers avoid `Option<EffectiveLocalPrefix>`.
    pub fn omitted() -> Self {
        Self {
            prefixes: vec![String::new()],
            wrap: true,
        }
    }

    /// Returns the head prefix — the only one in `Single`/`omitted`, the
    /// first listed in `Multi`. Useful for legacy code paths that expect
    /// a single `&str`.
    pub fn first(&self) -> &str {
        self.prefixes
            .first()
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Returns true if this represents the array form (no-wrap mode).
    pub fn is_multi(&self) -> bool {
        !self.wrap
    }

    /// Returns the number of distinct prefixes.
    pub fn len(&self) -> usize {
        self.prefixes.len()
    }

    /// Cache invalidation key. `Single("src")` → `"src"`. `Multi([...])` →
    /// `"multi:<sorted prefixes joined by '|'>"`. Sorted so reordering the
    /// array doesn't invalidate the cache; `multi:` marker so a `Single("app")`
    /// cache never matches a `Multi(["app"])` build (different node IDs).
    pub fn cache_key(&self) -> String {
        if self.wrap {
            self.first().to_string()
        } else {
            let mut sorted = self.prefixes.clone();
            sorted.sort();
            format!("multi:{}", sorted.join("|"))
        }
    }

    /// Predicate: is this module id "local" by virtue of its top segment?
    /// In wrap mode this is trivially true (every walker-discovered file
    /// already gets the prefix applied). In no-wrap mode, returns true iff
    /// the id's first dot-segment matches one of the listed prefixes.
    pub fn matches_top_segment(&self, id: &str) -> bool {
        if self.wrap {
            return true;
        }
        let top = id.split('.').next().unwrap_or("");
        self.prefixes.iter().any(|p| p == top)
    }
}

impl From<&LocalPrefix> for EffectiveLocalPrefix {
    fn from(lp: &LocalPrefix) -> Self {
        match lp {
            LocalPrefix::Single(s) => Self {
                prefixes: vec![s.clone()],
                wrap: true,
            },
            LocalPrefix::Multi(v) => {
                let mut seen = std::collections::HashSet::new();
                let prefixes: Vec<String> = v
                    .iter()
                    .filter(|s| seen.insert((*s).clone()))
                    .cloned()
                    .collect();
                Self {
                    prefixes,
                    wrap: false,
                }
            }
        }
    }
}
```

Add to `crates/graphify-extract/src/lib.rs` (near other `pub mod` lines):

```rust
pub mod local_prefix;
pub use local_prefix::{EffectiveLocalPrefix, LocalPrefix};
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p graphify-extract local_prefix:: 2>&1 | tail -30
```

Expected: all 14 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/local_prefix.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(extract): add LocalPrefix enum + EffectiveLocalPrefix collapse (FEAT-049)"
```

---

## Task 2: Validation in `load_config` (single-element warning, empty fail-fast, dedup, PHP rejection)

**Files:**
- Modify: `crates/graphify-cli/src/main.rs:182` (field type) and `:1944-1958` (PHP warning extension)
- Add inline tests in `crates/graphify-cli/src/main.rs` mod `tests` (use existing convention)

- [ ] **Step 1: Write the failing tests**

Append to the inline `mod tests` of `crates/graphify-cli/src/main.rs` (use `grep -n '^#\[cfg(test)\]' crates/graphify-cli/src/main.rs` to find the right one — there is one near the bottom for config validation):

```rust
#[test]
fn load_config_accepts_string_form_local_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graphify.toml");
    std::fs::write(
        &path,
        r#"
[[project]]
name = "p"
repo = "./p"
lang = ["typescript"]
local_prefix = "src"
"#,
    )
    .unwrap();
    let cfg = load_config(&path);
    let lp = cfg.project[0].local_prefix.as_ref().unwrap();
    match lp {
        LocalPrefix::Single(s) => assert_eq!(s, "src"),
        _ => panic!("expected Single"),
    }
}

#[test]
fn load_config_accepts_array_form_local_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("graphify.toml");
    std::fs::write(
        &path,
        r#"
[[project]]
name = "p"
repo = "./p"
lang = ["typescript"]
local_prefix = ["app", "lib", "components"]
"#,
    )
    .unwrap();
    let cfg = load_config(&path);
    let lp = cfg.project[0].local_prefix.as_ref().unwrap();
    match lp {
        LocalPrefix::Multi(v) => {
            assert_eq!(v, &vec!["app".to_string(), "lib".to_string(), "components".to_string()]);
        }
        _ => panic!("expected Multi"),
    }
}

// validate_local_prefix is a free function called by load_config; we can test
// it directly without spinning up a full config file.
#[test]
fn validate_local_prefix_empty_array_returns_err() {
    let lp = LocalPrefix::Multi(Vec::new());
    let err = validate_local_prefix("p", &Some(lp), &["typescript".to_string()]).unwrap_err();
    assert!(err.contains("empty"), "err was: {err}");
    assert!(err.contains("'p'"), "err was: {err}");
}

#[test]
fn validate_local_prefix_single_element_array_returns_warning() {
    let lp = LocalPrefix::Multi(vec!["src".to_string()]);
    let result = validate_local_prefix("p", &Some(lp), &["typescript".to_string()]);
    let warning = result.unwrap();
    assert!(warning.is_some(), "expected a warning");
    let w = warning.unwrap();
    assert!(w.contains("single-element"), "warning was: {w}");
    assert!(w.contains("local_prefix = \"src\""), "warning was: {w}");
}

#[test]
fn validate_local_prefix_multi_dupes_emit_warning() {
    let lp = LocalPrefix::Multi(vec!["app".to_string(), "lib".to_string(), "app".to_string()]);
    let result = validate_local_prefix("p", &Some(lp), &["typescript".to_string()]);
    let warning = result.unwrap();
    assert!(warning.is_some(), "expected a dupe warning");
    let w = warning.unwrap();
    assert!(w.contains("duplicate"), "warning was: {w}");
}

#[test]
fn validate_local_prefix_php_rejects_array_form() {
    let lp = LocalPrefix::Multi(vec!["app".to_string()]);
    let err = validate_local_prefix("p", &Some(lp), &["php".to_string()]).unwrap_err();
    assert!(err.contains("PHP"), "err was: {err}");
    assert!(err.contains("PSR-4"), "err was: {err}");
}

#[test]
fn validate_local_prefix_string_form_no_warning() {
    let lp = LocalPrefix::Single("src".to_string());
    let result = validate_local_prefix("p", &Some(lp), &["typescript".to_string()]);
    let warning = result.unwrap();
    assert!(warning.is_none());
}

#[test]
fn validate_local_prefix_omitted_no_warning() {
    let result = validate_local_prefix("p", &None, &["typescript".to_string()]);
    let warning = result.unwrap();
    assert!(warning.is_none());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-cli load_config_accepts_string\|load_config_accepts_array\|validate_local_prefix 2>&1 | tail -20
```

Expected: compile error — `LocalPrefix` not in scope, `validate_local_prefix` not defined, `ProjectConfig.local_prefix` still `Option<String>`.

- [ ] **Step 3: Wire the type change + add `validate_local_prefix`**

Edit `crates/graphify-cli/src/main.rs:182`:

```rust
    local_prefix: Option<String>,
```

becomes:

```rust
    local_prefix: Option<graphify_extract::LocalPrefix>,
```

Add a `use` import near the top of the file (search for the existing `use graphify_extract::` block):

```rust
use graphify_extract::{EffectiveLocalPrefix, LocalPrefix};
```

Add the validator function near `load_config` (above its definition):

```rust
/// Validate a project's `local_prefix` value. Returns `Ok(Some(warning))` when
/// the value is legal but suspect (single-element array, dupes), `Ok(None)`
/// when fully clean, and `Err(message)` on a fail-fast condition (empty array,
/// `Multi` on PHP).
fn validate_local_prefix(
    project_name: &str,
    lp: &Option<LocalPrefix>,
    languages: &[String],
) -> Result<Option<String>, String> {
    let Some(lp) = lp else {
        return Ok(None);
    };

    let is_php_only = !languages.is_empty()
        && languages.iter().all(|l| l.eq_ignore_ascii_case("php"));

    match lp {
        LocalPrefix::Single(_) => Ok(None),
        LocalPrefix::Multi(v) if v.is_empty() => Err(format!(
            "Project '{project_name}' has an empty local_prefix array. \
             Either remove the field or list at least one root directory."
        )),
        LocalPrefix::Multi(_) if is_php_only => Err(format!(
            "Project '{project_name}' is PHP and uses a local_prefix array. \
             PHP projects derive prefixes from PSR-4 in composer.json — remove \
             local_prefix entirely."
        )),
        LocalPrefix::Multi(v) if v.len() == 1 => Ok(Some(format!(
            "Project '{project_name}' uses a single-element local_prefix array. \
             Prefer the string form: local_prefix = \"{}\". \
             The array form skips wrapping; the string form preserves the legacy \
             namespace prefix.",
            v[0]
        ))),
        LocalPrefix::Multi(v) => {
            let mut seen = std::collections::HashSet::new();
            let dupes: Vec<&str> = v
                .iter()
                .filter(|s| !seen.insert((*s).clone()))
                .map(|s| s.as_str())
                .collect();
            if !dupes.is_empty() {
                Ok(Some(format!(
                    "Project '{project_name}' has duplicate local_prefix entries: {}. \
                     Dedup'd silently; consider cleaning up graphify.toml.",
                    dupes.join(", ")
                )))
            } else {
                Ok(None)
            }
        }
    }
}
```

Then patch `load_config` (around `crates/graphify-cli/src/main.rs:1944-1958`) — replace the existing PHP warning loop with a single validation pass that calls `validate_local_prefix`:

```rust
    for project in &cfg.project {
        match validate_local_prefix(&project.name, &project.local_prefix, &project.lang) {
            Err(msg) => {
                eprintln!("Invalid config {:?}: {msg}", path);
                std::process::exit(1);
            }
            Ok(Some(warn)) => eprintln!("Warning: {warn}"),
            Ok(None) => {}
        }

        // Existing DOC-002 PHP warning for the legacy string form on PHP.
        let is_php = project.lang.iter().any(|l| l.eq_ignore_ascii_case("php"));
        let has_string_prefix = matches!(
            &project.local_prefix,
            Some(LocalPrefix::Single(p)) if !p.is_empty()
        );
        if is_php && has_string_prefix {
            eprintln!(
                "Warning: project '{}' sets local_prefix for a PHP project — \
                 PSR-4 mappings from composer.json should be used instead. \
                 Consider removing local_prefix.",
                project.name
            );
        }
    }
```

Now the compiler will yell about every site that reads `.local_prefix.as_deref()`. **Don't fix them yet** — that's Tasks 3-7. To unblock the build, add a temporary `Project::effective_local_prefix()` method and switch the legacy call sites to call `.first()` to keep their existing single-string semantics:

Search and replace in `crates/graphify-cli/src/main.rs` — every `project.local_prefix.as_deref()` pattern. The existing two call sites (lines ~2165 and ~2377) use the same shape; a temporary helper makes the migration cleaner. Add this near the bottom of the file:

```rust
impl ProjectConfig {
    fn effective_local_prefix(&self) -> EffectiveLocalPrefix {
        self.local_prefix
            .as_ref()
            .map(EffectiveLocalPrefix::from)
            .unwrap_or_else(EffectiveLocalPrefix::omitted)
    }
}
```

For the suggest-stubs site (around `crates/graphify-cli/src/main.rs:5496`), change:

```rust
        let local_prefix = project
            .local_prefix
            .clone()
            .unwrap_or_else(|| "src".to_string());
```

to (keep behavior — single prefix only feeds `suggest stubs` for now; Task 8 generalizes):

```rust
        let effective = project.effective_local_prefix();
        let local_prefix = if effective.is_multi() {
            // Multi-prefix shadowing handled in Task 8.
            effective.first().to_string()
        } else {
            // Preserve legacy default: when omitted, suggest stubs assumed "src".
            if effective.first().is_empty() {
                "src".to_string()
            } else {
                effective.first().to_string()
            }
        };
```

For the two `run_extract*` sites (around `:2165` and `:2377`), change:

```rust
    let (effective_local_prefix, _auto) = match project.local_prefix.as_deref() {
        Some(prefix) => (prefix.to_owned(), false),
        None => (
            detect_local_prefix(&repo_path, &languages, &extra_excludes),
            true,
        ),
    };
```

to (still passing a `String` to existing walker/resolver — Task 3-5 changes those signatures):

```rust
    let (effective_local_prefix, _auto) = match &project.local_prefix {
        Some(lp) => (EffectiveLocalPrefix::from(lp).first().to_string(), false),
        None => (
            detect_local_prefix(&repo_path, &languages, &extra_excludes),
            true,
        ),
    };
```

The same shape for the second call site (which also uses `auto_detected`).

For `barrel_exclusion_ids` at `:3210`:

```rust
    let prefix = match project.local_prefix.as_deref() {
        Some(p) if !p.is_empty() => p,
        _ => return Vec::new(),
    };
```

becomes (still single-prefix; Task 8 adds Multi support):

```rust
    let effective = project.effective_local_prefix();
    let prefix = if effective.is_multi() {
        // Multi-mode barrel exclusion handled in Task 8 — fall through for now.
        return Vec::new();
    } else if effective.first().is_empty() {
        return Vec::new();
    } else {
        effective.first()
    };
```

Note: the function returns `Vec<&'a str>` borrowing from `project`. With the new path, the borrow comes from the `Project::local_prefix` Single variant. To preserve the lifetime, change the match to extract the `&str` directly:

```rust
fn barrel_exclusion_ids<'a>(
    project: &'a ProjectConfig,
    consolidation: &ConsolidationConfig,
) -> Vec<&'a str> {
    if !consolidation.suppress_barrel_cycles() {
        return Vec::new();
    }
    let prefix: &'a str = match &project.local_prefix {
        Some(LocalPrefix::Single(p)) if !p.is_empty() => p.as_str(),
        // Multi handled in Task 8; for now, no barrel suppression in array mode.
        _ => return Vec::new(),
    };
    if consolidation.matches(prefix) {
        vec![prefix]
    } else {
        Vec::new()
    }
}
```

- [ ] **Step 4: Run the tests + full workspace build**

```bash
cargo test -p graphify-cli load_config_accepts_string\|load_config_accepts_array\|validate_local_prefix 2>&1 | tail -20
cargo build --workspace 2>&1 | tail -10
```

Expected: 7 new tests pass; full build green.

Also confirm existing tests still pass:

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: all pre-existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat(cli): wire LocalPrefix enum + validate_local_prefix into load_config (FEAT-049)"
```

---

## Task 3: Walker accepts `&EffectiveLocalPrefix`

**Files:**
- Modify: `crates/graphify-extract/src/walker.rs:108-180` (`path_to_module`, `path_to_go_package`)
- Modify: `crates/graphify-extract/src/walker.rs:187-227` (`path_to_module_psr4`)
- Modify: `crates/graphify-extract/src/walker.rs:237-?` (`discover_files`, `discover_files_with_psr4`, `walk_dir`)
- Update inline tests + downstream callers (see Task 6 for caller migration)

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` in `crates/graphify-extract/src/walker.rs`:

```rust
#[test]
fn path_to_module_no_wrap_keeps_natural_id() {
    use crate::EffectiveLocalPrefix;
    use crate::LocalPrefix;
    let base = std::path::Path::new("/repo");
    let file = std::path::Path::new("/repo/lib/util.ts");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "app".to_string(),
        "lib".to_string(),
    ]));
    assert_eq!(path_to_module_eff(base, file, &eff), "lib.util");
}

#[test]
fn path_to_module_no_wrap_unmatched_root_still_no_wrap() {
    // Multi-mode does NOT filter out non-matching files — the walker
    // discovers everything; the array is purely a naming hint.
    use crate::EffectiveLocalPrefix;
    use crate::LocalPrefix;
    let base = std::path::Path::new("/repo");
    let file = std::path::Path::new("/repo/scripts/build.ts");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "app".to_string(),
        "lib".to_string(),
    ]));
    // `scripts` is not in the Multi list, but the file is still discovered —
    // its module id stays natural.
    assert_eq!(path_to_module_eff(base, file, &eff), "scripts.build");
}

#[test]
fn path_to_module_wrap_uses_single_prefix_unchanged() {
    use crate::EffectiveLocalPrefix;
    use crate::LocalPrefix;
    let base = std::path::Path::new("/repo");
    let file = std::path::Path::new("/repo/lib/util.ts");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
    // Mirrors current behavior — `lib/util.ts` under `local_prefix = "src"`
    // becomes `src.lib.util`.
    assert_eq!(path_to_module_eff(base, file, &eff), "src.lib.util");
}

#[test]
fn path_to_module_wrap_idempotent_on_already_prefixed() {
    use crate::EffectiveLocalPrefix;
    use crate::LocalPrefix;
    let base = std::path::Path::new("/repo");
    let file = std::path::Path::new("/repo/src/foo.ts");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
    assert_eq!(path_to_module_eff(base, file, &eff), "src.foo");
}

#[test]
fn path_to_module_omitted_returns_root_relative() {
    use crate::EffectiveLocalPrefix;
    let base = std::path::Path::new("/repo");
    let file = std::path::Path::new("/repo/foo/bar.ts");
    let eff = EffectiveLocalPrefix::omitted();
    assert_eq!(path_to_module_eff(base, file, &eff), "foo.bar");
}

#[test]
fn path_to_go_package_no_wrap_keeps_natural_id() {
    use crate::EffectiveLocalPrefix;
    use crate::LocalPrefix;
    let base = std::path::Path::new("/repo");
    let file = std::path::Path::new("/repo/cmd/server/main.go");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec!["cmd".to_string()]));
    // Multi mode: no wrapping. Package path = `cmd.server`.
    assert_eq!(path_to_module_eff(base, file, &eff), "cmd.server");
}

#[test]
fn discover_files_eff_returns_same_set_in_either_mode() {
    use crate::{EffectiveLocalPrefix, LocalPrefix, Language};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::create_dir_all(root.join("lib")).unwrap();
    std::fs::write(root.join("app/index.ts"), "").unwrap();
    std::fs::write(root.join("lib/util.ts"), "").unwrap();

    let single = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
    let multi = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "app".to_string(),
        "lib".to_string(),
    ]));

    let langs = vec![Language::TypeScript];
    let files_single = discover_files_eff(root, &langs, &single, &[]);
    let files_multi = discover_files_eff(root, &langs, &multi, &[]);

    // Both modes discover the same files (just with different module IDs).
    assert_eq!(files_single.len(), 2);
    assert_eq!(files_multi.len(), 2);

    let multi_ids: Vec<String> = files_multi.iter().map(|f| f.module_name.clone()).collect();
    assert!(multi_ids.contains(&"app".to_string())); // app/index.ts → "app"
    assert!(multi_ids.contains(&"lib.util".to_string()));

    let single_ids: Vec<String> = files_single.iter().map(|f| f.module_name.clone()).collect();
    assert!(single_ids.contains(&"src.app".to_string()));
    assert!(single_ids.contains(&"src.lib.util".to_string()));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-extract walker:: 2>&1 | tail -20
```

Expected: compile errors — `path_to_module_eff` and `discover_files_eff` not defined.

- [ ] **Step 3: Add the new entry points**

Add to `crates/graphify-extract/src/walker.rs` near the existing `path_to_module`:

```rust
/// `EffectiveLocalPrefix`-aware variant of [`path_to_module`].
///
/// In wrap mode (`Single`), behaves identically to the legacy `path_to_module`.
/// In no-wrap mode (`Multi`), returns the module id from the relative path
/// without any prepended prefix.
pub fn path_to_module_eff(
    base: &Path,
    file: &Path,
    prefix: &crate::EffectiveLocalPrefix,
) -> String {
    if prefix.wrap {
        path_to_module(base, file, prefix.first())
    } else {
        // No-wrap: pass an empty prefix so existing logic skips prepending.
        path_to_module(base, file, "")
    }
}

/// `EffectiveLocalPrefix`-aware variant of [`discover_files`].
pub fn discover_files_eff(
    root: &Path,
    languages: &[Language],
    prefix: &crate::EffectiveLocalPrefix,
    extra_excludes: &[&str],
) -> Vec<DiscoveredFile> {
    discover_files_eff_with_psr4(root, languages, prefix, extra_excludes, &[])
}

/// `EffectiveLocalPrefix`-aware variant of [`discover_files_with_psr4`].
pub fn discover_files_eff_with_psr4(
    root: &Path,
    languages: &[Language],
    prefix: &crate::EffectiveLocalPrefix,
    extra_excludes: &[&str],
    psr4_mappings: &[(String, String)],
) -> Vec<DiscoveredFile> {
    let legacy_prefix = if prefix.wrap { prefix.first() } else { "" };
    discover_files_with_psr4(root, languages, legacy_prefix, extra_excludes, psr4_mappings)
}
```

Re-export from `crates/graphify-extract/src/lib.rs` — add to the existing `pub use ... walker::{...}` block:

```rust
    walker::{
        detect_local_prefix, discover_files, discover_files_with_psr4,
        discover_files_eff, discover_files_eff_with_psr4,
        path_to_module, path_to_module_eff, path_to_module_psr4,
        // ... (preserve existing exports)
    },
```

(Find the existing `walker::{...}` re-export and append `path_to_module_eff`, `discover_files_eff`, `discover_files_eff_with_psr4` to the list.)

Why this shape? It keeps the legacy `path_to_module(base, file, &str)` and `discover_files(root, langs, &str, &[&str])` as-is (no breaking change for any external consumer of `graphify-extract`), and adds parallel `_eff` entry points for the new code path. Task 6 migrates the CLI's `run_extract*` to call `_eff` versions.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p graphify-extract walker:: 2>&1 | tail -30
```

Expected: 7 new tests pass; existing walker tests still green.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/walker.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(extract): walker accepts EffectiveLocalPrefix via _eff entry points (FEAT-049)"
```

---

## Task 4: Resolver accepts `&[String]` + `wrap_mode`

**Files:**
- Modify: `crates/graphify-extract/src/resolver.rs:121` (struct field), `:149` (constructor), `:161-178` (`set_local_prefix`, `apply_local_prefix`)
- Update existing inline tests that call `r.set_local_prefix("src")` — no signature change is needed because we'll keep the old method as a wrapper.

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` in `crates/graphify-extract/src/resolver.rs`:

```rust
#[test]
fn set_local_prefixes_single_acts_like_set_local_prefix() {
    let mut r = ModuleResolver::new(std::path::Path::new("/repo"));
    r.set_local_prefixes(&["src".to_string()], true);
    // apply_local_prefix should prepend "src." to bare ids — wrap mode active.
    assert_eq!(r.apply_local_prefix_for_test("foo"), "src.foo");
    assert_eq!(r.apply_local_prefix_for_test("src.foo"), "src.foo");
}

#[test]
fn set_local_prefixes_multi_no_wrap_does_not_prepend() {
    let mut r = ModuleResolver::new(std::path::Path::new("/repo"));
    r.set_local_prefixes(
        &["app".to_string(), "lib".to_string()],
        false,
    );
    // No-wrap mode: apply_local_prefix is a pass-through.
    assert_eq!(r.apply_local_prefix_for_test("foo"), "foo");
    assert_eq!(r.apply_local_prefix_for_test("lib.bar"), "lib.bar");
}

#[test]
fn legacy_set_local_prefix_still_works() {
    let mut r = ModuleResolver::new(std::path::Path::new("/repo"));
    r.set_local_prefix("src");
    assert_eq!(r.apply_local_prefix_for_test("foo"), "src.foo");
}
```

You'll also need to expose a test-only accessor — at the bottom of `mod tests` (or behind `#[cfg(test)] impl`):

```rust
#[cfg(test)]
impl ModuleResolver {
    fn apply_local_prefix_for_test(&self, id: &str) -> String {
        self.apply_local_prefix(id)
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-extract resolver::tests::set_local_prefixes\|legacy_set_local_prefix 2>&1 | tail -20
```

Expected: compile error — `set_local_prefixes` doesn't exist; `apply_local_prefix_for_test` doesn't exist.

- [ ] **Step 3: Implement the multi-prefix method**

Edit `crates/graphify-extract/src/resolver.rs`. Change the struct field at `:121`:

```rust
    local_prefix: String,
```

becomes:

```rust
    local_prefix: String,
    /// `true` when `set_local_prefix*` was called with wrap-mode semantics
    /// (Single TOML form). `false` for `Multi` array mode — `apply_local_prefix`
    /// becomes a no-op so paths stay in their natural form.
    wrap_mode: bool,
    /// Full list of prefixes (Multi mode). Length 1 in wrap mode, length 0
    /// when never set, length N for Multi. Used by callers that need to
    /// know all roots (e.g. consumer-side `is_local_module` future hooks).
    /// In wrap mode, equals `vec![local_prefix.clone()]`.
    local_prefixes: Vec<String>,
```

Update the constructor at `:149`:

```rust
            local_prefix: String::new(),
```

add right after:

```rust
            wrap_mode: true,
            local_prefixes: Vec::new(),
```

Replace the existing `set_local_prefix` at `:161` with both methods (legacy preserved as a wrapper):

```rust
    /// Legacy wrapper: equivalent to `set_local_prefixes(&[prefix], true)`.
    /// Preserved so existing callers don't need migration in lockstep.
    pub fn set_local_prefix(&mut self, prefix: &str) {
        if prefix.is_empty() {
            self.local_prefix.clear();
            self.local_prefixes.clear();
        } else {
            self.local_prefix = prefix.to_owned();
            self.local_prefixes = vec![prefix.to_owned()];
        }
        self.wrap_mode = true;
    }

    /// Multi-prefix-aware setter.
    ///
    /// `wrap = true` (Single TOML form): `prefixes` must contain exactly one
    /// non-empty entry; `apply_local_prefix` will prepend it to bare ids.
    ///
    /// `wrap = false` (Multi TOML form): `prefixes` may contain N entries;
    /// `apply_local_prefix` becomes a no-op (paths stay in natural form).
    /// `local_prefix` (the legacy field) is set to the FIRST entry for
    /// compatibility with code paths that still read it directly during the
    /// migration window — but those paths should not affect resolution in
    /// no-wrap mode.
    pub fn set_local_prefixes(&mut self, prefixes: &[String], wrap: bool) {
        self.wrap_mode = wrap;
        self.local_prefixes = prefixes.to_vec();
        self.local_prefix = prefixes
            .first()
            .cloned()
            .unwrap_or_default();
    }
```

Replace `apply_local_prefix` at `:168`:

```rust
    fn apply_local_prefix(&self, id: &str) -> String {
        if !self.wrap_mode {
            // Multi mode: paths are already in natural form — no-op.
            return id.to_owned();
        }
        if self.local_prefix.is_empty() {
            return id.to_owned();
        }
        if id.is_empty() {
            return self.local_prefix.clone();
        }
        if id == self.local_prefix || id.starts_with(&format!("{}.", self.local_prefix)) {
            return id.to_owned();
        }
        format!("{}.{}", self.local_prefix, id)
    }
```

(The body is the same as today, just guarded by the `wrap_mode` early return.)

- [ ] **Step 4: Run the tests**

```bash
cargo test -p graphify-extract resolver:: 2>&1 | tail -30
```

Expected: 3 new tests pass; existing resolver tests still green (the 2287/2306/2328 etc. all use `set_local_prefix("src")` which still works via the legacy wrapper).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/resolver.rs
git commit -m "feat(extract): resolver gains set_local_prefixes(&[String], wrap_mode) (FEAT-049)"
```

---

## Task 5: ExtractionCache key derived from `EffectiveLocalPrefix`

**Files:**
- Modify: `crates/graphify-extract/src/cache.rs:38-49` (struct field, constructor) and `:74-89` (load), `:96-117` (save)

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `crates/graphify-extract/src/cache.rs`:

```rust
#[test]
fn cache_key_single_string_preserves_legacy_format() {
    use crate::{EffectiveLocalPrefix, LocalPrefix};
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("app".to_string()));
    let cache = ExtractionCache::new_eff(&eff);
    // Key on disk should be just "app" — same as legacy ExtractionCache::new("app")
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    cache.save(&path);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"local_prefix\": \"app\""), "raw: {raw}");
}

#[test]
fn cache_key_multi_uses_marker_format() {
    use crate::{EffectiveLocalPrefix, LocalPrefix};
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "lib".to_string(),
        "app".to_string(),
    ]));
    let cache = ExtractionCache::new_eff(&eff);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    cache.save(&path);
    let raw = std::fs::read_to_string(&path).unwrap();
    // Sorted: app|lib. Marker: multi:
    assert!(
        raw.contains("\"local_prefix\": \"multi:app|lib\""),
        "raw: {raw}"
    );
}

#[test]
fn cache_load_eff_round_trip_single() {
    use crate::{EffectiveLocalPrefix, LocalPrefix};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
    let mut cache = ExtractionCache::new_eff(&eff);
    cache.insert("a.py".to_string(), "h1".to_string(), make_result());
    cache.save(&path);

    let loaded = ExtractionCache::load_eff(&path, &eff).unwrap();
    assert!(loaded.lookup("a.py", "h1").is_some());
}

#[test]
fn cache_load_eff_round_trip_multi() {
    use crate::{EffectiveLocalPrefix, LocalPrefix};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "app".to_string(),
        "lib".to_string(),
    ]));
    let mut cache = ExtractionCache::new_eff(&eff);
    cache.insert("a.ts".to_string(), "h1".to_string(), make_result());
    cache.save(&path);

    let loaded = ExtractionCache::load_eff(&path, &eff).unwrap();
    assert!(loaded.lookup("a.ts", "h1").is_some());
}

#[test]
fn cache_load_eff_invalidates_when_switching_string_to_multi() {
    use crate::{EffectiveLocalPrefix, LocalPrefix};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");

    let single = EffectiveLocalPrefix::from(&LocalPrefix::Single("app".to_string()));
    let multi = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec!["app".to_string()]));

    let mut cache = ExtractionCache::new_eff(&single);
    cache.insert("a.ts".to_string(), "h1".to_string(), make_result());
    cache.save(&path);

    // Loading with Multi(["app"]) must miss — different node IDs land in the graph.
    assert!(ExtractionCache::load_eff(&path, &multi).is_none());
}

#[test]
fn cache_load_eff_stable_under_multi_reorder() {
    use crate::{EffectiveLocalPrefix, LocalPrefix};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");

    let a = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "app".to_string(),
        "lib".to_string(),
    ]));
    let b = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
        "lib".to_string(),
        "app".to_string(),
    ]));

    let mut cache = ExtractionCache::new_eff(&a);
    cache.insert("x.ts".to_string(), "h1".to_string(), make_result());
    cache.save(&path);

    // Reordering must not invalidate.
    assert!(ExtractionCache::load_eff(&path, &b).is_some());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-extract cache::tests::cache_key\|cache_load_eff 2>&1 | tail -20
```

Expected: compile errors — `new_eff` and `load_eff` don't exist.

- [ ] **Step 3: Implement the new entry points**

Add to `crates/graphify-extract/src/cache.rs` (alongside existing `new`/`load`):

```rust
    /// `EffectiveLocalPrefix`-aware constructor — preferred over [`Self::new`]
    /// for FEAT-049 multi-root support. The cache file's `local_prefix` field
    /// holds `prefix.cache_key()`, which is `"multi:<sorted prefixes>"` for
    /// array mode and bare prefix for string mode.
    pub fn new_eff(prefix: &crate::EffectiveLocalPrefix) -> Self {
        Self {
            local_prefix: prefix.cache_key(),
            entries: HashMap::new(),
        }
    }

    /// `EffectiveLocalPrefix`-aware loader. Returns `None` on any of the
    /// existing miss conditions plus when the on-disk `local_prefix` field
    /// doesn't match `prefix.cache_key()`.
    pub fn load_eff(path: &Path, prefix: &crate::EffectiveLocalPrefix) -> Option<Self> {
        Self::load(path, &prefix.cache_key())
    }
```

The legacy `new`/`load` stay untouched.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p graphify-extract cache:: 2>&1 | tail -20
```

Expected: 6 new tests pass; legacy tests still green.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/cache.rs
git commit -m "feat(extract): ExtractionCache::{new,load}_eff for multi-prefix cache keys (FEAT-049)"
```

---

## Task 6: CLI plumbing — switch `run_extract*` and helpers to `_eff` entry points

**Files:**
- Modify: `crates/graphify-cli/src/main.rs:2155-2230` (`build_project_reexport_context` — first `run_extract*` shape)
- Modify: `crates/graphify-cli/src/main.rs:2370-2470` (`run_extract_with_workspace`)

- [ ] **Step 1: Inspect the call sites**

Run `grep -n "discover_files\|discover_files_with_psr4\|set_local_prefix\b\|ExtractionCache::new\|ExtractionCache::load" crates/graphify-cli/src/main.rs` to map all sites that need migration.

- [ ] **Step 2: Migrate call sites**

In `build_project_reexport_context` (around `:2155-2230`):

Replace:

```rust
    let (effective_local_prefix, _auto) = match &project.local_prefix {
        Some(lp) => (EffectiveLocalPrefix::from(lp).first().to_string(), false),
        None => (
            detect_local_prefix(&repo_path, &languages, &extra_excludes),
            true,
        ),
    };
```

with:

```rust
    let effective: EffectiveLocalPrefix = match &project.local_prefix {
        Some(lp) => EffectiveLocalPrefix::from(lp),
        None => {
            let auto = detect_local_prefix(&repo_path, &languages, &extra_excludes);
            EffectiveLocalPrefix::from(&LocalPrefix::Single(auto))
        }
    };
```

Replace the `discover_files_with_psr4(... &effective_local_prefix ...)` call with:

```rust
    let files = graphify_extract::discover_files_eff_with_psr4(
        &repo_path,
        &languages,
        &effective,
        &extra_excludes,
        &psr4_mappings,
    );
```

Replace the cache load:

```rust
    let cache = match (force, cache_dir) {
        (false, Some(dir)) => {
            let cache_path = dir.join(".graphify-cache.json");
            ExtractionCache::load_eff(&cache_path, &effective)
                .unwrap_or_else(|| ExtractionCache::new_eff(&effective))
        }
        _ => ExtractionCache::new_eff(&effective),
    };
```

Replace the resolver wiring:

```rust
    let mut resolver = graphify_extract::resolver::ModuleResolver::new(&repo_path);
    resolver.set_local_prefixes(&effective.prefixes, effective.wrap);
```

Apply the same set of changes to `run_extract_with_workspace` (around `:2370-2470`). Preserve the auto-detect log line — adapt it:

```rust
    if matches!(&project.local_prefix, None) {
        let shown_prefix = if effective.first().is_empty() {
            "(root-level)"
        } else {
            effective.first()
        };
        eprintln!(
            "[{}] Auto-detected local_prefix: {}",
            project.name, shown_prefix
        );
    }
```

Also update the `files.len() <= 1` warning to print the effective prefix (the `cache_key()` form is the most informative since it captures Multi too):

```rust
    if files.len() <= 1 {
        eprintln!(
            "Warning: project '{}' discovered only {} file(s). Check repo path ('{}') and local_prefix ('{}') configuration.",
            project.name,
            files.len(),
            project.repo,
            effective.cache_key(),
        );
    }
```

Also pass `EffectiveLocalPrefix` into the `path_to_module` call inside the `register_module_path` loop. Search the file for `path_to_module(` to confirm — if any call site outside the walker still uses the legacy form with `&str`, replace with `path_to_module_eff(... &effective ...)`.

- [ ] **Step 3: Build + run all tests**

```bash
cargo build --workspace 2>&1 | tail -10
cargo test --workspace 2>&1 | tail -20
```

Expected: clean build; all tests pass. Existing behavior on string-form configs is unchanged because `EffectiveLocalPrefix::from(&LocalPrefix::Single("src"))` produces `wrap=true, prefixes=["src"]`, which makes `discover_files_eff` route through the legacy `discover_files_with_psr4("src", ...)`.

- [ ] **Step 4: Sanity-check on the dogfood config**

```bash
cargo run -p graphify-cli --release -- run --config graphify.toml 2>&1 | tail -20
```

Expected: same output as before (5 projects, all wrap-mode `src` prefix). Verify `report/<crate>/analysis.json` files have no diff:

```bash
git status report/ 2>&1 | head -5
```

Expected: clean (report/ is gitignored, but analysis.json content shouldn't shift).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "refactor(cli): plumb EffectiveLocalPrefix through run_extract* (FEAT-049)"
```

---

## Task 7: MCP server mirror

**Files:**
- Modify: `crates/graphify-mcp/src/main.rs:57` (field type), `:202-226` (run_extract equivalent)

- [ ] **Step 1: Apply the same shape as Task 6**

`crates/graphify-mcp/src/main.rs:57`:

```rust
    local_prefix: Option<String>,
```

becomes:

```rust
    local_prefix: Option<graphify_extract::LocalPrefix>,
```

Add the import:

```rust
use graphify_extract::{EffectiveLocalPrefix, LocalPrefix};
```

Replace `:202-226` block:

```rust
    let local_prefix = project.local_prefix.as_deref().unwrap_or("");
    // ...
    let files = discover_files(&repo_path, &languages, local_prefix, &extra_excludes);
    // ...
    resolver.set_local_prefix(local_prefix);
```

with:

```rust
    let effective: EffectiveLocalPrefix = project
        .local_prefix
        .as_ref()
        .map(EffectiveLocalPrefix::from)
        .unwrap_or_else(EffectiveLocalPrefix::omitted);
    // ...
    let files = graphify_extract::discover_files_eff(
        &repo_path,
        &languages,
        &effective,
        &extra_excludes,
    );
    // ...
    resolver.set_local_prefixes(&effective.prefixes, effective.wrap);
```

Update the `discovered only N` warning (around `:211-215`) to use `effective.cache_key()` like in Task 6.

- [ ] **Step 2: Build the MCP crate**

```bash
cargo build -p graphify-mcp 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 3: Smoke test**

The MCP server isn't easily testable end-to-end without a Claude session attached. Verify the binary starts at minimum:

```bash
cargo run -p graphify-mcp --release -- --config graphify.toml --help 2>&1 | head -10
```

(Adjust flag if `--help` isn't accepted — the goal is only to confirm startup parses the multi-root config without crashing.)

Run with a test config that has a Multi prefix:

```bash
cat > /tmp/feat-049-mcp-test.toml <<'EOF'
[settings]
output = "/tmp/feat-049-mcp-out"

[[project]]
name = "graphify-core"
repo = "./crates/graphify-core"
lang = ["rust"]
local_prefix = ["src"]
EOF
cargo run -p graphify-mcp --release -- --config /tmp/feat-049-mcp-test.toml 2>&1 | head -5
```

Expected: stderr shows the single-element-array warning ("Use string form local_prefix = \"src\""), then continues to load. Kill with Ctrl-C — the goal is to verify the warning fires + the server doesn't crash on the new config shape.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-mcp/src/main.rs
git commit -m "feat(mcp): mirror multi-prefix wiring from cli (FEAT-049)"
```

---

## Task 8: `barrel_exclusion_ids` + `SuggestProject` multi-prefix support

**Files:**
- Modify: `crates/graphify-cli/src/main.rs:3203-3219` (`barrel_exclusion_ids`)
- Modify: `crates/graphify-cli/src/main.rs:5450-5531` (`SuggestProject` construction in `cmd_suggest_stubs`)
- Modify: `crates/graphify-report/src/suggest.rs:38-48` (`ProjectInput`), `:160-200` (`score_stubs` shadow set)

- [ ] **Step 1: Write the failing tests**

In `crates/graphify-cli/src/main.rs` `mod tests`:

```rust
#[test]
fn barrel_exclusion_array_mode_returns_all_matching_prefixes() {
    use graphify_extract::LocalPrefix;
    // Build a project with a Multi prefix where two of three roots are in
    // the consolidation allowlist.
    let project = ProjectConfig {
        name: "mobile".to_string(),
        repo: "./apps/mobile".to_string(),
        lang: vec!["typescript".to_string()],
        local_prefix: Some(LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
            "components".to_string(),
        ])),
        external_stubs: Vec::new(),
        check: None,
    };
    let raw = ConsolidationConfigRaw {
        allowlist: vec!["^(app|lib)$".to_string()], // matches app, lib — not components
        intentional_mirrors: Vec::new(),
        suppress_barrel_cycles: true,
    };
    let consolidation = ConsolidationConfig::compile(raw).unwrap();
    let excluded = barrel_exclusion_ids(&project, &consolidation);
    let set: std::collections::HashSet<&str> = excluded.into_iter().collect();
    assert!(set.contains("app"));
    assert!(set.contains("lib"));
    assert!(!set.contains("components"));
}

#[test]
fn barrel_exclusion_single_mode_unchanged() {
    use graphify_extract::LocalPrefix;
    let project = ProjectConfig {
        name: "p".to_string(),
        repo: "./p".to_string(),
        lang: vec!["typescript".to_string()],
        local_prefix: Some(LocalPrefix::Single("src".to_string())),
        external_stubs: Vec::new(),
        check: None,
    };
    let raw = ConsolidationConfigRaw {
        allowlist: vec!["^src$".to_string()],
        intentional_mirrors: Vec::new(),
        suppress_barrel_cycles: true,
    };
    let consolidation = ConsolidationConfig::compile(raw).unwrap();
    let excluded = barrel_exclusion_ids(&project, &consolidation);
    assert_eq!(excluded, vec!["src"]);
}

#[test]
fn barrel_exclusion_disabled_when_no_suppress() {
    use graphify_extract::LocalPrefix;
    let project = ProjectConfig {
        name: "p".to_string(),
        repo: "./p".to_string(),
        lang: vec!["typescript".to_string()],
        local_prefix: Some(LocalPrefix::Multi(vec!["app".to_string()])),
        external_stubs: Vec::new(),
        check: None,
    };
    let raw = ConsolidationConfigRaw {
        allowlist: vec!["^app$".to_string()],
        intentional_mirrors: Vec::new(),
        suppress_barrel_cycles: false,
    };
    let consolidation = ConsolidationConfig::compile(raw).unwrap();
    assert!(barrel_exclusion_ids(&project, &consolidation).is_empty());
}
```

In `crates/graphify-report/src/suggest.rs` `mod tests`:

```rust
#[test]
fn score_stubs_skips_all_multi_prefixes_in_shadow_set() {
    // Multi-root project: app, lib, components are all local. None of them
    // should be suggested as external_stub even if they appear in nodes
    // of OTHER projects (which they do here).
    let nodes_other = vec![
        Node {
            id: "lib.foo".to_string(),
            kind: "module".to_string(),
            file_path: None,
            language: None,
            line: None,
            is_local: false,
        },
        Node {
            id: "app.bar".to_string(),
            kind: "module".to_string(),
            file_path: None,
            language: None,
            line: None,
            is_local: false,
        },
    ];
    let inputs = vec![
        ProjectInput {
            name: "mobile",
            local_prefixes: &["app", "lib", "components"],
            current_stubs: &ExternalStubs::new(std::iter::empty::<String>()),
            graph: &GraphSnapshot {
                nodes: vec![],
                links: vec![],
            },
        },
        ProjectInput {
            name: "other",
            local_prefixes: &["src"],
            current_stubs: &ExternalStubs::new(std::iter::empty::<String>()),
            graph: &GraphSnapshot {
                nodes: nodes_other,
                links: vec![Link {
                    source: "src.x".into(),
                    target: "lib.foo".into(),
                    kind: "Imports".into(),
                    weight: 5,
                    confidence: 1.0,
                    confidence_kind: "Extracted".into(),
                    in_cycle: false,
                }],
            },
        },
    ];
    let report = score_stubs(&inputs, 1);
    let suggestions: Vec<&str> = report
        .settings
        .iter()
        .map(|s| s.prefix.as_str())
        .collect();
    assert!(!suggestions.contains(&"lib"));
    assert!(!suggestions.contains(&"app"));
}
```

(Note: the `Link`/`Node` field shape comes from existing `suggest.rs` tests. Copy from the closest existing test in that file to keep field names aligned.)

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-cli barrel_exclusion 2>&1 | tail -20
cargo test -p graphify-report score_stubs_skips_all_multi 2>&1 | tail -20
```

Expected: compile errors — `local_prefixes` field doesn't exist on `ProjectInput`; `barrel_exclusion_ids` still returns empty for Multi.

- [ ] **Step 3: Implement multi-prefix `barrel_exclusion_ids`**

Replace the existing function in `crates/graphify-cli/src/main.rs:3203-3219`:

```rust
fn barrel_exclusion_ids<'a>(
    project: &'a ProjectConfig,
    consolidation: &ConsolidationConfig,
) -> Vec<&'a str> {
    if !consolidation.suppress_barrel_cycles() {
        return Vec::new();
    }
    let candidates: Vec<&'a str> = match &project.local_prefix {
        Some(LocalPrefix::Single(p)) if !p.is_empty() => vec![p.as_str()],
        Some(LocalPrefix::Multi(v)) => v.iter().map(|s| s.as_str()).collect(),
        _ => return Vec::new(),
    };
    candidates
        .into_iter()
        .filter(|p| consolidation.matches(p))
        .collect()
}
```

(Note: spec mentioned intersecting with graph nodes. For YAGNI in this slice, allowlist gating is enough — the consumer passes the result through `find_simple_cycles_excluding` which already handles "node not in graph" gracefully.)

- [ ] **Step 4: Implement multi-prefix `ProjectInput`**

In `crates/graphify-report/src/suggest.rs:38-48`, change:

```rust
pub struct ProjectInput<'a> {
    pub name: &'a str,
    pub local_prefix: &'a str,
    pub current_stubs: &'a ExternalStubs,
    pub graph: &'a GraphSnapshot,
}
```

to:

```rust
pub struct ProjectInput<'a> {
    pub name: &'a str,
    /// All local-root prefixes (single string for legacy `local_prefix = "src"`
    /// configs, multiple for `local_prefix = ["app", "lib", ...]`). Every entry
    /// is added to the shadow-set so the scorer never suggests one as an
    /// external_stub.
    pub local_prefixes: &'a [&'a str],
    pub current_stubs: &'a ExternalStubs,
    pub graph: &'a GraphSnapshot,
}
```

In `score_stubs` (around `:160-180`), change the shadow-set construction:

```rust
        if !p.local_prefix.is_empty() {
            shadow_set.insert(p.local_prefix.to_string());
        }
```

to:

```rust
        for prefix in p.local_prefixes {
            if !prefix.is_empty() {
                shadow_set.insert((*prefix).to_string());
            }
        }
```

Update the suggestion message at `:426`:

```rust
            "{} prefix(es) matching local_prefix or known module: {}",
```

— this string already uses "local_prefix" generically; leave it.

Update every existing test in `suggest.rs` that constructs `ProjectInput { local_prefix: "x", ... }` to use `local_prefixes: &["x"]` instead. Search-replace; about 12 sites (per the earlier grep).

- [ ] **Step 5: Update the CLI's call site**

In `crates/graphify-cli/src/main.rs` around `:5523` (the `loaded.iter().map(...)` constructing `ProjectInput`), change:

```rust
    let inputs: Vec<ProjectInput<'_>> = loaded
        .iter()
        .map(|l| ProjectInput {
            name: l.name.as_str(),
            local_prefix: l.local_prefix.as_str(),
            current_stubs: &l.stubs,
            graph: &l.graph,
        })
        .collect();
```

The `Loaded` struct stored a single `local_prefix: String` per Task 2's transitional code. Update it to hold `local_prefixes: Vec<String>`:

```rust
    struct Loaded {
        name: String,
        local_prefixes: Vec<String>,
        stubs: ExternalStubs,
        graph: GraphSnapshot,
    }
```

Replace the population around `:5496-5510`:

```rust
        let local_prefixes: Vec<String> = match &project.local_prefix {
            Some(LocalPrefix::Single(s)) if !s.is_empty() => vec![s.clone()],
            Some(LocalPrefix::Single(_)) | None => vec!["src".to_string()], // legacy default
            Some(LocalPrefix::Multi(v)) => v.clone(),
        };
```

And the input mapping:

```rust
    let inputs: Vec<ProjectInput<'_>> = loaded
        .iter()
        .map(|l| {
            let prefix_refs: Vec<&str> = l.local_prefixes.iter().map(|s| s.as_str()).collect();
            ProjectInput {
                name: l.name.as_str(),
                // Need a stable backing slice; use a per-loaded Box<[&str]>:
                local_prefixes: &[],  // placeholder — see fix below
                current_stubs: &l.stubs,
                graph: &l.graph,
            }
        })
        .collect();
```

**Lifetime gotcha:** `&[&str]` borrowing from a `Vec<&str>` constructed inside `.map()` won't outlive the closure. Two clean options — pick the simpler:

(a) Store `prefix_refs: Vec<&'l str>` on `Loaded` itself. Won't work — self-referential struct.

(b) Compute the `&[&str]` slice in the outer iteration loop, alongside `inputs`:

```rust
    let prefix_buffers: Vec<Vec<&str>> = loaded
        .iter()
        .map(|l| l.local_prefixes.iter().map(|s| s.as_str()).collect())
        .collect();

    let inputs: Vec<ProjectInput<'_>> = loaded
        .iter()
        .zip(prefix_buffers.iter())
        .map(|(l, prefixes)| ProjectInput {
            name: l.name.as_str(),
            local_prefixes: prefixes.as_slice(),
            current_stubs: &l.stubs,
            graph: &l.graph,
        })
        .collect();
```

Use option (b).

- [ ] **Step 6: Run tests + full build**

```bash
cargo build --workspace 2>&1 | tail -10
cargo test --workspace 2>&1 | tail -20
```

Expected: clean build; all tests pass (the 12 migrated `ProjectInput { local_prefix: "x" }` sites in `suggest.rs` tests now use `local_prefixes: &["x"]`).

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-cli/src/main.rs crates/graphify-report/src/suggest.rs
git commit -m "feat: barrel exclusion + suggest stubs handle multi-prefix (FEAT-049)"
```

---

## Task 9: Auto-detect multi-root advisory warning

**Files:**
- Modify: `crates/graphify-extract/src/walker.rs:279-309` (`detect_local_prefix`)

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `crates/graphify-extract/src/walker.rs`:

```rust
#[test]
fn detect_local_prefix_warns_on_balanced_multi_root_pattern() {
    // Build a temp dir with two top-level dirs, each holding 12 .ts files —
    // top-1 < 3× top-2 (in fact 1×) → warning should fire.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for i in 0..12 {
        let p = root.join("app").join(format!("a{i}.ts"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }
    for i in 0..12 {
        let p = root.join("lib").join(format!("l{i}.ts"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    let mut warning_buf = Vec::<u8>::new();
    let prefix = detect_local_prefix_with_warning_sink(
        root,
        &[Language::TypeScript],
        &[],
        &mut warning_buf,
    );
    let warning = String::from_utf8(warning_buf).unwrap();
    assert!(
        warning.contains("Multi-root pattern detected"),
        "warning was: {warning}"
    );
    assert!(warning.contains("app"), "warning was: {warning}");
    assert!(warning.contains("lib"), "warning was: {warning}");
    // Function still returns a single prefix (or empty).
    assert!(prefix.is_empty() || prefix == "app" || prefix == "lib");
}

#[test]
fn detect_local_prefix_no_warning_when_top1_dominates() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for i in 0..30 {
        let p = root.join("src").join(format!("s{i}.ts"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }
    for i in 0..3 {
        let p = root.join("scripts").join(format!("x{i}.ts"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    let mut warning_buf = Vec::<u8>::new();
    let prefix = detect_local_prefix_with_warning_sink(
        root,
        &[Language::TypeScript],
        &[],
        &mut warning_buf,
    );
    let warning = String::from_utf8(warning_buf).unwrap();
    assert!(warning.is_empty(), "should not warn; got: {warning}");
    assert_eq!(prefix, "src");
}

#[test]
fn detect_local_prefix_no_warning_when_only_one_dir_has_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for i in 0..15 {
        let p = root.join("src").join(format!("s{i}.ts"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    let mut warning_buf = Vec::<u8>::new();
    detect_local_prefix_with_warning_sink(
        root,
        &[Language::TypeScript],
        &[],
        &mut warning_buf,
    );
    let warning = String::from_utf8(warning_buf).unwrap();
    assert!(warning.is_empty(), "should not warn; got: {warning}");
}

#[test]
fn detect_local_prefix_no_warning_below_min_files_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Two dirs with 5 files each — below the ≥10 threshold.
    for i in 0..5 {
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a").join(format!("{i}.ts")), "").unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::write(root.join("b").join(format!("{i}.ts")), "").unwrap();
    }
    let mut warning_buf = Vec::<u8>::new();
    detect_local_prefix_with_warning_sink(
        root,
        &[Language::TypeScript],
        &[],
        &mut warning_buf,
    );
    let warning = String::from_utf8(warning_buf).unwrap();
    assert!(warning.is_empty(), "should not warn; got: {warning}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cargo test -p graphify-extract detect_local_prefix_warns\|detect_local_prefix_no_warning 2>&1 | tail -15
```

Expected: compile error — `detect_local_prefix_with_warning_sink` not defined.

- [ ] **Step 3: Implement the warning-aware variant + wire stderr default**

Add to `crates/graphify-extract/src/walker.rs`:

```rust
const MULTI_ROOT_WARNING_MIN_FILES: usize = 10;
const MULTI_ROOT_WARNING_RATIO: f64 = 3.0;

/// Variant of [`detect_local_prefix`] that writes its multi-root advisory
/// warning to a caller-provided sink. The default `detect_local_prefix`
/// wraps this with an `eprintln!`-shaped sink targeting stderr.
pub fn detect_local_prefix_with_warning_sink<W: std::io::Write>(
    root: &Path,
    languages: &[Language],
    extra_excludes: &[&str],
    warning_sink: &mut W,
) -> String {
    let mut excludes: Vec<&str> = DEFAULT_EXCLUDES.to_vec();
    excludes.extend_from_slice(extra_excludes);

    let mut total_files = 0usize;
    let mut root_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    count_source_roots(
        root,
        root,
        languages,
        &excludes,
        &mut total_files,
        &mut root_counts,
    );

    if total_files == 0 {
        return String::new();
    }

    // Multi-root advisory: ≥2 dirs each with ≥MIN_FILES, top-1 < RATIO × top-2.
    let mut sorted: Vec<(&String, &usize)> = root_counts
        .iter()
        .filter(|(_, c)| **c >= MULTI_ROOT_WARNING_MIN_FILES)
        .collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    if sorted.len() >= 2 {
        let top1 = *sorted[0].1 as f64;
        let top2 = *sorted[1].1 as f64;
        if top1 < top2 * MULTI_ROOT_WARNING_RATIO {
            let candidates: Vec<&str> = sorted.iter().map(|(k, _)| k.as_str()).collect();
            let _ = writeln!(
                warning_sink,
                "Multi-root pattern detected: candidates [{}]. \
                 Consider local_prefix = [{}] in graphify.toml. \
                 Auto-detected single prefix '{}' for now.",
                candidates.join(", "),
                candidates.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", "),
                sorted[0].0,
            );
        }
    }

    let threshold = |count: usize| (count as f64) / (total_files as f64) > 0.6;

    if threshold(*root_counts.get("src").unwrap_or(&0)) {
        return "src".to_owned();
    }
    if threshold(*root_counts.get("app").unwrap_or(&0)) {
        return "app".to_owned();
    }

    String::new()
}

/// Stderr-default wrapper preserved for backward compatibility.
pub fn detect_local_prefix(root: &Path, languages: &[Language], extra_excludes: &[&str]) -> String {
    detect_local_prefix_with_warning_sink(
        root,
        languages,
        extra_excludes,
        &mut std::io::stderr(),
    )
}
```

(The original `detect_local_prefix` body is replaced — the new wrapper delegates to the warning-aware variant.)

- [ ] **Step 4: Run the tests + manual stderr check**

```bash
cargo test -p graphify-extract detect_local_prefix 2>&1 | tail -30
```

Expected: 4 new tests pass; existing 4 tests (`detect_local_prefix_prefers_src_when_it_dominates`, `detect_local_prefix_prefers_app_when_it_dominates`, `detect_local_prefix_returns_empty_when_no_directory_dominates`, `detect_local_prefix_returns_empty_when_root_files_are_significant`) still pass — they don't read stderr.

Manual smoke test on dogfood `graphify-extract` crate (which has `local_prefix = "src"` set, so auto-detect is bypassed — drop the prefix temporarily to test the warning):

```bash
cargo run -p graphify-cli --release -- run --config /tmp/feat-049-warn-test.toml 2>&1 | head -5
```

Where `/tmp/feat-049-warn-test.toml` is:

```toml
[settings]
output = "/tmp/feat-049-warn-out"

[[project]]
name = "this-repo"
repo = "."
lang = ["rust"]
# local_prefix omitted on purpose — should trigger detect_local_prefix
exclude = ["target", "report"]
```

Expected: stderr shows `Multi-root pattern detected: candidates [crates, ...]` because this repo has `crates/`, `docs/`, `integrations/`, etc. (Not strictly required to pass — the heuristic is advisory.)

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/walker.rs
git commit -m "feat(extract): detect_local_prefix emits multi-root advisory warning (FEAT-049)"
```

---

## Task 10: Integration test on Expo-shaped fixture

**Files:**
- Create: `crates/graphify-cli/tests/feat_049_multi_root.rs` (integration test using a temp-dir TS fixture)

- [ ] **Step 1: Write the failing test**

Create `crates/graphify-cli/tests/feat_049_multi_root.rs`:

```rust
//! End-to-end integration test for FEAT-049 multi-root local_prefix.
//!
//! Builds an Expo-shaped fixture (parallel `app/`, `lib/`, `components/`),
//! runs `graphify run` against it, and asserts:
//!   - module IDs are not wrapped under any prefix (no `app.lib.foo`)
//!   - cross-imports between `lib/` and `components/` resolve as local edges
//!   - third-party imports (`react`) are NOT classified local

use std::path::PathBuf;
use std::process::Command;

fn graphify_bin() -> PathBuf {
    // Built artifact path — assumes `cargo build -p graphify-cli` already ran.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../target/debug/graphify");
    p
}

#[test]
fn multi_root_expo_fixture_no_wrapping_and_third_party_external() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let out = dir.path().join("report");
    std::fs::create_dir_all(repo.join("app/(tabs)")).unwrap();
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    std::fs::create_dir_all(repo.join("components")).unwrap();

    std::fs::write(
        repo.join("app/(tabs)/_layout.tsx"),
        r#"
import { Button } from "@/components/Button";
import { client } from "@/lib/api";
import React from "react";

export default function Layout() {
  return null;
}
"#,
    )
    .unwrap();

    std::fs::write(
        repo.join("lib/api.ts"),
        r#"
export const client = {};
"#,
    )
    .unwrap();

    std::fs::write(
        repo.join("components/Button.tsx"),
        r#"
import { client } from "@/lib/api";
export const Button = () => null;
"#,
    )
    .unwrap();

    // Minimal tsconfig.json with `@` alias to repo root — same shape as
    // Expo Router's default.
    std::fs::write(
        repo.join("tsconfig.json"),
        r#"
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@/*": ["./*"] }
  }
}
"#,
    )
    .unwrap();

    let config = format!(
        r#"
[settings]
output = "{}"

[[project]]
name = "mobile"
repo = "{}"
lang = ["typescript"]
local_prefix = ["app", "lib", "components"]
external_stubs = ["react"]
"#,
        out.display(),
        repo.display(),
    );
    let cfg_path = dir.path().join("graphify.toml");
    std::fs::write(&cfg_path, config).unwrap();

    let output = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&cfg_path)
        .output()
        .expect("graphify binary should run");

    assert!(
        output.status.success(),
        "graphify run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let analysis = std::fs::read_to_string(out.join("mobile/analysis.json"))
        .expect("analysis.json must exist");
    let json: serde_json::Value = serde_json::from_str(&analysis).unwrap();

    let nodes = json["nodes"].as_array().expect("nodes is an array");
    let node_ids: Vec<&str> = nodes
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();

    // No wrapping — `lib.api`, `components.Button` exist directly.
    assert!(
        node_ids.iter().any(|id| *id == "lib.api"),
        "expected 'lib.api' in nodes; got: {node_ids:?}"
    );
    assert!(
        node_ids.iter().any(|id| *id == "components.Button"),
        "expected 'components.Button' in nodes; got: {node_ids:?}"
    );
    // No `app.lib.api` (would prove wrapping leaked).
    assert!(
        !node_ids.iter().any(|id| *id == "app.lib.api"),
        "must not wrap under 'app': {node_ids:?}"
    );

    // `react` should be classified ExpectedExternal via external_stubs.
    let react_node = nodes
        .iter()
        .find(|n| n["id"].as_str() == Some("react"))
        .expect("react node should exist");
    let is_local = react_node["is_local"].as_bool().unwrap_or(true);
    assert!(!is_local, "react must be classified non-local: {react_node:?}");
}
```

- [ ] **Step 2: Build the binary + run the test**

```bash
cargo build -p graphify-cli 2>&1 | tail -5
cargo test -p graphify-cli --test feat_049_multi_root 2>&1 | tail -30
```

Expected: test passes. If it fails on a specific assertion, the failure points at which Task (3-8) needs more wiring. Diagnose and patch in this Task before committing.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-cli/tests/feat_049_multi_root.rs
git commit -m "test(cli): end-to-end Expo-shaped fixture for multi-root local_prefix (FEAT-049)"
```

---

## Task 11: Documentation + version bump

**Files:**
- Modify: `CHANGELOG.md` (top of file — new entry under unreleased / next version)
- Modify: `CLAUDE.md` (add convention bullet near the existing `local_prefix` mentions)
- Modify: `Cargo.toml` (workspace `[workspace.package].version` bump)
- Modify: `crates/graphify-cli/src/main.rs` `cmd_init` — update the template's `local_prefix` line to mention the array form (around `:1441`)

- [ ] **Step 1: Bump version**

Edit `Cargo.toml` `[workspace.package]` — bump the patch (or minor, your call) version. For a meaningful new feature, **prefer minor**: `0.13.7` → `0.14.0`.

- [ ] **Step 2: Update `cmd_init` template**

In `crates/graphify-cli/src/main.rs:1441`, change:

```rust
local_prefix = "app"        # Leave unset for PHP — PSR-4 from composer.json
```

to:

```rust
local_prefix = "app"        # String form: prefix wraps file paths (e.g. "lib/foo.ts" → "app.lib.foo").
                            # Array form for multi-root projects (Expo Router and similar):
                            #   local_prefix = ["app", "lib", "components"]
                            # Array form does NOT wrap — file paths stay as-is.
                            # Leave unset for PHP — PSR-4 from composer.json provides the prefix structure.
```

- [ ] **Step 3: Add CHANGELOG entry**

At the top of `CHANGELOG.md`, add (mimicking existing entries' style):

```markdown
## 0.14.0 — 2026-05-02

### Added
- **FEAT-049**: `[[project]].local_prefix` now accepts an array of root
  directories in addition to the existing string form. Designed for Expo
  Router and similar layouts where source spans parallel top-level dirs
  (`app/`, `lib/`, `components/`) without a common parent. Array form is
  no-wrap (file IDs stay as natural paths); string form keeps current
  wrapping behavior, zero breaking change. Auto-detect emits an advisory
  warning when a multi-root pattern is suspected. See
  `docs/superpowers/specs/2026-05-02-feat-049-multi-root-local-prefix-design.md`.
```

- [ ] **Step 4: Add CLAUDE.md convention bullet**

In `CLAUDE.md`, search for the section that documents `local_prefix` conventions (look for "PHP projects should leave `[[project]].local_prefix` unset" near the bottom of the Conventions list). Add immediately after it:

```markdown
- `[[project]].local_prefix` accepts a string OR an array (FEAT-049 / GH #16). String form (`local_prefix = "src"`) wraps file paths under the prefix and is unchanged from previous releases. Array form (`local_prefix = ["app", "lib", "components"]`) does NOT wrap — IDs stay as natural paths (`lib/util.ts` → `lib.util`). Use array form for Expo Router and similar layouts. Internal flow: TOML `LocalPrefix { Single, Multi }` collapses to `EffectiveLocalPrefix { prefixes, wrap }` at config-load time; downstream walker/resolver/cache consume the effective form. Cache key is `prefix.cache_key()` (`"src"` for Single, `"multi:<sorted>"` for Multi) — switching shapes invalidates the cache automatically. PHP projects reject `Multi` fail-fast (PSR-4 already provides per-namespace roots).
```

- [ ] **Step 5: Build + run all tests once more**

```bash
cargo build --workspace --release 2>&1 | tail -5
cargo test --workspace 2>&1 | tail -10
cargo fmt --all -- --check 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -5
```

Expected: clean build, all tests green, fmt/clippy gates pass.

If `cargo clippy` flags anything in the new code, fix inline before committing.

- [ ] **Step 6: Local install + smoke test**

```bash
cargo install --path crates/graphify-cli --force
graphify --version
```

Expected: `graphify 0.14.0` (or whatever bump landed in Step 1).

Re-run the dogfood pipeline to catch any regression on the canonical config:

```bash
graphify run --config graphify.toml 2>&1 | tail -10
graphify check --config graphify.toml 2>&1 | tail -10
```

Expected: same output as before — 5 projects, 0 cycles, hotspots unchanged.

- [ ] **Step 7: Commit + tag**

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md CLAUDE.md crates/graphify-cli/src/main.rs
git commit -m "$(cat <<'EOF'
release: 0.14.0 — multi-root local_prefix (FEAT-049, GH #16)

Wraps up FEAT-049: multi-root `local_prefix` for Expo Router and similar
layouts. String form unchanged (zero breaking change); array form is new,
no-wrap, opt-in. Auto-detect emits advisory warning on suspected multi-root
patterns. CLAUDE.md updated with the new convention.
EOF
)"
git tag v0.14.0
```

(Per CLAUDE.md, the user pushes the tag manually after this point: `git push origin main --tags` triggers CI release.)

---

## Self-Review Checklist

**1. Spec coverage** — every spec section has a Task:
- Spec §3 (TOML surface + Rust types) → Task 1 (`LocalPrefix`, `EffectiveLocalPrefix`)
- Spec §3 (validation rules) → Task 2 (`validate_local_prefix` + load_config)
- Spec §4 (walker semantics) → Tasks 3, 6 (walker `_eff` + CLI plumbing)
- Spec §5 (resolver impact) → Task 4
- Spec §6 (cache invalidation) → Task 5
- Spec §7 (barrel exclusion) → Task 8
- Spec §8 (suggest stubs shadow set) → Task 8
- Spec §9 (auto-detect advisory) → Task 9
- Spec §10 (end-to-end behavior) → Task 10 (integration fixture)
- Spec §11 (compatibility) → covered by Tasks 2-8 not breaking existing tests + dogfood smoke in Tasks 6, 11
- Spec §13 (test sketch) → distributed across Tasks 1-10
- MCP plumbing → Task 7 (mentioned in spec under §5)
- CHANGELOG, CLAUDE.md, version bump → Task 11

**2. Placeholder scan** — no TBD, TODO, "implement later", "similar to Task N", or vague-handling phrases. Every step has runnable code/commands.

**3. Type consistency** — confirmed:
- `LocalPrefix::Single(String)` and `LocalPrefix::Multi(Vec<String>)` used identically across all tasks
- `EffectiveLocalPrefix { prefixes: Vec<String>, wrap: bool }` field names stable
- `set_local_prefixes(&[String], wrap: bool)` signature stable across Task 4 and call sites in Tasks 6, 7
- `ProjectInput.local_prefixes: &'a [&'a str]` (renamed from `local_prefix`) stable in Task 8

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-02-feat-049-multi-root-local-prefix.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
