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
        self.prefixes.first().map(|s| s.as_str()).unwrap_or("")
    }

    /// Returns true if this represents the array form (no-wrap mode).
    pub fn is_multi(&self) -> bool {
        !self.wrap
    }

    /// Returns the number of distinct prefixes.
    pub fn len(&self) -> usize {
        self.prefixes.len()
    }

    /// Always `false` — `prefixes` is non-empty by construction (omitted form
    /// holds a single empty string; `Single`/`Multi` always yield ≥1 entry).
    /// Provided to satisfy clippy's `len_without_is_empty` lint.
    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
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

    /// The prefix string to pass to legacy `&str`-based APIs.
    ///
    /// In wrap mode (`Single`), returns the head prefix — legacy callers
    /// prepend it to file paths to compute module ids. In no-wrap mode
    /// (`Multi`), returns an empty string so legacy paths are computed
    /// relative to the project root with no prepending.
    ///
    /// Centralizes the wrap-rule used by the `_eff` walker entry points
    /// and any future legacy bridge sites.
    pub fn legacy_prefix(&self) -> &str {
        if self.wrap {
            self.first()
        } else {
            ""
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

/// Validate a project's `local_prefix` value. Returns `Ok(Some(warning))` when
/// the value is legal but suspect (single-element array, dupes), `Ok(None)`
/// when fully clean, and `Err(message)` on a fail-fast condition (empty array,
/// `Multi` on PHP).
///
/// Hoisted from `graphify-cli` (CHORE-012) so both the CLI and the MCP
/// `load_config` paths can share the same validation. Pure function: no I/O,
/// no globals — caller is responsible for routing the result (eprintln vs
/// exit).
pub fn validate_local_prefix(
    project_name: &str,
    lp: &Option<LocalPrefix>,
    languages: &[String],
) -> Result<Option<String>, String> {
    let Some(lp) = lp else {
        return Ok(None);
    };

    let is_php_only =
        !languages.is_empty() && languages.iter().all(|l| l.eq_ignore_ascii_case("php"));

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
    fn legacy_prefix_wrap_returns_first() {
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Single("src".to_string()));
        assert_eq!(eff.legacy_prefix(), "src");
    }

    #[test]
    fn legacy_prefix_no_wrap_returns_empty() {
        let eff = EffectiveLocalPrefix::from(&LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
        ]));
        assert_eq!(eff.legacy_prefix(), "");
    }

    #[test]
    fn legacy_prefix_omitted_returns_empty_string() {
        // Omitted form is wrap-mode with empty first prefix — equivalent
        // to legacy "no prefix configured" path.
        let eff = EffectiveLocalPrefix::omitted();
        assert_eq!(eff.legacy_prefix(), "");
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

    // validate_local_prefix tests — moved from graphify-cli (CHORE-012).

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
        let lp = LocalPrefix::Multi(vec![
            "app".to_string(),
            "lib".to_string(),
            "app".to_string(),
        ]);
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
}
