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

use std::collections::HashMap;

use crate::lang::ReExportEntry;

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
}

impl ProjectReExportContext {
    /// Cheap constructor — useful in tests and in the collection site.
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
#[derive(Default, Debug, Clone)]
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
        self.projects.push(ctx);
    }

    /// Every project context in insertion order.
    pub fn projects(&self) -> &[ProjectReExportContext] {
        &self.projects
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
}
