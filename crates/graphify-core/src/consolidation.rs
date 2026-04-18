//! Consolidation allowlist — user-curated regex patterns that mark symbols as
//! intentional "duplicates" (cross-language contracts, vendor mirrors, DTO
//! families) so consumers can filter them out of consolidation candidates,
//! hotspot annotations, and drift reports.
//!
//! The config lives in `graphify.toml` under `[consolidation]` and is piped
//! through the pipeline. Absent section = current behaviour (no-op).
//!
//! Patterns are regexes anchored `^...$` and matched against the *leaf* symbol
//! name (the last dot-segment of a node id, e.g. `TokenUsage` from
//! `app.models.tokens.TokenUsage`).
//!
//! Compilation happens once at config-load time; invalid patterns fail fast
//! so malformed `graphify.toml` never reaches the pipeline.

use std::collections::HashMap;

use regex::Regex;

/// Raw, pre-compile consolidation config — typically parsed from TOML.
#[derive(Debug, Default, Clone)]
pub struct ConsolidationConfigRaw {
    /// Regex patterns matched against the leaf symbol name. Each pattern is
    /// automatically anchored (`^...$`) before compilation.
    pub allowlist: Vec<String>,
    /// Declared intentional cross-project mirrors. The key is the leaf symbol
    /// name; the value is a list of `"<project>:<node_id>"` qualifiers naming
    /// every endpoint that is expected to hold the mirror.
    pub intentional_mirrors: HashMap<String, Vec<String>>,
}

/// Compiled consolidation config — ready to apply at report time.
#[derive(Debug, Default, Clone)]
pub struct ConsolidationConfig {
    patterns: Vec<Regex>,
    /// Original regex strings, preserved so they can be serialized back into
    /// `analysis.json` for downstream consumers.
    pattern_sources: Vec<String>,
    intentional_mirrors: HashMap<String, Vec<String>>,
}

/// Error returned when an allowlist entry fails to compile.
#[derive(Debug)]
pub struct InvalidAllowlistPattern {
    pub pattern: String,
    pub error: String,
}

impl std::fmt::Display for InvalidAllowlistPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid consolidation allowlist pattern {:?}: {}",
            self.pattern, self.error
        )
    }
}

impl std::error::Error for InvalidAllowlistPattern {}

impl ConsolidationConfig {
    /// Compiles a raw config into an active one. Each pattern is anchored with
    /// `^...$` before compilation to prevent accidental substring matches.
    pub fn compile(raw: ConsolidationConfigRaw) -> Result<Self, InvalidAllowlistPattern> {
        let mut patterns = Vec::with_capacity(raw.allowlist.len());
        let mut pattern_sources = Vec::with_capacity(raw.allowlist.len());
        for src in raw.allowlist {
            let anchored = anchor(&src);
            match Regex::new(&anchored) {
                Ok(re) => {
                    patterns.push(re);
                    pattern_sources.push(src);
                }
                Err(e) => {
                    return Err(InvalidAllowlistPattern {
                        pattern: src,
                        error: e.to_string(),
                    });
                }
            }
        }
        Ok(Self {
            patterns,
            pattern_sources,
            intentional_mirrors: raw.intentional_mirrors,
        })
    }

    /// True if no allowlist entries and no intentional mirrors are declared.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.intentional_mirrors.is_empty()
    }

    /// Original (un-anchored) pattern strings, in source order.
    pub fn pattern_sources(&self) -> &[String] {
        &self.pattern_sources
    }

    /// Intentional cross-project mirrors.
    pub fn intentional_mirrors(&self) -> &HashMap<String, Vec<String>> {
        &self.intentional_mirrors
    }

    /// Returns true if the leaf symbol name of `node_id` matches any
    /// allowlist pattern.
    pub fn matches(&self, node_id: &str) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let leaf = leaf_symbol(node_id);
        self.patterns.iter().any(|re| re.is_match(leaf))
    }

    /// Collects every node id whose leaf symbol matches the allowlist.
    pub fn allowlisted<'a, I>(&self, ids: I) -> Vec<String>
    where
        I: IntoIterator<Item = &'a str>,
    {
        if self.patterns.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<String> = ids
            .into_iter()
            .filter(|id| self.matches(id))
            .map(|s| s.to_string())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Extracts the leaf symbol name (portion after the last `.`).
///
/// A node id like `app.models.tokens.TokenUsage` yields `"TokenUsage"`.
/// IDs with no dot return themselves unchanged.
pub fn leaf_symbol(node_id: &str) -> &str {
    match node_id.rfind('.') {
        Some(idx) => &node_id[idx + 1..],
        None => node_id,
    }
}

fn anchor(pattern: &str) -> String {
    let mut s = String::with_capacity(pattern.len() + 2);
    if !pattern.starts_with('^') {
        s.push('^');
    }
    s.push_str(pattern);
    if !pattern.ends_with('$') {
        s.push('$');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_symbol_strips_dotted_prefix() {
        assert_eq!(leaf_symbol("app.models.TokenUsage"), "TokenUsage");
        assert_eq!(leaf_symbol("TokenUsage"), "TokenUsage");
        assert_eq!(leaf_symbol(""), "");
        assert_eq!(leaf_symbol("a.b.c"), "c");
    }

    #[test]
    fn anchor_idempotent() {
        assert_eq!(anchor("TokenUsage"), "^TokenUsage$");
        assert_eq!(anchor("^TokenUsage$"), "^TokenUsage$");
        assert_eq!(anchor("^TokenUsage"), "^TokenUsage$");
        assert_eq!(anchor("TokenUsage$"), "^TokenUsage$");
    }

    #[test]
    fn compile_rejects_invalid_regex() {
        let raw = ConsolidationConfigRaw {
            allowlist: vec!["[".into()],
            ..Default::default()
        };
        let err = ConsolidationConfig::compile(raw).unwrap_err();
        assert_eq!(err.pattern, "[");
    }

    #[test]
    fn matches_leaf_symbol_not_substring() {
        let cfg = ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec!["TokenUsage".into()],
            ..Default::default()
        })
        .unwrap();
        // exact match on leaf
        assert!(cfg.matches("app.models.TokenUsage"));
        // leaf differs — must not match even though "TokenUsage" appears as
        // a substring of the full id.
        assert!(!cfg.matches("app.models.TokenUsageAdapter"));
        assert!(!cfg.matches("app.TokenUsage.inner"));
    }

    #[test]
    fn matches_regex_pattern() {
        let cfg = ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec![".*(Response|Output|Dto)".into()],
            ..Default::default()
        })
        .unwrap();
        assert!(cfg.matches("pkg.types.UserResponse"));
        assert!(cfg.matches("pkg.types.RawOutput"));
        assert!(cfg.matches("pkg.types.OrderDto"));
        assert!(!cfg.matches("pkg.types.UserRequest"));
    }

    #[test]
    fn allowlisted_returns_sorted_unique() {
        let cfg = ConsolidationConfig::compile(ConsolidationConfigRaw {
            allowlist: vec!["TokenUsage".into(), "LessonType".into()],
            ..Default::default()
        })
        .unwrap();
        let ids = [
            "b.TokenUsage",
            "a.TokenUsage",
            "c.LessonType",
            "c.LessonType",
            "x.Other",
        ];
        let out = cfg.allowlisted(ids.iter().copied());
        assert_eq!(
            out,
            vec![
                "a.TokenUsage".to_string(),
                "b.TokenUsage".to_string(),
                "c.LessonType".to_string(),
            ]
        );
    }

    #[test]
    fn empty_config_matches_nothing() {
        let cfg = ConsolidationConfig::default();
        assert!(cfg.is_empty());
        assert!(!cfg.matches("anything"));
        let ids: Vec<&str> = vec!["a.b", "c.d"];
        assert!(cfg.allowlisted(ids).is_empty());
    }
}
