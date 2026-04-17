//! External-stub prefix matcher.
//!
//! Supports `[[project]].external_stubs` in `graphify.toml`: consumers declare
//! packages (npm, Python, etc.) they know are intentionally external. Edges
//! that resolve to those packages get `ConfidenceKind::ExpectedExternal`
//! instead of `Ambiguous`, so the ambiguity metric reflects only edges the
//! extractor failed to classify — not correct-but-opaque external references.

/// Pre-compiled list of external-stub prefixes.
///
/// Matching rule: a target matches if it equals a stub or starts with the
/// stub followed by `/` or `.`. So `drizzle-orm` matches `drizzle-orm` and
/// `drizzle-orm/pg-core` and `drizzle-orm.eq`, but NOT `drizzle-orm-extra`.
/// This keeps the matcher safe against accidental prefix collisions without
/// requiring glob syntax in v1.
#[derive(Debug, Clone, Default)]
pub struct ExternalStubs {
    prefixes: Vec<String>,
}

impl ExternalStubs {
    pub fn new<I, S>(prefixes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut prefixes: Vec<String> = prefixes
            .into_iter()
            .map(Into::into)
            .filter(|p| !p.is_empty())
            .collect();
        // Longer prefixes first so `drizzle-orm/pg-core` wins over `drizzle-orm`.
        prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
        Self { prefixes }
    }

    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }

    /// Returns true if `target` equals any stub or starts with a stub followed
    /// by `/` or `.`.
    pub fn matches(&self, target: &str) -> bool {
        self.prefixes.iter().any(|p| prefix_matches(p, target))
    }
}

fn prefix_matches(prefix: &str, target: &str) -> bool {
    if target == prefix {
        return true;
    }
    if let Some(rest) = target.strip_prefix(prefix) {
        // Require a boundary char so `drizzle-orm` does not match `drizzle-orm-extra`.
        return rest.starts_with('/') || rest.starts_with('.');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let stubs = ExternalStubs::new(["drizzle-orm"]);
        assert!(stubs.matches("drizzle-orm"));
    }

    #[test]
    fn slash_subpath_match() {
        let stubs = ExternalStubs::new(["drizzle-orm"]);
        assert!(stubs.matches("drizzle-orm/pg-core"));
        assert!(stubs.matches("drizzle-orm/postgres-js"));
    }

    #[test]
    fn dot_suffix_match_for_symbol_nodes() {
        // Extractor sometimes emits targets like `drizzle-orm.eq` for bare symbols
        // re-exported from external packages.
        let stubs = ExternalStubs::new(["drizzle-orm"]);
        assert!(stubs.matches("drizzle-orm.eq"));
    }

    #[test]
    fn no_substring_false_match() {
        let stubs = ExternalStubs::new(["drizzle-orm"]);
        assert!(!stubs.matches("drizzle-orm-extra"));
        assert!(!stubs.matches("my-drizzle-orm"));
    }

    #[test]
    fn empty_stubs_never_matches() {
        let stubs = ExternalStubs::default();
        assert!(!stubs.matches("anything"));
        assert!(stubs.is_empty());
    }

    #[test]
    fn empty_prefix_is_filtered() {
        let stubs = ExternalStubs::new(["", "zod"]);
        assert!(stubs.matches("zod"));
        assert!(!stubs.matches(""));
        assert!(!stubs.matches("foo"));
    }

    #[test]
    fn scoped_npm_package_matches() {
        let stubs = ExternalStubs::new(["@repo/types"]);
        assert!(stubs.matches("@repo/types"));
        assert!(stubs.matches("@repo/types/nested"));
        assert!(!stubs.matches("@repo/types-extra"));
        assert!(!stubs.matches("@repo/other"));
    }

    #[test]
    fn longer_prefix_evaluated_first() {
        // Not observable from `matches` alone, but the sort stability matters
        // if callers ever ask which prefix matched. Smoke-test both work.
        let stubs = ExternalStubs::new(["drizzle-orm", "drizzle-orm/pg-core"]);
        assert!(stubs.matches("drizzle-orm/pg-core/schema"));
        assert!(stubs.matches("drizzle-orm/other"));
    }
}
