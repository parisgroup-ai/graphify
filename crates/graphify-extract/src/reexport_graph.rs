//! Project-wide re-export graph for TypeScript barrel collapse (FEAT-021).
//!
//! The TypeScript extractor captures `export … from …` statements into
//! [`crate::lang::ReExportEntry`] values as a side effect of normal extraction.
//! After every file has been extracted and the [`crate::resolver::ModuleResolver`]
//! knows every local module, this module consumes those entries and builds a
//! searchable graph that lets callers ask:
//!
//! > "Given a consumer that imports `foo` from barrel `M`, where is the
//! > canonical declaration of `foo`?"
//!
//! # Design
//!
//! A [`ReExportGraph`] stores, per resolved module, a map from published
//! local name → the upstream `(module, name)` it forwards to. `export *`
//! entries are stored separately and checked when the named lookup misses.
//! Star edges are only followed when the upstream module also appears in
//! the local project (`is_local == true`) — otherwise the chain terminates.
//!
//! [`resolve_canonical`] walks the chain one hop at a time, tracking
//! `(module, name)` pairs it has already visited to guarantee termination
//! even when the source contains cyclic `export * from` loops.
//!
//! # Outcomes
//!
//! The walker never errors; the return value is always a
//! [`CanonicalResolution`]:
//!
//! - [`CanonicalResolution::Canonical`] — found a non-reexport declaration;
//! - [`CanonicalResolution::Unresolved`] — chain ended at a module the
//!   project doesn't own (package boundary, missing file, …);
//! - [`CanonicalResolution::Cycle`] — cycle detected; callers should emit a
//!   diagnostic and downgrade the edge to `Ambiguous`.

use std::collections::{HashMap, HashSet};

use crate::lang::ReExportEntry;

// ---------------------------------------------------------------------------
// ReExportGraph
// ---------------------------------------------------------------------------

/// One edge in the re-export graph: `(barrel, local_name)` → upstream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedReExport {
    /// Module the barrel forwards to, already resolved to dot-notation.
    pub upstream_module: String,
    /// `true` if `upstream_module` is a local (in-project) module.
    pub upstream_is_local: bool,
    /// Symbol name in the upstream module (before this barrel's alias).
    pub upstream_name: String,
    /// Line in the barrel file, 1-indexed.
    pub line: usize,
}

/// One `export * from …` edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StarReExport {
    pub upstream_module: String,
    pub upstream_is_local: bool,
    pub line: usize,
}

/// Outcome of [`ReExportGraph::resolve_canonical`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalResolution {
    /// Chain terminated at a module that does not re-export the name.
    ///
    /// `canonical_id` is the dot-notation id the consumer should be linked
    /// to (e.g. `src.entities.Course`). `alternative_paths` lists every
    /// intermediate barrel-scoped id the walker passed through (e.g.
    /// `src.domain.Course`), in walk order.
    Canonical {
        canonical_module: String,
        canonical_name: String,
        canonical_id: String,
        alternative_paths: Vec<String>,
    },
    /// Chain ended at a non-local module (external package, unknown file)
    /// before reaching a canonical declaration.
    ///
    /// `last_module` / `last_name` describe where the walk stopped;
    /// `alternative_paths` records every barrel-scoped id traversed.
    Unresolved {
        last_module: String,
        last_name: String,
        alternative_paths: Vec<String>,
    },
    /// Detected a cycle while walking — the chain revisits a
    /// `(module, name)` pair already seen.
    ///
    /// Callers should emit a diagnostic and degrade the edge to
    /// `Ambiguous`.
    Cycle {
        at_module: String,
        at_name: String,
        alternative_paths: Vec<String>,
    },
}

/// Re-export graph indexed by resolved module id.
#[derive(Default, Debug)]
pub struct ReExportGraph {
    /// `module_id → (local_name → NamedReExport)`
    named: HashMap<String, HashMap<String, NamedReExport>>,
    /// `module_id → Vec<StarReExport>`
    star: HashMap<String, Vec<StarReExport>>,
}

/// Callback signature used by [`ReExportGraph::build`] to resolve the raw
/// target of an `export … from …` statement.
///
/// The callback receives `(raw_target, from_module)` and must return the
/// resolved dot-notation module id plus whether that id is local. This
/// lets callers reuse [`crate::resolver::ModuleResolver::resolve`] without
/// this module depending on it directly.
pub type ResolveFn<'a> = dyn Fn(&str, &str) -> (String, bool) + 'a;

impl ReExportGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Populate from a flat list of [`ReExportEntry`] values collected
    /// during extraction. `resolve` translates each entry's `raw_target`
    /// into the project-wide canonical module id.
    pub fn build<'a>(entries: &[ReExportEntry], resolve: &ResolveFn<'a>) -> Self {
        let mut graph = ReExportGraph::new();
        for entry in entries {
            let (upstream_module, upstream_is_local) =
                resolve(&entry.raw_target, &entry.from_module);

            if entry.is_star {
                graph
                    .star
                    .entry(entry.from_module.clone())
                    .or_default()
                    .push(StarReExport {
                        upstream_module,
                        upstream_is_local,
                        line: entry.line,
                    });
                continue;
            }

            let named_bucket = graph.named.entry(entry.from_module.clone()).or_default();
            for spec in &entry.specs {
                named_bucket.insert(
                    spec.local_name.clone(),
                    NamedReExport {
                        upstream_module: upstream_module.clone(),
                        upstream_is_local,
                        upstream_name: spec.exported_name.clone(),
                        line: entry.line,
                    },
                );
            }
        }
        graph
    }

    /// Return the direct re-export (if any) for `(module, local_name)`.
    pub fn lookup(&self, module: &str, local_name: &str) -> Option<&NamedReExport> {
        self.named.get(module).and_then(|m| m.get(local_name))
    }

    /// Return every `export * from …` edge leaving `module`.
    pub fn star_edges(&self, module: &str) -> &[StarReExport] {
        self.star.get(module).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Walk the chain starting at `(from_module, local_name)`, returning the
    /// canonical declaration (or a diagnostic variant explaining why the
    /// walk stopped). See [`CanonicalResolution`].
    ///
    /// `is_local_module` is a callback that answers "is this dot-notation
    /// module id a local project module?" — needed because `export *` chains
    /// can only be meaningfully followed through local modules.
    pub fn resolve_canonical<F>(
        &self,
        from_module: &str,
        local_name: &str,
        is_local_module: &F,
    ) -> CanonicalResolution
    where
        F: Fn(&str) -> bool,
    {
        let mut visited: HashSet<(String, String)> = HashSet::new();
        let mut alternatives: Vec<String> = Vec::new();
        let mut cur_module = from_module.to_owned();
        let mut cur_name = local_name.to_owned();

        loop {
            // Cycle guard.
            let key = (cur_module.clone(), cur_name.clone());
            if !visited.insert(key) {
                return CanonicalResolution::Cycle {
                    at_module: cur_module,
                    at_name: cur_name,
                    alternative_paths: alternatives,
                };
            }

            // Every hop through a barrel becomes an alternative path.
            alternatives.push(format!("{}.{}", cur_module, cur_name));

            // Named re-export?
            if let Some(hop) = self.lookup(&cur_module, &cur_name) {
                // The walk terminates if the upstream isn't local — the
                // canonical declaration lives in a module the project
                // doesn't own. The barrel-scoped alt path we just pushed
                // stays in `alternatives` because it IS a legitimate
                // reachable id for the consumer.
                if !hop.upstream_is_local {
                    return CanonicalResolution::Unresolved {
                        last_module: hop.upstream_module.clone(),
                        last_name: hop.upstream_name.clone(),
                        alternative_paths: alternatives,
                    };
                }
                cur_module = hop.upstream_module.clone();
                cur_name = hop.upstream_name.clone();
                continue;
            }

            // Fall back to `export *` lookups: try each star edge; if the
            // upstream is local and exposes the name (directly or through
            // its own star chain), follow it.
            let mut matched_via_star: Option<(String, String)> = None;
            for star in self.star_edges(&cur_module) {
                if !star.upstream_is_local {
                    continue;
                }
                // Direct named re-export in the star target?
                if self.lookup(&star.upstream_module, &cur_name).is_some() {
                    matched_via_star = Some((star.upstream_module.clone(), cur_name.clone()));
                    break;
                }
                // The name might resolve via a deeper star chain from here.
                // We don't descend recursively here — callers that need
                // transitive star resolution should re-invoke
                // `resolve_canonical` starting at the star target.
            }

            if let Some((next_module, next_name)) = matched_via_star {
                cur_module = next_module;
                cur_name = next_name;
                continue;
            }

            // No hop available from (cur_module, cur_name) — we've hit a
            // terminal. Treat the last hop as canonical when it lives in a
            // local module; otherwise report Unresolved so callers keep the
            // existing barrel edge.
            let canonical_id = alternatives
                .pop()
                .unwrap_or_else(|| format!("{}.{}", cur_module, cur_name));

            if is_local_module(&cur_module) {
                return CanonicalResolution::Canonical {
                    canonical_module: cur_module,
                    canonical_name: cur_name,
                    canonical_id,
                    alternative_paths: alternatives,
                };
            }

            return CanonicalResolution::Unresolved {
                last_module: cur_module,
                last_name: cur_name,
                alternative_paths: alternatives,
            };
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{ReExportEntry, ReExportSpec};

    /// Build a resolver closure that treats `raw` as already-resolved (for
    /// tests) and marks every id starting with `app.` as local.
    fn id_resolver<'a>() -> impl Fn(&str, &str) -> (String, bool) + 'a {
        |raw: &str, _from: &str| {
            let is_local = raw.starts_with("app.");
            (raw.to_owned(), is_local)
        }
    }

    fn is_local_app(module: &str) -> bool {
        module.starts_with("app.")
    }

    fn spec(exported: &str, local: &str) -> ReExportSpec {
        ReExportSpec {
            exported_name: exported.to_owned(),
            local_name: local.to_owned(),
        }
    }

    fn entry(from: &str, target: &str, specs: Vec<ReExportSpec>, is_star: bool) -> ReExportEntry {
        ReExportEntry {
            from_module: from.to_owned(),
            raw_target: target.to_owned(),
            line: 1,
            specs,
            is_star,
        }
    }

    // -----------------------------------------------------------------------
    // Multi-level barrel chain (primary fixture)
    // -----------------------------------------------------------------------

    #[test]
    fn multi_level_barrel_chain_resolves_to_canonical() {
        // app.domain              re-exports Course from app.domain.entities
        // app.domain.entities     is the canonical declaration file
        let entries = vec![entry(
            "app.domain",
            "app.domain.entities",
            vec![spec("Course", "Course")],
            false,
        )];
        let resolve = id_resolver();
        let graph = ReExportGraph::build(&entries, &resolve);

        let result = graph.resolve_canonical("app.domain", "Course", &is_local_app);
        match result {
            CanonicalResolution::Canonical {
                canonical_module,
                canonical_name,
                canonical_id,
                alternative_paths,
            } => {
                assert_eq!(canonical_module, "app.domain.entities");
                assert_eq!(canonical_name, "Course");
                assert_eq!(canonical_id, "app.domain.entities.Course");
                assert_eq!(alternative_paths, vec!["app.domain.Course"]);
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn deep_barrel_chain_collects_all_alternatives() {
        // app.a  -> re-exports Course from app.b
        // app.b  -> re-exports Course from app.c
        // app.c  -> canonical declaration (no re-export entry)
        let entries = vec![
            entry("app.a", "app.b", vec![spec("Course", "Course")], false),
            entry("app.b", "app.c", vec![spec("Course", "Course")], false),
        ];
        let graph = ReExportGraph::build(&entries, &id_resolver());

        let result = graph.resolve_canonical("app.a", "Course", &is_local_app);
        match result {
            CanonicalResolution::Canonical {
                canonical_id,
                alternative_paths,
                ..
            } => {
                assert_eq!(canonical_id, "app.c.Course");
                assert_eq!(
                    alternative_paths,
                    vec!["app.a.Course", "app.b.Course"],
                    "every barrel hop must land in alternative_paths"
                );
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Aliased re-export
    // -----------------------------------------------------------------------

    #[test]
    fn aliased_reexport_preserves_canonical_name() {
        // app.barrel re-exports Course as Bar from app.entities
        let entries = vec![entry(
            "app.barrel",
            "app.entities",
            vec![spec("Course", "Bar")],
            false,
        )];
        let graph = ReExportGraph::build(&entries, &id_resolver());

        // Consumer imports `Bar` from the barrel.
        let result = graph.resolve_canonical("app.barrel", "Bar", &is_local_app);
        match result {
            CanonicalResolution::Canonical {
                canonical_module,
                canonical_name,
                canonical_id,
                alternative_paths,
            } => {
                assert_eq!(canonical_module, "app.entities");
                assert_eq!(
                    canonical_name, "Course",
                    "alias must not overwrite canonical name"
                );
                assert_eq!(canonical_id, "app.entities.Course");
                // The barrel-scoped alias id shows up in alternatives.
                assert_eq!(alternative_paths, vec!["app.barrel.Bar"]);
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn aliased_chain_carries_canonical_name_through_multiple_hops() {
        // a re-exports Bar as Bar from b
        // b re-exports Foo as Bar from c (renames mid-chain)
        // c is the canonical source of `Foo`
        let entries = vec![
            entry("app.a", "app.b", vec![spec("Bar", "Bar")], false),
            entry("app.b", "app.c", vec![spec("Foo", "Bar")], false),
        ];
        let graph = ReExportGraph::build(&entries, &id_resolver());

        let result = graph.resolve_canonical("app.a", "Bar", &is_local_app);
        match result {
            CanonicalResolution::Canonical {
                canonical_name,
                canonical_id,
                ..
            } => {
                assert_eq!(canonical_name, "Foo");
                assert_eq!(canonical_id, "app.c.Foo");
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Cycle detection
    // -----------------------------------------------------------------------

    #[test]
    fn cyclic_reexports_do_not_hang_and_return_cycle_variant() {
        // app.a re-exports Foo from app.b  (keeps name Foo)
        // app.b re-exports Foo from app.a  (cycle)
        let entries = vec![
            entry("app.a", "app.b", vec![spec("Foo", "Foo")], false),
            entry("app.b", "app.a", vec![spec("Foo", "Foo")], false),
        ];
        let graph = ReExportGraph::build(&entries, &id_resolver());

        let result = graph.resolve_canonical("app.a", "Foo", &is_local_app);
        match result {
            CanonicalResolution::Cycle {
                at_module,
                at_name,
                alternative_paths,
            } => {
                // The walker detected the revisit at (app.a, Foo).
                assert_eq!(at_module, "app.a");
                assert_eq!(at_name, "Foo");
                // Both barrel hops show up before the cycle terminates.
                assert_eq!(alternative_paths, vec!["app.a.Foo", "app.b.Foo"]);
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Non-local chain termination
    // -----------------------------------------------------------------------

    #[test]
    fn chain_into_external_package_is_unresolved() {
        // app.barrel re-exports anything from an external package —
        // upstream is not local, so walk stops with Unresolved.
        let entries = vec![entry(
            "app.barrel",
            "external.pkg",
            vec![spec("Thing", "Thing")],
            false,
        )];
        let graph = ReExportGraph::build(&entries, &id_resolver());

        let result = graph.resolve_canonical("app.barrel", "Thing", &is_local_app);
        match result {
            CanonicalResolution::Unresolved {
                last_module,
                last_name,
                alternative_paths,
            } => {
                assert_eq!(last_module, "external.pkg");
                assert_eq!(last_name, "Thing");
                // The barrel alias itself is still informative.
                assert_eq!(alternative_paths, vec!["app.barrel.Thing"]);
            }
            other => panic!("expected Unresolved, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // `export * from …`
    // -----------------------------------------------------------------------

    #[test]
    fn star_reexport_exposes_upstream_named_export() {
        // app.barrel re-exports * from app.entities
        // app.entities re-exports Course as Course from app.entities.course
        let entries = vec![
            entry("app.barrel", "app.entities", vec![], true),
            entry(
                "app.entities",
                "app.entities.course",
                vec![spec("Course", "Course")],
                false,
            ),
        ];
        let graph = ReExportGraph::build(&entries, &id_resolver());

        let result = graph.resolve_canonical("app.barrel", "Course", &is_local_app);
        match result {
            CanonicalResolution::Canonical {
                canonical_id,
                alternative_paths,
                ..
            } => {
                assert_eq!(canonical_id, "app.entities.course.Course");
                assert!(
                    alternative_paths.contains(&"app.barrel.Course".to_string()),
                    "star hop must contribute to alternative_paths, got {:?}",
                    alternative_paths
                );
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Direct declaration (no re-export chain)
    // -----------------------------------------------------------------------

    #[test]
    fn unrelated_module_has_no_chain_and_resolves_to_itself() {
        // No re-exports registered; asking for a name in app.x that isn't
        // forwarded returns Canonical at (app.x, Name) with no alternatives.
        let graph = ReExportGraph::build(&[], &id_resolver());

        let result = graph.resolve_canonical("app.x", "Name", &is_local_app);
        match result {
            CanonicalResolution::Canonical {
                canonical_module,
                canonical_name,
                canonical_id,
                alternative_paths,
            } => {
                assert_eq!(canonical_module, "app.x");
                assert_eq!(canonical_name, "Name");
                assert_eq!(canonical_id, "app.x.Name");
                assert!(alternative_paths.is_empty());
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn non_local_starting_module_is_unresolved() {
        // Queries that start at an external module (e.g. the consumer imported
        // a name from `react`) resolve immediately to Unresolved — there is no
        // local declaration to collapse to.
        let graph = ReExportGraph::build(&[], &id_resolver());

        let result = graph.resolve_canonical("react", "useState", &is_local_app);
        match result {
            CanonicalResolution::Unresolved {
                last_module,
                last_name,
                ..
            } => {
                assert_eq!(last_module, "react");
                assert_eq!(last_name, "useState");
            }
            other => panic!("expected Unresolved, got {other:?}"),
        }
    }
}
