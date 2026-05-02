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
}
