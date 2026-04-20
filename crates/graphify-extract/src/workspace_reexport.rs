//! Workspace-scoped aggregation of per-project re-export data (FEAT-028, step 1).
//!
//! # Why this module exists
//!
//! FEAT-021 Part B shipped a **per-project** [`ReExportGraph`](crate::reexport_graph::ReExportGraph)
//! that collapses TypeScript barrel re-exports back to their canonical declarations,
//! and FEAT-026 extended that to module-level named-import fan-out. Both still stop
//! at the `[[project]]` boundary — the per-project graph in `graphify-cli::run_extract`
//! only sees one project's `all_reexports` / `known_modules` at a time.
//!
//! FEAT-027 confirmed the gap (see `tests/integration_test.rs::feat_027_cross_project_alias_stays_at_barrel_v1_contract`):
//! a consumer project importing `import { Foo } from '@repo/core'` (where `@repo/*`
//! is a tsconfig alias pointing at a sibling `[[project]]`) lands on the raw
//! `@repo/core` barrel node instead of the canonical `Foo` declaration in the
//! core project.
//!
//! This module introduces the workspace-scoped aggregation shape that a future
//! cross-project fan-out pass will consume. **It does not yet wire into the
//! extraction pipeline** — step 1 of the FEAT-028 task body. Steps 2–7
//! (namespacing decision, workspace-scoped `resolve_canonical`, alias →
//! workspace-target lookup, fan-out integration, and tripwire inversion) are
//! intentionally out of scope for this slice so the v1 contract is preserved
//! until the full feature lands.
//!
//! # Module-id namespacing (open question, step 2 in task body)
//!
//! Two candidates were discussed in the task body:
//!
//! 1. **Full prefix** — every node id becomes `{project_name}.{module_id}`
//!    (e.g. `consumer.src.main`, `core.src.foo`). Clean namespacing, but every
//!    existing consumer reading `graph.json` / `analysis.json` ids would break.
//!    Not backwards compatible.
//! 2. **Workspace lookup map** — public node ids stay `src.foo` etc. (one node
//!    per `(project, module_id)` in the per-project graphs), but a workspace
//!    registry keyed by `(project_name, module_id)` teaches a future
//!    `resolve_canonical_workspace` to walk across project boundaries.
//!
//! This scaffold commits to option (2): each [`ProjectReExportContext`]
//! carries its `project_name` and already-resolved `module_id`s without
//! renaming them. When the walker is wired up in a follow-up slice, it will
//! return `(project_name, canonical_module_id)` tuples — letting callers
//! emit cross-project edges while leaving intra-project node ids unchanged.
//! Revisiting option (1) would be a later, explicitly-gated decision.
//!
//! # Intended consumer shape (reserved for FEAT-028 follow-up)
//!
//! ```ignore
//! // In graphify-cli, BEFORE the per-project run_extract loop:
//! let mut workspace = WorkspaceReExportGraph::new();
//! for project in &cfg.project {
//!     let ctx = collect_project_context(project, ...);
//!     workspace.add_project(ctx);
//! }
//!
//! // Then per-project run_extract receives &workspace and, at the fan-out
//! // step, falls back to workspace.resolve_canonical_cross_project(...) when
//! // the per-project walker lands on a non-local raw alias that points into
//! // another project's root.
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::lang::ReExportEntry;
use crate::reexport_graph::ReExportGraph;

// ---------------------------------------------------------------------------
// Per-project context
// ---------------------------------------------------------------------------

/// One project's re-export inputs, collected before any project's extraction
/// pipeline emits edges.
///
/// This is a plain value-type aggregate — all fields are owned, cheap to clone
/// for tests, and free of references into the resolver so callers can hold a
/// workspace snapshot independently of the per-project mutable state.
///
/// Callers populate this from the same signals `run_extract` already uses:
///
/// - `project_name`: the `name` field on `[[project]]` (unique within a run).
/// - `repo_root`: the canonical path of the project's `repo = "./…"` directory,
///   used later by the cross-project alias lookup to decide whether a resolved
///   target path falls inside THIS project's root. Stored as a dot-normalised
///   string (platform-agnostic) so comparisons don't depend on `Path` internals.
/// - `known_modules`: the set of dot-notation ids the project's
///   [`ModuleResolver`](crate::resolver::ModuleResolver) recognises as local.
///   Pre-snapshotted rather than shared-by-reference because the aggregate
///   lives past the per-project resolver's mutable scope.
/// - `reexports`: the `all_reexports` vector collected during per-file
///   extraction (clone of what today feeds
///   [`ReExportGraph::build`](crate::reexport_graph::ReExportGraph::build)).
#[derive(Clone, Debug)]
pub struct ProjectReExportContext {
    /// Display name of the project, as declared in `graphify.toml`.
    pub project_name: String,
    /// Canonical (normalised, forward-slash) path of the project's repo root.
    ///
    /// Used by the future cross-project alias lookup: when
    /// [`crate::resolver::ModuleResolver::apply_ts_alias_with_context`] returns
    /// a raw alias string because the target path traversed OUT of the current
    /// project's root, the workspace walker can ask "does that resolved path
    /// fall inside any OTHER project's root?" to decide whether to cross the
    /// boundary.
    pub repo_root: String,
    /// Every module id this project treats as local (dot notation, post-prefix).
    pub known_modules: Vec<String>,
    /// Every `export … from …` entry captured during TypeScript extraction.
    pub reexports: Vec<ReExportEntry>,
    /// Path-based lookup keys for this project's discovered modules, mirroring
    /// [`crate::resolver::ModuleResolver::register_module_path`].
    ///
    /// Each entry is a `(normalised_path, module_id)` pair. Paths are stored
    /// in the same normalised, extension-stripped form the resolver uses, so
    /// the workspace-wide path index (built in
    /// [`WorkspaceReExportGraph::add_project`]) can answer "which project
    /// owns this resolved tsconfig-alias target path?" with O(1) lookup.
    ///
    /// Populated alongside `known_modules` from the walker's
    /// [`crate::walker::DiscoveredFile`] stream — see FEAT-028 step 4 for
    /// how the cross-project alias resolver consumes this.
    pub module_paths: Vec<(PathBuf, String)>,
}

impl ProjectReExportContext {
    /// Cheap constructor — useful in tests and in the collection site.
    ///
    /// Starts with an empty `module_paths`; callers populate it via
    /// [`ProjectReExportContext::add_module_path`] as the walker discovers
    /// files. The split constructor (rather than a single all-args variant)
    /// keeps the collection site readable: the caller loops over
    /// [`crate::walker::DiscoveredFile`] entries and appends each one, which
    /// matches the existing pattern used to populate
    /// [`crate::resolver::ModuleResolver::register_module_path`].
    pub fn new(
        project_name: impl Into<String>,
        repo_root: impl Into<String>,
        known_modules: Vec<String>,
        reexports: Vec<ReExportEntry>,
    ) -> Self {
        Self {
            project_name: project_name.into(),
            repo_root: repo_root.into(),
            known_modules,
            reexports,
            module_paths: Vec::new(),
        }
    }

    /// Record a `(file_path, module_id)` pair. Mirrors the side-effect shape
    /// of [`crate::resolver::ModuleResolver::register_module_path`]: the
    /// caller passes the discovered file's path and `is_package` flag; this
    /// helper normalises (extension-strip + canonical components) and, for
    /// package entry points, also records the parent-directory variant so a
    /// tsconfig alias target of `packages/core/src` (without `/index`)
    /// matches the `core.src.index` module.
    pub fn add_module_path(&mut self, module_id: &str, file_path: &Path, is_package: bool) {
        let without_ext = path_without_extension(file_path);
        self.module_paths
            .push((normalize_path(&without_ext), module_id.to_owned()));

        if is_package {
            if let Some(parent) = file_path.parent() {
                self.module_paths
                    .push((normalize_path(parent), module_id.to_owned()));
            }
        }
    }

    /// `true` if `module_id` is a local module in this project's scope.
    pub fn is_local_module(&self, module_id: &str) -> bool {
        // Linear scan is fine here — this is a diagnostic / lookup path used
        // once per cross-project hop, not per-file per-edge. Switch to a
        // `HashSet` if profiling shows pressure.
        self.known_modules.iter().any(|m| m == module_id)
    }
}

// Local path helpers — intentionally private duplicates of the ones in
// `resolver.rs` so this module can normalise paths consistently without
// widening that module's surface. Both are ~5 lines; if a third consumer
// shows up, promote to a shared `path_utils` submodule.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn path_without_extension(path: &Path) -> PathBuf {
    match (path.parent(), path.file_stem()) {
        (Some(parent), Some(stem)) => parent.join(stem),
        _ => path.to_path_buf(),
    }
}

// ---------------------------------------------------------------------------
// WorkspaceReExportGraph
// ---------------------------------------------------------------------------

/// Workspace-scoped aggregate of per-project re-export contexts.
///
/// This is the **scaffold** that step 1 of FEAT-028 introduces. It is
/// populated by `graphify-cli` before the per-project `run_extract` loop and
/// will be consumed by a future `resolve_canonical_cross_project` entry point
/// added in a subsequent slice.
///
/// Today's shape deliberately stops at **aggregation only**: no merging, no
/// cross-project walking, no alias rewriting. The one derived data structure
/// built eagerly is `modules_to_project` (a `module_id → project_name` index)
/// so callers can answer "which project owns this module id?" in O(1). That
/// index is enough to stub out the cross-project alias lookup without
/// committing to a naming scheme or a walker implementation.
///
/// Duplicates across projects (same `module_id` in two projects, e.g. both
/// have a `src.index` barrel) are **retained** — the index keeps the
/// first-wins entry and records collisions in `module_collisions` so a
/// future diagnostic pass can warn. This is deliberate: FEAT-028's step 2
/// still has to decide whether to prefix-namespace ids to disambiguate, so
/// this scaffold refuses to make that decision on behalf of the caller.
#[derive(Default, Debug)]
pub struct WorkspaceReExportGraph {
    /// Per-project contexts, in insertion order (matches config order).
    projects: Vec<ProjectReExportContext>,
    /// `module_id → project_name` — the first project to register wins.
    ///
    /// Used by the future cross-project walker to answer "which project
    /// declares this module?" without scanning every project.
    modules_to_project: HashMap<String, String>,
    /// Every collision observed while building `modules_to_project`, kept so
    /// a diagnostic pass can warn when the workspace has ambiguous module
    /// ids. Each tuple is `(module_id, project_that_lost)`.
    module_collisions: Vec<(String, String)>,
    /// Path → `(project_name, module_id)` index, built from every
    /// [`ProjectReExportContext::module_paths`] entry when the project is
    /// registered. Paths are normalised and extension-stripped, matching the
    /// per-project [`crate::resolver::ModuleResolver::module_lookup_paths`]
    /// shape so the workspace-aware alias resolver (FEAT-028 step 4) can
    /// answer "does this tsconfig-alias target land inside another
    /// registered project?" in O(1).
    ///
    /// First-wins, mirroring `modules_to_project`: if two projects both
    /// register the same path (shouldn't happen in a well-formed monorepo
    /// but defensive just in case), the first registration claims it.
    module_paths: HashMap<PathBuf, (String, String)>,
    /// Per-project re-export graphs keyed by `project_name`, populated via
    /// [`WorkspaceReExportGraph::set_project_graph`].
    ///
    /// `run_extract` already builds one [`ReExportGraph`] per project (via
    /// [`ReExportGraph::build`] with the project's `ModuleResolver` as the
    /// [`crate::reexport_graph::ResolveFn`]). FEAT-028 step 3 consumes that
    /// by letting the caller hand each pre-built graph into the workspace
    /// aggregate, so the cross-project walker can look up
    /// `(module, local_name)` hops in the owning project's graph without
    /// this module owning a `raw_target` resolution path.
    ///
    /// If a project has no graph registered (e.g. a non-TypeScript project),
    /// the walker treats barrel lookups in its modules as "no re-exports",
    /// which matches the per-project behaviour for an empty
    /// [`ReExportGraph`].
    project_graphs: HashMap<String, ReExportGraph>,
}

impl WorkspaceReExportGraph {
    /// Create an empty workspace aggregate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a project's context to the workspace. Projects are appended in
    /// call order; the first project to register a given module id wins the
    /// `modules_to_project` slot (losers are recorded in `module_collisions`).
    pub fn add_project(&mut self, ctx: ProjectReExportContext) {
        for module_id in &ctx.known_modules {
            if let Some(owner) = self.modules_to_project.get(module_id) {
                // Don't record a collision if the same project re-registers
                // the same id (defensive — shouldn't happen, but harmless).
                if owner != &ctx.project_name {
                    self.module_collisions
                        .push((module_id.clone(), ctx.project_name.clone()));
                }
            } else {
                self.modules_to_project
                    .insert(module_id.clone(), ctx.project_name.clone());
            }
        }
        for (path, module_id) in &ctx.module_paths {
            self.module_paths
                .entry(path.clone())
                .or_insert_with(|| (ctx.project_name.clone(), module_id.clone()));
        }
        self.projects.push(ctx);
    }

    /// Every project context in insertion order.
    pub fn projects(&self) -> &[ProjectReExportContext] {
        &self.projects
    }

    /// Look up which `(project_name, module_id)` owns a given resolved file
    /// path. The `candidate` path is normalised before lookup so callers can
    /// pass raw `base_dir.join(resolved)` output without pre-canonicalising.
    ///
    /// Tries the path as-given first, then the extension-stripped form, so
    /// a tsconfig alias target like `packages/core/src/foo` matches a
    /// registered `packages/core/src/foo.ts`. Returns `None` if no
    /// registered project owns the path — used by the workspace-aware alias
    /// resolver to decide whether to fall back to the raw-alias v1 contract.
    pub fn lookup_module_by_path(&self, candidate: &Path) -> Option<(String, String)> {
        let key = normalize_path(candidate);
        if let Some(hit) = self.module_paths.get(&key) {
            return Some(hit.clone());
        }
        let without_ext = normalize_path(&path_without_extension(&key));
        self.module_paths.get(&without_ext).cloned()
    }

    /// Look up which project owns a given dot-notation module id.
    ///
    /// Returns the `project_name` of the first project that registered it;
    /// `None` if no project owns the id (external module, typo, or the id
    /// belongs to a project that hasn't been added yet).
    pub fn project_for_module(&self, module_id: &str) -> Option<&str> {
        self.modules_to_project.get(module_id).map(String::as_str)
    }

    /// Look up a project's context by name.
    pub fn project(&self, project_name: &str) -> Option<&ProjectReExportContext> {
        self.projects
            .iter()
            .find(|p| p.project_name == project_name)
    }

    /// Collisions observed during `add_project` — `(module_id, losing_project)`.
    ///
    /// Non-empty results mean the workspace contains at least two projects
    /// that both publish the same module id (e.g. both have a `src.index`
    /// barrel). A future step will decide whether to namespace ids or to
    /// tolerate collisions; until then, this getter lets callers surface a
    /// `Warning:` on stderr.
    pub fn module_collisions(&self) -> &[(String, String)] {
        &self.module_collisions
    }

    /// `true` if the workspace has at least one project registered.
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    /// Number of registered projects.
    pub fn len(&self) -> usize {
        self.projects.len()
    }

    /// Attach a pre-built per-project [`ReExportGraph`] to the workspace.
    ///
    /// Callers own the graph's construction (see [`ReExportGraph::build`]
    /// with the project's [`crate::resolver::ModuleResolver`]) and hand the
    /// finished graph here so the cross-project walker can look up hops in
    /// any project's context without this module owning resolution of raw
    /// `export … from …` targets.
    ///
    /// Overwrites any previously-attached graph for the same `project_name`.
    pub fn set_project_graph(&mut self, project_name: impl Into<String>, graph: ReExportGraph) {
        self.project_graphs.insert(project_name.into(), graph);
    }

    /// Look up the per-project re-export graph for `project_name`.
    ///
    /// Returns `None` if no graph was attached for that project (e.g. a
    /// non-TypeScript project, or a TypeScript project that was registered
    /// via [`WorkspaceReExportGraph::add_project`] but whose graph hasn't
    /// been set yet).
    pub fn project_graph(&self, project_name: &str) -> Option<&ReExportGraph> {
        self.project_graphs.get(project_name)
    }

    /// Walk a re-export chain starting from `(from_project, from_module,
    /// spec_name)`, crossing project boundaries as needed.
    ///
    /// This is the workspace-scoped analogue of
    /// [`ReExportGraph::resolve_canonical`]. Hops are resolved in the
    /// current project's graph first; when a hop targets a module owned by
    /// another workspace project (per `modules_to_project`), the walker
    /// hands off to that project's graph and continues.
    ///
    /// # Cross-project locality
    ///
    /// "Local to the workspace" replaces "local to this project" — every
    /// module that any registered project owns counts as local. That matches
    /// the v1 policy sketched in FEAT-028 step 4: alias-through-barrel edges
    /// that cross a `[[project]]` boundary should still fan out to the
    /// canonical declaration instead of terminating at the barrel.
    ///
    /// # Termination & variants
    ///
    /// - [`CrossProjectResolution::Canonical`] — chain ends at a module that
    ///   is local to some workspace project and does not forward the name
    ///   any further. `project` / `module` / `symbol` are that endpoint's
    ///   workspace coordinates; `intermediates` lists every barrel-scoped
    ///   `(project, module, name)` triple traversed, in walk order.
    /// - [`CrossProjectResolution::Cycle`] — a `(project, module, name)`
    ///   triple was revisited. Participants are listed in visit order.
    /// - [`CrossProjectResolution::Unresolved`] — the walk ended at a
    ///   module no workspace project owns (external package, unknown file,
    ///   or a project whose graph isn't registered).
    ///
    /// The walker never panics and never loops indefinitely — the cycle
    /// guard uses a `HashSet` over the fully-qualified triple.
    pub fn resolve_canonical_cross_project(
        &self,
        from_project: &str,
        from_module: &str,
        spec_name: &str,
    ) -> CrossProjectResolution {
        let mut visited: HashSet<(String, String, String)> = HashSet::new();
        let mut intermediates: Vec<CrossProjectHop> = Vec::new();

        let mut cur_project = from_project.to_owned();
        let mut cur_module = from_module.to_owned();
        let mut cur_name = spec_name.to_owned();

        loop {
            // Cycle guard — the triple makes the same module+name in a
            // different project distinct, which is exactly what we want
            // when chains bounce between projects.
            let key = (cur_project.clone(), cur_module.clone(), cur_name.clone());
            if !visited.insert(key) {
                return CrossProjectResolution::Cycle {
                    participants: intermediates
                        .into_iter()
                        .chain(std::iter::once(CrossProjectHop {
                            project: cur_project,
                            module: cur_module,
                            symbol: cur_name,
                        }))
                        .collect(),
                };
            }

            // Every hop (including the very first one) becomes an
            // intermediate — matching the per-project walker where the
            // starting `(module, name)` becomes the first alternative path.
            intermediates.push(CrossProjectHop {
                project: cur_project.clone(),
                module: cur_module.clone(),
                symbol: cur_name.clone(),
            });

            // Look up in the current project's graph, if any.
            let named_hop = self
                .project_graphs
                .get(&cur_project)
                .and_then(|g| g.lookup(&cur_module, &cur_name))
                .cloned();

            if let Some(hop) = named_hop {
                // Is the upstream module local to ANY workspace project?
                if let Some(next_project) =
                    self.modules_to_project.get(&hop.upstream_module).cloned()
                {
                    cur_project = next_project;
                    cur_module = hop.upstream_module;
                    cur_name = hop.upstream_name;
                    continue;
                }

                // Upstream lives outside the workspace — terminate.
                // The barrel hop we just pushed is a legitimate
                // intermediate (parity with the per-project walker's
                // `chain_into_external_package_is_unresolved` where
                // `alternative_paths` keeps the barrel alias); `last`
                // captures the extra-workspace endpoint separately.
                return CrossProjectResolution::Unresolved {
                    last: CrossProjectHop {
                        project: cur_project,
                        module: hop.upstream_module,
                        symbol: hop.upstream_name,
                    },
                    intermediates,
                };
            }

            // Fall back to `export * from …` edges in the current project.
            let star_hop = self
                .project_graphs
                .get(&cur_project)
                .map(|g| g.star_edges(&cur_module))
                .unwrap_or(&[])
                .iter()
                .find_map(|star| {
                    // Only follow stars whose upstream is local to SOME
                    // workspace project AND where the upstream directly
                    // re-exports the name we're looking for. Matches the
                    // per-project policy: a star edge on its own doesn't
                    // prove the target has the name.
                    let next_project = self.modules_to_project.get(&star.upstream_module)?;
                    let graph = self.project_graphs.get(next_project)?;
                    graph.lookup(&star.upstream_module, &cur_name)?;
                    Some((next_project.clone(), star.upstream_module.clone()))
                });

            if let Some((next_project, next_module)) = star_hop {
                cur_project = next_project;
                cur_module = next_module;
                // `cur_name` unchanged — `export *` preserves names.
                continue;
            }

            // Terminal — no hop available. If the current module is
            // workspace-local, this is the canonical declaration; otherwise
            // the chain ran off the edge of the workspace (Unresolved).
            let is_workspace_local = self.modules_to_project.contains_key(&cur_module);
            let mut chain = intermediates;
            // The endpoint is captured separately in `Canonical { … }`;
            // drop the corresponding entry from `intermediates` so the
            // callsite sees only the intermediate barrel hops.
            chain.pop();

            if is_workspace_local {
                return CrossProjectResolution::Canonical {
                    project: cur_project,
                    module: cur_module,
                    symbol: cur_name,
                    intermediates: chain,
                };
            }

            return CrossProjectResolution::Unresolved {
                last: CrossProjectHop {
                    project: cur_project,
                    module: cur_module,
                    symbol: cur_name,
                },
                intermediates: chain,
            };
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace-aware alias target (FEAT-028 step 4)
// ---------------------------------------------------------------------------

/// Result of resolving a TypeScript tsconfig alias whose target path falls
/// **outside** the current project's root but **inside** another workspace
/// project's root.
///
/// Returned by
/// [`crate::resolver::ModuleResolver::apply_ts_alias_workspace`] — callers in
/// the FEAT-028 fan-out loop (step 5) use this to emit a cross-project edge
/// targeting the sibling project's canonical module id, instead of
/// terminating at the raw alias string as v1 did (the FEAT-027 tripwire).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceAliasTarget {
    /// Name of the sibling project that owns the resolved module.
    pub project: String,
    /// Dot-notation module id within that project (e.g. `src.index`).
    pub module_id: String,
}

// ---------------------------------------------------------------------------
// Cross-project walker outcome
// ---------------------------------------------------------------------------

/// One step in a cross-project re-export chain.
///
/// Scoped by `project` so the workspace walker can disambiguate cases where
/// two projects both publish the same `module_id` (a collision already
/// recorded in [`WorkspaceReExportGraph::module_collisions`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossProjectHop {
    /// The `project_name` of the project that owns `module` at this hop.
    pub project: String,
    /// The dot-notation module id within that project.
    pub module: String,
    /// The symbol name as seen at this hop (pre-alias-rewrite).
    pub symbol: String,
}

/// Outcome of [`WorkspaceReExportGraph::resolve_canonical_cross_project`].
///
/// Mirrors the per-project
/// [`CanonicalResolution`](crate::reexport_graph::CanonicalResolution)
/// shape but carries `(project, module, symbol)` triples at every position
/// so callers can emit cross-project edges that target the right project's
/// canonical node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CrossProjectResolution {
    /// Chain terminated at a workspace-local module that does not forward
    /// the name any further. The endpoint is the canonical declaration;
    /// `intermediates` lists every barrel-scoped hop (each with its own
    /// owning project) traversed in walk order.
    Canonical {
        project: String,
        module: String,
        symbol: String,
        intermediates: Vec<CrossProjectHop>,
    },
    /// Chain ended at a module no workspace project owns — either an
    /// external package or a project whose graph is not registered.
    Unresolved {
        last: CrossProjectHop,
        intermediates: Vec<CrossProjectHop>,
    },
    /// A `(project, module, name)` triple was revisited. `participants`
    /// lists the full cycle including the revisit point.
    Cycle { participants: Vec<CrossProjectHop> },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{ReExportEntry, ReExportSpec};

    fn ctx(
        name: &str,
        root: &str,
        modules: &[&str],
        reexports: Vec<ReExportEntry>,
    ) -> ProjectReExportContext {
        ProjectReExportContext::new(
            name,
            root,
            modules.iter().map(|s| (*s).to_owned()).collect(),
            reexports,
        )
    }

    fn entry(from: &str, target: &str) -> ReExportEntry {
        ReExportEntry {
            from_module: from.to_owned(),
            raw_target: target.to_owned(),
            line: 1,
            specs: vec![ReExportSpec {
                exported_name: "Foo".to_owned(),
                local_name: "Foo".to_owned(),
            }],
            is_star: false,
        }
    }

    // -----------------------------------------------------------------------
    // ProjectReExportContext
    // -----------------------------------------------------------------------

    #[test]
    fn project_context_round_trip_preserves_fields() {
        let reexports = vec![entry("src.index", "./foo")];
        let c = ctx(
            "core",
            "/abs/packages/core",
            &["src.index", "src.foo"],
            reexports.clone(),
        );
        assert_eq!(c.project_name, "core");
        assert_eq!(c.repo_root, "/abs/packages/core");
        assert_eq!(c.known_modules, vec!["src.index", "src.foo"]);
        assert_eq!(c.reexports, reexports);
    }

    #[test]
    fn project_context_is_local_module_checks_known_modules() {
        let c = ctx("core", "/abs/core", &["src.index", "src.foo"], vec![]);
        assert!(c.is_local_module("src.index"));
        assert!(c.is_local_module("src.foo"));
        assert!(!c.is_local_module("src.bar"));
        assert!(!c.is_local_module("@repo/core"));
    }

    // -----------------------------------------------------------------------
    // WorkspaceReExportGraph — aggregation invariants
    // -----------------------------------------------------------------------

    #[test]
    fn empty_workspace_has_no_projects_and_no_owners() {
        let ws = WorkspaceReExportGraph::new();
        assert!(ws.is_empty());
        assert_eq!(ws.len(), 0);
        assert_eq!(ws.projects().len(), 0);
        assert!(ws.project_for_module("src.anything").is_none());
        assert!(ws.module_collisions().is_empty());
    }

    #[test]
    fn add_project_indexes_every_known_module_to_its_project() {
        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx("consumer", "/abs/apps/consumer", &["src.main"], vec![]));
        ws.add_project(ctx(
            "core",
            "/abs/packages/core",
            &["src.index", "src.foo"],
            vec![entry("src.index", "./foo")],
        ));

        assert_eq!(ws.len(), 2);
        assert_eq!(ws.project_for_module("src.main"), Some("consumer"));
        assert_eq!(ws.project_for_module("src.index"), Some("core"));
        assert_eq!(ws.project_for_module("src.foo"), Some("core"));
        assert_eq!(ws.project_for_module("src.nope"), None);
    }

    #[test]
    fn projects_are_retained_in_insertion_order() {
        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx("a", "/a", &["src.a"], vec![]));
        ws.add_project(ctx("b", "/b", &["src.b"], vec![]));
        ws.add_project(ctx("c", "/c", &["src.c"], vec![]));

        let names: Vec<&str> = ws
            .projects()
            .iter()
            .map(|p| p.project_name.as_str())
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn project_lookup_by_name_returns_context() {
        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx("consumer", "/abs/apps/consumer", &["src.main"], vec![]));
        ws.add_project(ctx(
            "core",
            "/abs/packages/core",
            &["src.index"],
            vec![entry("src.index", "./foo")],
        ));

        let core = ws.project("core").expect("core project");
        assert_eq!(core.repo_root, "/abs/packages/core");
        assert_eq!(core.reexports.len(), 1);

        assert!(ws.project("missing").is_none());
    }

    // -----------------------------------------------------------------------
    // Collision handling (feeds step-2 namespacing decision)
    // -----------------------------------------------------------------------

    #[test]
    fn duplicate_module_id_keeps_first_owner_and_records_collision() {
        // Both `consumer` and `core` declare `src.index` — classic monorepo
        // barrel collision. First-wins keeps the index deterministic; the
        // loser lands in `module_collisions` so a future diagnostic can warn.
        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx("consumer", "/c", &["src.index", "src.main"], vec![]));
        ws.add_project(ctx("core", "/k", &["src.index", "src.foo"], vec![]));

        // First-wins: consumer claims `src.index`.
        assert_eq!(ws.project_for_module("src.index"), Some("consumer"));
        // Non-colliding ids land on their real owners.
        assert_eq!(ws.project_for_module("src.main"), Some("consumer"));
        assert_eq!(ws.project_for_module("src.foo"), Some("core"));

        // Collision recorded with the losing project's name.
        assert_eq!(
            ws.module_collisions(),
            &[("src.index".to_owned(), "core".to_owned())],
        );
    }

    #[test]
    fn same_project_reregistering_same_id_is_not_a_collision() {
        // Defensive: if the same project somehow registers the same module id
        // twice, that's not a cross-project collision. (Shouldn't happen in
        // practice — `known_modules` comes from a HashMap — but guarding the
        // behaviour avoids a confusing false-positive warning later.)
        let mut ws = WorkspaceReExportGraph::new();
        let c = ProjectReExportContext::new(
            "solo",
            "/solo",
            vec!["src.dup".to_owned(), "src.dup".to_owned()],
            vec![],
        );
        ws.add_project(c);

        assert_eq!(ws.project_for_module("src.dup"), Some("solo"));
        assert!(ws.module_collisions().is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_canonical_cross_project — FEAT-028 step 3
    // -----------------------------------------------------------------------

    /// Small helper: build a per-project [`ReExportGraph`] from a flat list
    /// of entries, treating each `raw_target` as already-resolved. The
    /// resulting graph's `upstream_is_local` flag is unused by the
    /// cross-project walker (locality is re-derived from
    /// `modules_to_project`), so we hard-code `true` here to keep the
    /// fixture obvious.
    fn build_identity_graph(entries: &[ReExportEntry]) -> ReExportGraph {
        let resolve = |raw: &str, _from: &str| (raw.to_owned(), true);
        ReExportGraph::build(entries, &resolve)
    }

    fn named_entry(from: &str, target: &str, specs: &[(&str, &str)]) -> ReExportEntry {
        ReExportEntry {
            from_module: from.to_owned(),
            raw_target: target.to_owned(),
            line: 1,
            specs: specs
                .iter()
                .map(|(exported, local)| ReExportSpec {
                    exported_name: (*exported).to_owned(),
                    local_name: (*local).to_owned(),
                })
                .collect(),
            is_star: false,
        }
    }

    fn star_entry(from: &str, target: &str) -> ReExportEntry {
        ReExportEntry {
            from_module: from.to_owned(),
            raw_target: target.to_owned(),
            line: 1,
            specs: vec![],
            is_star: true,
        }
    }

    fn hop(project: &str, module: &str, symbol: &str) -> CrossProjectHop {
        CrossProjectHop {
            project: project.to_owned(),
            module: module.to_owned(),
            symbol: symbol.to_owned(),
        }
    }

    // --- Parity case: single-hop, same project ------------------------------

    #[test]
    fn cross_project_walker_same_project_single_hop_mirrors_per_project_canonical() {
        // Only one project registered. `core.src.index` re-exports `Foo`
        // from `core.src.foo`. Walker should land on
        // `(core, core.src.foo, Foo)` just like the per-project walker.
        let entries = vec![named_entry(
            "core.src.index",
            "core.src.foo",
            &[("Foo", "Foo")],
        )];
        let graph = build_identity_graph(&entries);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx(
            "core",
            "/abs/core",
            &["core.src.index", "core.src.foo"],
            entries,
        ));
        ws.set_project_graph("core", graph);

        let result = ws.resolve_canonical_cross_project("core", "core.src.index", "Foo");
        match result {
            CrossProjectResolution::Canonical {
                project,
                module,
                symbol,
                intermediates,
            } => {
                assert_eq!(project, "core");
                assert_eq!(module, "core.src.foo");
                assert_eq!(symbol, "Foo");
                assert_eq!(intermediates, vec![hop("core", "core.src.index", "Foo")]);
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // --- Primary case: single-hop cross-project -----------------------------

    #[test]
    fn cross_project_walker_crosses_project_boundary_on_named_hop() {
        // `consumer.src.index` re-exports `Foo` from `core.src.foo`
        // (a module owned by a different project). The walker should
        // cross the boundary and return the canonical coordinates in
        // `core`.
        let consumer_entries = vec![named_entry(
            "consumer.src.index",
            "core.src.foo",
            &[("Foo", "Foo")],
        )];
        let consumer_graph = build_identity_graph(&consumer_entries);
        let core_graph = build_identity_graph(&[]);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx(
            "consumer",
            "/abs/consumer",
            &["consumer.src.index"],
            consumer_entries,
        ));
        ws.add_project(ctx("core", "/abs/core", &["core.src.foo"], vec![]));
        ws.set_project_graph("consumer", consumer_graph);
        ws.set_project_graph("core", core_graph);

        let result = ws.resolve_canonical_cross_project("consumer", "consumer.src.index", "Foo");
        match result {
            CrossProjectResolution::Canonical {
                project,
                module,
                symbol,
                intermediates,
            } => {
                assert_eq!(project, "core", "must cross into core project");
                assert_eq!(module, "core.src.foo");
                assert_eq!(symbol, "Foo");
                assert_eq!(
                    intermediates,
                    vec![hop("consumer", "consumer.src.index", "Foo")],
                    "the consumer barrel hop should be recorded as intermediate"
                );
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // --- Multi-hop cross-project --------------------------------------------

    #[test]
    fn cross_project_walker_multi_hop_chain_collects_every_intermediate() {
        // consumer barrel -> shared barrel (different project) -> core leaf
        // (yet another project). Walker should traverse all three and
        // return the canonical id in `core`, with both intermediate barrel
        // hops recorded in order.
        let consumer_entries = vec![named_entry(
            "consumer.src.index",
            "shared.src.index",
            &[("Foo", "Foo")],
        )];
        let shared_entries = vec![named_entry(
            "shared.src.index",
            "core.src.foo",
            &[("Foo", "Foo")],
        )];
        let consumer_graph = build_identity_graph(&consumer_entries);
        let shared_graph = build_identity_graph(&shared_entries);
        let core_graph = build_identity_graph(&[]);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx(
            "consumer",
            "/abs/consumer",
            &["consumer.src.index"],
            consumer_entries,
        ));
        ws.add_project(ctx(
            "shared",
            "/abs/shared",
            &["shared.src.index"],
            shared_entries,
        ));
        ws.add_project(ctx("core", "/abs/core", &["core.src.foo"], vec![]));
        ws.set_project_graph("consumer", consumer_graph);
        ws.set_project_graph("shared", shared_graph);
        ws.set_project_graph("core", core_graph);

        let result = ws.resolve_canonical_cross_project("consumer", "consumer.src.index", "Foo");
        match result {
            CrossProjectResolution::Canonical {
                project,
                module,
                symbol,
                intermediates,
            } => {
                assert_eq!(project, "core");
                assert_eq!(module, "core.src.foo");
                assert_eq!(symbol, "Foo");
                assert_eq!(
                    intermediates,
                    vec![
                        hop("consumer", "consumer.src.index", "Foo"),
                        hop("shared", "shared.src.index", "Foo"),
                    ],
                    "both barrel hops, in order, must appear as intermediates"
                );
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // --- Cycle across project boundary --------------------------------------

    #[test]
    fn cross_project_walker_detects_cycle_spanning_two_projects() {
        // a.barrel re-exports Foo from b.barrel; b.barrel re-exports Foo
        // from a.barrel. Both live in different projects. Walker must
        // return Cycle rather than looping forever.
        let a_entries = vec![named_entry("a.barrel", "b.barrel", &[("Foo", "Foo")])];
        let b_entries = vec![named_entry("b.barrel", "a.barrel", &[("Foo", "Foo")])];
        let a_graph = build_identity_graph(&a_entries);
        let b_graph = build_identity_graph(&b_entries);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx("a", "/abs/a", &["a.barrel"], a_entries));
        ws.add_project(ctx("b", "/abs/b", &["b.barrel"], b_entries));
        ws.set_project_graph("a", a_graph);
        ws.set_project_graph("b", b_graph);

        let result = ws.resolve_canonical_cross_project("a", "a.barrel", "Foo");
        match result {
            CrossProjectResolution::Cycle { participants } => {
                // The full cycle is traversed: a -> b -> back to a (revisit).
                assert_eq!(
                    participants,
                    vec![
                        hop("a", "a.barrel", "Foo"),
                        hop("b", "b.barrel", "Foo"),
                        hop("a", "a.barrel", "Foo"),
                    ],
                    "participants must include the revisit point"
                );
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    // --- Unresolved (external package / non-workspace target) ---------------

    #[test]
    fn cross_project_walker_terminates_unresolved_when_target_outside_workspace() {
        // Consumer barrel re-exports `Foo` from an external npm package
        // (not owned by any workspace project). Walker must terminate
        // with Unresolved and record the barrel hop as intermediate.
        let consumer_entries = vec![named_entry(
            "consumer.src.index",
            "external.pkg",
            &[("Foo", "Foo")],
        )];
        let consumer_graph = build_identity_graph(&consumer_entries);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx(
            "consumer",
            "/abs/consumer",
            &["consumer.src.index"],
            consumer_entries,
        ));
        ws.set_project_graph("consumer", consumer_graph);

        let result = ws.resolve_canonical_cross_project("consumer", "consumer.src.index", "Foo");
        match result {
            CrossProjectResolution::Unresolved {
                last,
                intermediates,
            } => {
                assert_eq!(last, hop("consumer", "external.pkg", "Foo"));
                assert_eq!(
                    intermediates,
                    vec![hop("consumer", "consumer.src.index", "Foo")],
                );
            }
            other => panic!("expected Unresolved, got {other:?}"),
        }
    }

    #[test]
    fn cross_project_walker_terminal_module_with_no_hops_is_canonical_itself() {
        // `consumer.src.main` is local but has no re-exports for `Bar`.
        // With no barrel hop to take, the walker treats `(consumer,
        // consumer.src.main, Bar)` as the canonical declaration (it's
        // workspace-local and the chain has nowhere to go). This mirrors
        // `unrelated_module_has_no_chain_and_resolves_to_itself` on the
        // per-project walker.
        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx(
            "consumer",
            "/abs/consumer",
            &["consumer.src.main"],
            vec![],
        ));
        ws.set_project_graph("consumer", build_identity_graph(&[]));

        let result = ws.resolve_canonical_cross_project("consumer", "consumer.src.main", "Bar");
        match result {
            CrossProjectResolution::Canonical {
                project,
                module,
                symbol,
                intermediates,
            } => {
                assert_eq!(project, "consumer");
                assert_eq!(module, "consumer.src.main");
                assert_eq!(symbol, "Bar");
                assert!(intermediates.is_empty());
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    #[test]
    fn cross_project_walker_unresolved_when_starting_module_not_in_workspace() {
        // Starting at a module no project owns (e.g. `react.useState`)
        // resolves immediately to Unresolved — mirroring the per-project
        // `non_local_starting_module_is_unresolved`.
        let ws = WorkspaceReExportGraph::new();
        let result = ws.resolve_canonical_cross_project("nobody", "react", "useState");
        match result {
            CrossProjectResolution::Unresolved {
                last,
                intermediates,
            } => {
                assert_eq!(last, hop("nobody", "react", "useState"));
                assert!(intermediates.is_empty());
            }
            other => panic!("expected Unresolved, got {other:?}"),
        }
    }

    // --- `export * from …` crossing a project boundary ----------------------

    #[test]
    fn cross_project_walker_follows_star_reexport_across_project_boundary() {
        // consumer.src.index does `export * from 'core.src.index'`
        // (cross-project). core.src.index re-exports Foo from core.src.foo.
        // Walker must follow the star hop into core, then the named hop
        // to core.src.foo.
        let consumer_entries = vec![star_entry("consumer.src.index", "core.src.index")];
        let core_entries = vec![named_entry(
            "core.src.index",
            "core.src.foo",
            &[("Foo", "Foo")],
        )];
        let consumer_graph = build_identity_graph(&consumer_entries);
        let core_graph = build_identity_graph(&core_entries);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(ctx(
            "consumer",
            "/abs/consumer",
            &["consumer.src.index"],
            consumer_entries,
        ));
        ws.add_project(ctx(
            "core",
            "/abs/core",
            &["core.src.index", "core.src.foo"],
            core_entries,
        ));
        ws.set_project_graph("consumer", consumer_graph);
        ws.set_project_graph("core", core_graph);

        let result = ws.resolve_canonical_cross_project("consumer", "consumer.src.index", "Foo");
        match result {
            CrossProjectResolution::Canonical {
                project,
                module,
                symbol,
                intermediates,
            } => {
                assert_eq!(project, "core");
                assert_eq!(module, "core.src.foo");
                assert_eq!(symbol, "Foo");
                assert_eq!(
                    intermediates,
                    vec![
                        hop("consumer", "consumer.src.index", "Foo"),
                        hop("core", "core.src.index", "Foo"),
                    ],
                );
            }
            other => panic!("expected Canonical, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // FEAT-028 step 4 — path-based module lookup
    // -----------------------------------------------------------------------

    #[test]
    fn add_module_path_records_extension_stripped_form() {
        let mut c = ctx("core", "/abs/core", &["core.src.foo"], vec![]);
        c.add_module_path(
            "core.src.foo",
            Path::new("/abs/core/src/foo.ts"),
            /* is_package */ false,
        );
        assert_eq!(
            c.module_paths,
            vec![(
                PathBuf::from("/abs/core/src/foo"),
                "core.src.foo".to_owned()
            )],
        );
    }

    #[test]
    fn add_module_path_for_package_records_parent_too() {
        // Package entry points (`index.ts` / `__init__.py`) need the parent
        // directory variant so a tsconfig alias target of
        // `packages/core/src` (no `/index`) matches the `core.src.index`
        // module id — mirrors the resolver's side-effect in
        // `register_module_path`.
        let mut c = ctx("core", "/abs/core", &["core.src.index"], vec![]);
        c.add_module_path(
            "core.src.index",
            Path::new("/abs/core/src/index.ts"),
            /* is_package */ true,
        );
        assert_eq!(
            c.module_paths,
            vec![
                (
                    PathBuf::from("/abs/core/src/index"),
                    "core.src.index".to_owned(),
                ),
                (PathBuf::from("/abs/core/src"), "core.src.index".to_owned(),),
            ],
        );
    }

    #[test]
    fn workspace_lookup_by_path_returns_project_and_module_id() {
        let mut core = ctx("core", "/abs/core", &["core.src.index"], vec![]);
        core.add_module_path("core.src.index", Path::new("/abs/core/src/index.ts"), true);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(core);

        // Exact extension-stripped hit.
        assert_eq!(
            ws.lookup_module_by_path(Path::new("/abs/core/src/index")),
            Some(("core".to_owned(), "core.src.index".to_owned())),
        );
        // Parent-directory hit (the package variant).
        assert_eq!(
            ws.lookup_module_by_path(Path::new("/abs/core/src")),
            Some(("core".to_owned(), "core.src.index".to_owned())),
        );
        // Extension present — resolver falls back to stripped form.
        assert_eq!(
            ws.lookup_module_by_path(Path::new("/abs/core/src/index.ts")),
            Some(("core".to_owned(), "core.src.index".to_owned())),
        );
    }

    #[test]
    fn workspace_lookup_by_path_returns_none_when_path_not_registered() {
        let mut core = ctx("core", "/abs/core", &["core.src.index"], vec![]);
        core.add_module_path("core.src.index", Path::new("/abs/core/src/index.ts"), true);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(core);

        assert!(ws
            .lookup_module_by_path(Path::new("/abs/other/src/index"))
            .is_none(),);
        assert!(ws
            .lookup_module_by_path(Path::new("/abs/core/src/missing"))
            .is_none(),);
    }

    #[test]
    fn workspace_lookup_by_path_first_project_wins_on_collision() {
        // Two projects somehow register the same absolute path — first wins,
        // defensive behaviour mirroring `modules_to_project`.
        let mut a = ctx("a", "/abs/a", &["a.src"], vec![]);
        a.add_module_path("a.src", Path::new("/shared/path.ts"), false);
        let mut b = ctx("b", "/abs/b", &["b.src"], vec![]);
        b.add_module_path("b.src", Path::new("/shared/path.ts"), false);

        let mut ws = WorkspaceReExportGraph::new();
        ws.add_project(a);
        ws.add_project(b);

        assert_eq!(
            ws.lookup_module_by_path(Path::new("/shared/path.ts")),
            Some(("a".to_owned(), "a.src".to_owned())),
        );
    }
}
