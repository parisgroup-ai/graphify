# FEAT-043 — `graphify suggest stubs` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `graphify suggest stubs` — a post-analysis subcommand that scans `graph.json` from each configured project, groups external references by language-aware prefix, auto-classifies them as cross-project (settings) or single-project candidates, and emits md/toml/json output or applies the additions in place to `graphify.toml`.

**Architecture:** Pure renderer + types in `crates/graphify-report/src/suggest.rs` (no I/O, no extractor deps). Thin orchestration in `crates/graphify-cli/src/main.rs` (`cmd_suggest_stubs`). New `Commands::Suggest { kind: SuggestKind }` enum opens the `suggest <kind>` namespace; `stubs` is the only kind shipped here. Consumes existing `graph.json` artifacts (NOT `analysis.json` — `graph.json` carries `is_local` per node and `weight` per edge, both required for filtering and ranking).

**Tech Stack:** Rust 2021, `serde`/`serde_json` (existing), `clap` (existing), `toml_edit 0.22` (new — preserves comments/order on round-trip).

**Spec:** `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `Cargo.toml` (workspace) | Modify | Add `toml_edit = "0.22"` to `[workspace.dependencies]` |
| `crates/graphify-cli/Cargo.toml` | Modify | Add `toml_edit = { workspace = true }` to `[dependencies]` |
| `crates/graphify-report/src/suggest.rs` | Create | Pure types (`GraphSnapshot`, `StubCandidate`, `SuggestReport`), `extract_prefix`, `score_stubs`, `render_markdown`, `render_toml`, `render_json` + unit tests |
| `crates/graphify-report/src/lib.rs` | Modify | `pub mod suggest;` + re-export key types |
| `crates/graphify-cli/src/main.rs` | Modify | Add `Commands::Suggest`, `SuggestKind` enum, `cmd_suggest_stubs` orchestration |
| `crates/graphify-cli/tests/fixtures/suggest/graphify.toml` | Create | Tiny 2-project fixture |
| `crates/graphify-cli/tests/fixtures/suggest/proj-a/graph.json` | Create | Hand-authored graph with known externals (e.g. `tokio`, `serde`, `rmcp`) |
| `crates/graphify-cli/tests/fixtures/suggest/proj-b/graph.json` | Create | Hand-authored graph sharing `tokio`+`serde`, plus its own `clap` |
| `crates/graphify-cli/tests/suggest_integration.rs` | Create | End-to-end CLI tests (read-only + `--apply`) |
| `README.md` | Modify | Add subsection under existing `## Commands` describing `graphify suggest stubs` |
| `CHANGELOG.md` | Modify | Add entry under `## [Unreleased]` |

---

## Task 1: Add `toml_edit` workspace dependency

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/graphify-cli/Cargo.toml`

- [ ] **Step 1: Add `toml_edit` to workspace deps**

In `Cargo.toml`, locate the `[workspace.dependencies]` section (around line 18) and add the line `toml_edit = "0.22"` after `which = "6"`:

```toml
[workspace.dependencies]
graphify-core = { version = "0.13.1", path = "crates/graphify-core" }
graphify-extract = { version = "0.13.1", path = "crates/graphify-extract" }
graphify-report = { version = "0.13.1", path = "crates/graphify-report" }
tempfile = "3"
serde_json = "1"
serde_yaml = "0.9"
sha2 = "0.10"
include_dir = "0.7"
thiserror = "1"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
dirs = "5"
which = "6"
toml_edit = "0.22"
```

- [ ] **Step 2: Pull `toml_edit` into the CLI crate**

In `crates/graphify-cli/Cargo.toml`, add to `[dependencies]` (after the existing `toml = "0.8"` line):

```toml
toml_edit = { workspace = true }
```

- [ ] **Step 3: Sanity build**

Run: `cargo build --workspace`
Expected: clean build, `toml_edit` downloaded and compiled.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/graphify-cli/Cargo.toml
git commit -m "build: add toml_edit 0.22 workspace dep for FEAT-043"
```

---

## Task 2: Create `suggest.rs` skeleton with types and `extract_prefix`

**Files:**
- Create: `crates/graphify-report/src/suggest.rs`
- Modify: `crates/graphify-report/src/lib.rs` (add `pub mod suggest;`)

- [ ] **Step 1: Wire the module into `lib.rs`**

In `crates/graphify-report/src/lib.rs`, add `pub mod suggest;` to the `pub mod` list (alphabetical placement, after `pub mod smells;`):

```rust
pub mod smells;
pub mod suggest;
pub mod trend_json;
```

- [ ] **Step 2: Create `suggest.rs` with types only (no logic yet)**

Create `crates/graphify-report/src/suggest.rs`:

```rust
//! FEAT-043 — `graphify suggest stubs` core types + scoring + renderers.
//!
//! Pure module: no I/O, no extractor deps. Consumes a `GraphSnapshot`
//! (deserialised from `graph.json`) per project plus the project's
//! already-merged `ExternalStubs`, and emits a `SuggestReport` describing
//! candidate prefixes to add to `[settings].external_stubs` or
//! `[[project]].external_stubs`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Input: subset of graph.json the suggester needs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphNode {
    pub id: String,
    /// Debug-formatted `Language` enum: "Rust", "Python", "TypeScript",
    /// "Php", "Go". Matched verbatim by `extract_prefix`.
    pub language: String,
    pub is_local: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
    pub weight: u32,
}

pub struct ProjectInput<'a> {
    pub name: &'a str,
    pub local_prefix: &'a str,
    pub current_stubs: &'a graphify_extract::stubs::ExternalStubs,
    pub graph: &'a GraphSnapshot,
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StubCandidate {
    pub prefix: String,
    pub language: String,
    pub edge_weight: u64,
    pub node_count: usize,
    pub projects: Vec<String>,
    pub example_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestReport {
    pub min_edges: u64,
    pub settings_candidates: Vec<StubCandidate>,
    pub per_project_candidates: BTreeMap<String, Vec<StubCandidate>>,
    pub already_covered_prefixes: Vec<String>,
    pub shadowed_prefixes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Prefix extraction (language-aware)
// ---------------------------------------------------------------------------

/// Extracts the natural stub prefix from a node id, language-aware.
///
/// Rules per language (matches `language` string from `GraphNode`):
/// - `Rust`: first segment before `::`
/// - `Python`: first segment before `.`
/// - `Php`: first segment before `.` (PSR-4 already normalised `\` → `.`)
/// - `TypeScript`: if id starts with `@`, take two `/`-separated segments
///   (`@scope/name`); otherwise first `/`-separated segment.
/// - `Go`: if first `/`-separated segment contains `.` (path-style like
///   `github.com/spf13/cobra`), take first 3 `/`-segments. Otherwise
///   first `.`-separated segment (like `fmt.Println`).
/// - Anything else (unknown language string): return the id verbatim.
///
/// Returns `None` only for an empty id; otherwise returns at least
/// the input itself.
pub fn extract_prefix(node_id: &str, language: &str) -> Option<String> {
    let trimmed = node_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    match language {
        "Rust" => Some(trimmed.split("::").next().unwrap().to_string()),
        "Python" | "Php" => Some(trimmed.split('.').next().unwrap().to_string()),
        "TypeScript" => {
            if let Some(without_at) = trimmed.strip_prefix('@') {
                let mut parts = without_at.splitn(3, '/');
                match (parts.next(), parts.next()) {
                    (Some(scope), Some(pkg)) if !scope.is_empty() && !pkg.is_empty() => {
                        Some(format!("@{}/{}", scope, pkg))
                    }
                    _ => Some(trimmed.to_string()),
                }
            } else {
                Some(trimmed.split('/').next().unwrap().to_string())
            }
        }
        "Go" => {
            let parts: Vec<&str> = trimmed.split('/').collect();
            if parts.len() >= 3 && parts[0].contains('.') {
                Some(parts[..3].join("/"))
            } else if parts.len() == 1 {
                Some(parts[0].split('.').next().unwrap().to_string())
            } else {
                Some(parts[0].to_string())
            }
        }
        _ => Some(trimmed.to_string()),
    }
}
```

- [ ] **Step 3: Write the failing prefix-extraction test**

Append to `crates/graphify-report/src/suggest.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_prefix_rust_takes_first_double_colon_segment() {
        assert_eq!(
            extract_prefix("tokio::sync::mpsc::Sender", "Rust").as_deref(),
            Some("tokio")
        );
        assert_eq!(extract_prefix("std", "Rust").as_deref(), Some("std"));
    }

    #[test]
    fn extract_prefix_python_takes_first_dot_segment() {
        assert_eq!(
            extract_prefix("numpy.linalg.norm", "Python").as_deref(),
            Some("numpy")
        );
    }

    #[test]
    fn extract_prefix_php_takes_first_dot_segment() {
        assert_eq!(
            extract_prefix("Symfony.Component.HttpFoundation.Request", "Php").as_deref(),
            Some("Symfony")
        );
    }

    #[test]
    fn extract_prefix_ts_scoped_takes_two_segments() {
        assert_eq!(
            extract_prefix("@anthropic-ai/sdk/messages", "TypeScript").as_deref(),
            Some("@anthropic-ai/sdk")
        );
    }

    #[test]
    fn extract_prefix_ts_unscoped_takes_first_segment() {
        assert_eq!(
            extract_prefix("react/jsx-runtime", "TypeScript").as_deref(),
            Some("react")
        );
        assert_eq!(extract_prefix("lodash", "TypeScript").as_deref(), Some("lodash"));
    }

    #[test]
    fn extract_prefix_ts_scoped_single_segment_returns_as_is() {
        assert_eq!(
            extract_prefix("@anthropic-ai", "TypeScript").as_deref(),
            Some("@anthropic-ai")
        );
    }

    #[test]
    fn extract_prefix_go_path_style_takes_three_segments() {
        assert_eq!(
            extract_prefix("github.com/spf13/cobra/cmd", "Go").as_deref(),
            Some("github.com/spf13/cobra")
        );
    }

    #[test]
    fn extract_prefix_go_simple_takes_first_dot_segment() {
        assert_eq!(extract_prefix("fmt.Println", "Go").as_deref(), Some("fmt"));
    }

    #[test]
    fn extract_prefix_empty_returns_none() {
        assert_eq!(extract_prefix("", "Rust"), None);
        assert_eq!(extract_prefix("   ", "Rust"), None);
    }

    #[test]
    fn extract_prefix_unknown_language_returns_verbatim() {
        assert_eq!(extract_prefix("foo.bar", "Klingon").as_deref(), Some("foo.bar"));
    }
}
```

- [ ] **Step 4: Add `graphify-extract` to graphify-report dev-deps for `ExternalStubs`**

Wait — `ProjectInput` references `graphify_extract::stubs::ExternalStubs`. We need this dep at build time, not dev-time, since `ProjectInput` is a public type.

In `crates/graphify-report/Cargo.toml`, add to `[dependencies]`:

```toml
graphify-extract = { workspace = true }
```

Confirm the workspace already exposes it (it does — `Cargo.toml` line 21 lists `graphify-extract = { version = "0.13.1", path = "crates/graphify-extract" }`).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p graphify-report suggest::tests`
Expected: all 10 prefix-extraction tests pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/graphify-report/Cargo.toml crates/graphify-report/src/lib.rs crates/graphify-report/src/suggest.rs
git commit -m "feat(report): add suggest module skeleton + prefix extraction (FEAT-043)"
```

---

## Task 3: Implement `score_stubs` core logic

**Files:**
- Modify: `crates/graphify-report/src/suggest.rs` (add `score_stubs` + tests)

- [ ] **Step 1: Write the first failing test (groups across projects)**

In the `tests` mod of `suggest.rs`, add:

```rust
fn make_node(id: &str, lang: &str, is_local: bool) -> GraphNode {
    GraphNode { id: id.to_string(), language: lang.to_string(), is_local }
}

fn make_link(source: &str, target: &str, weight: u32) -> GraphLink {
    GraphLink { source: source.to_string(), target: target.to_string(), weight }
}

#[test]
fn score_stubs_promotes_cross_project_prefix_to_settings() {
    use graphify_extract::stubs::ExternalStubs;
    let empty_stubs = ExternalStubs::default();

    let proj_a_graph = GraphSnapshot {
        nodes: vec![
            make_node("crate_a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
        ],
        links: vec![make_link("crate_a::main", "tokio::spawn", 5)],
    };
    let proj_b_graph = GraphSnapshot {
        nodes: vec![
            make_node("crate_b::main", "Rust", true),
            make_node("tokio::sync::mpsc::Sender", "Rust", false),
        ],
        links: vec![make_link("crate_b::main", "tokio::sync::mpsc::Sender", 3)],
    };

    let inputs = vec![
        ProjectInput {
            name: "proj-a",
            local_prefix: "crate_a",
            current_stubs: &empty_stubs,
            graph: &proj_a_graph,
        },
        ProjectInput {
            name: "proj-b",
            local_prefix: "crate_b",
            current_stubs: &empty_stubs,
            graph: &proj_b_graph,
        },
    ];

    let report = score_stubs(&inputs, 1);

    assert_eq!(report.settings_candidates.len(), 1, "tokio should be in settings");
    let cand = &report.settings_candidates[0];
    assert_eq!(cand.prefix, "tokio");
    assert_eq!(cand.edge_weight, 8); // 5 + 3
    assert_eq!(cand.projects, vec!["proj-a".to_string(), "proj-b".to_string()]);
    assert!(report.per_project_candidates.is_empty());
}
```

- [ ] **Step 2: Run the test, verify it fails (function not defined)**

Run: `cargo test -p graphify-report suggest::tests::score_stubs_promotes`
Expected: FAIL — `score_stubs` not found.

- [ ] **Step 3: Implement `score_stubs`**

Above the `#[cfg(test)] mod tests` block in `suggest.rs`, add:

```rust
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Scoring entry-point. Aggregates external prefix candidates across
/// `projects`, applying `min_edges` per-project before cross-project
/// auto-classification.
pub fn score_stubs(projects: &[ProjectInput<'_>], min_edges: u64) -> SuggestReport {
    // Build a global index of local-prefixes + local-node-top-segments for
    // shadowing safety (rule (a) + (b) from the spec).
    let mut shadow_set: BTreeSet<String> = BTreeSet::new();
    for p in projects {
        if !p.local_prefix.is_empty() {
            shadow_set.insert(p.local_prefix.to_string());
        }
        for n in &p.graph.nodes {
            if n.is_local {
                if let Some(top) = top_segment(&n.id) {
                    shadow_set.insert(top);
                }
            }
        }
    }

    let mut already_covered: BTreeSet<String> = BTreeSet::new();
    let mut shadowed: BTreeSet<String> = BTreeSet::new();

    // Per-project per-prefix accumulator.
    // Key: (project_name, prefix). Value: aggregated stats.
    struct PerProject {
        edge_weight: u64,
        nodes: BTreeSet<String>,
        language: String,
    }
    let mut per_project: HashMap<(String, String), PerProject> = HashMap::new();

    for p in projects {
        // Index nodes by id for quick is_local + language lookup.
        let node_lang: HashMap<&str, (&str, bool)> = p
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), (n.language.as_str(), n.is_local)))
            .collect();

        for link in &p.graph.links {
            let Some((lang, is_local)) = node_lang.get(link.target.as_str()).copied() else {
                continue;
            };
            if is_local {
                continue;
            }
            // Already covered?
            if p.current_stubs.matches(&link.target) {
                if let Some(prefix) = extract_prefix(&link.target, lang) {
                    already_covered.insert(prefix);
                }
                continue;
            }
            let Some(prefix) = extract_prefix(&link.target, lang) else {
                continue;
            };
            // Shadowing?
            if shadow_set.contains(&prefix) {
                shadowed.insert(prefix);
                continue;
            }

            let entry = per_project
                .entry((p.name.to_string(), prefix.clone()))
                .or_insert(PerProject {
                    edge_weight: 0,
                    nodes: BTreeSet::new(),
                    language: lang.to_string(),
                });
            entry.edge_weight += u64::from(link.weight);
            entry.nodes.insert(link.target.clone());
        }
    }

    // Apply per-project threshold.
    per_project.retain(|_, v| v.edge_weight >= min_edges);

    // Group by prefix → list of (project, stats).
    let mut by_prefix: BTreeMap<String, Vec<(String, PerProject)>> = BTreeMap::new();
    for ((proj, prefix), stats) in per_project {
        by_prefix.entry(prefix).or_default().push((proj, stats));
    }

    let mut settings_candidates: Vec<StubCandidate> = Vec::new();
    let mut per_project_candidates: BTreeMap<String, Vec<StubCandidate>> = BTreeMap::new();

    for (prefix, hits) in by_prefix {
        let mut projects_sorted: Vec<String> = hits.iter().map(|(p, _)| p.clone()).collect();
        projects_sorted.sort();
        let total_weight: u64 = hits.iter().map(|(_, s)| s.edge_weight).sum();
        let mut node_set: BTreeSet<String> = BTreeSet::new();
        for (_, s) in &hits {
            for n in &s.nodes {
                node_set.insert(n.clone());
            }
        }
        let example_nodes: Vec<String> = node_set.iter().take(3).cloned().collect();
        // Language: take from the first hit (all hits for the same prefix
        // are expected to share a language; tied to the prefix-extraction
        // rule which is language-keyed).
        let language = hits[0].1.language.clone();

        let cand = StubCandidate {
            prefix: prefix.clone(),
            language,
            edge_weight: total_weight,
            node_count: node_set.len(),
            projects: projects_sorted.clone(),
            example_nodes,
        };

        if projects_sorted.len() >= 2 {
            settings_candidates.push(cand);
        } else {
            per_project_candidates
                .entry(projects_sorted[0].clone())
                .or_default()
                .push(cand);
        }
    }

    // Sort: edge_weight desc, prefix asc as tie-break.
    settings_candidates.sort_by(|a, b| {
        b.edge_weight
            .cmp(&a.edge_weight)
            .then_with(|| a.prefix.cmp(&b.prefix))
    });
    for v in per_project_candidates.values_mut() {
        v.sort_by(|a, b| {
            b.edge_weight
                .cmp(&a.edge_weight)
                .then_with(|| a.prefix.cmp(&b.prefix))
        });
    }

    SuggestReport {
        min_edges,
        settings_candidates,
        per_project_candidates,
        already_covered_prefixes: already_covered.into_iter().collect(),
        shadowed_prefixes: shadowed.into_iter().collect(),
    }
}

fn top_segment(id: &str) -> Option<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Cheapest cross-language top-segment grab: split on the first of
    // `::`, `/`, `.`, whichever appears first.
    let positions = [
        trimmed.find("::").map(|p| (p, 2)),
        trimmed.find('/').map(|p| (p, 1)),
        trimmed.find('.').map(|p| (p, 1)),
    ];
    let earliest = positions.iter().filter_map(|x| *x).min_by_key(|(p, _)| *p);
    match earliest {
        Some((p, _)) => Some(trimmed[..p].to_string()),
        None => Some(trimmed.to_string()),
    }
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `cargo test -p graphify-report suggest::tests::score_stubs_promotes`
Expected: PASS.

- [ ] **Step 5: Add per-project, threshold, already-covered, shadowing tests**

Append to `tests` mod:

```rust
#[test]
fn score_stubs_keeps_single_project_prefix_per_project() {
    use graphify_extract::stubs::ExternalStubs;
    let empty = ExternalStubs::default();

    let g = GraphSnapshot {
        nodes: vec![
            make_node("crate_a::main", "Rust", true),
            make_node("rmcp::ServerHandler", "Rust", false),
        ],
        links: vec![make_link("crate_a::main", "rmcp::ServerHandler", 7)],
    };
    let inputs = vec![ProjectInput {
        name: "proj-a",
        local_prefix: "crate_a",
        current_stubs: &empty,
        graph: &g,
    }];

    let report = score_stubs(&inputs, 1);
    assert!(report.settings_candidates.is_empty());
    assert_eq!(report.per_project_candidates.len(), 1);
    let proj_a = report.per_project_candidates.get("proj-a").unwrap();
    assert_eq!(proj_a.len(), 1);
    assert_eq!(proj_a[0].prefix, "rmcp");
}

#[test]
fn score_stubs_threshold_drops_per_project_before_aggregation() {
    use graphify_extract::stubs::ExternalStubs;
    let empty = ExternalStubs::default();

    // tokio appears in both projects but only weight-4 in each — below
    // threshold of 5, so both drop and the cross-project promotion never
    // happens.
    let g_a = GraphSnapshot {
        nodes: vec![
            make_node("crate_a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
        ],
        links: vec![make_link("crate_a::main", "tokio::spawn", 4)],
    };
    let g_b = GraphSnapshot {
        nodes: vec![
            make_node("crate_b::main", "Rust", true),
            make_node("tokio::sync::mpsc::Sender", "Rust", false),
        ],
        links: vec![make_link("crate_b::main", "tokio::sync::mpsc::Sender", 4)],
    };
    let inputs = vec![
        ProjectInput {
            name: "proj-a",
            local_prefix: "crate_a",
            current_stubs: &empty,
            graph: &g_a,
        },
        ProjectInput {
            name: "proj-b",
            local_prefix: "crate_b",
            current_stubs: &empty,
            graph: &g_b,
        },
    ];

    let report = score_stubs(&inputs, 5);
    assert!(report.settings_candidates.is_empty(), "below threshold per project");
    assert!(report.per_project_candidates.is_empty());
}

#[test]
fn score_stubs_records_already_covered_and_skips_them() {
    use graphify_extract::stubs::ExternalStubs;
    let stubs = ExternalStubs::new(["tokio"]);

    let g = GraphSnapshot {
        nodes: vec![
            make_node("crate_a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
        ],
        links: vec![make_link("crate_a::main", "tokio::spawn", 10)],
    };
    let inputs = vec![ProjectInput {
        name: "proj-a",
        local_prefix: "crate_a",
        current_stubs: &stubs,
        graph: &g,
    }];

    let report = score_stubs(&inputs, 1);
    assert!(report.settings_candidates.is_empty());
    assert!(report.per_project_candidates.is_empty());
    assert_eq!(report.already_covered_prefixes, vec!["tokio".to_string()]);
}

#[test]
fn score_stubs_records_shadowing_against_local_prefix() {
    use graphify_extract::stubs::ExternalStubs;
    let empty = ExternalStubs::default();

    // src is project-a's local_prefix. An external node id "src::foo" from
    // project-b would be shadowing — never suggest.
    let g_a = GraphSnapshot {
        nodes: vec![make_node("src::main", "Rust", true)],
        links: vec![],
    };
    let g_b = GraphSnapshot {
        nodes: vec![
            make_node("crate_b::main", "Rust", true),
            make_node("src::foo::bar", "Rust", false),
        ],
        links: vec![make_link("crate_b::main", "src::foo::bar", 9)],
    };
    let inputs = vec![
        ProjectInput {
            name: "proj-a",
            local_prefix: "src",
            current_stubs: &empty,
            graph: &g_a,
        },
        ProjectInput {
            name: "proj-b",
            local_prefix: "crate_b",
            current_stubs: &empty,
            graph: &g_b,
        },
    ];

    let report = score_stubs(&inputs, 1);
    assert!(report.settings_candidates.is_empty());
    assert!(report.per_project_candidates.is_empty());
    assert_eq!(report.shadowed_prefixes, vec!["src".to_string()]);
}

#[test]
fn score_stubs_ranks_by_edge_weight_then_prefix() {
    use graphify_extract::stubs::ExternalStubs;
    let empty = ExternalStubs::default();

    // Two prefixes both in 2 projects so they cross to settings; verify
    // sort order: serde (weight 100) before tokio (weight 50), and an
    // equal-weight tie sorts by prefix asc.
    let g_a = GraphSnapshot {
        nodes: vec![
            make_node("a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
            make_node("serde::Deserialize", "Rust", false),
            make_node("clap::Parser", "Rust", false),
        ],
        links: vec![
            make_link("a::main", "tokio::spawn", 25),
            make_link("a::main", "serde::Deserialize", 50),
            make_link("a::main", "clap::Parser", 25),
        ],
    };
    let g_b = GraphSnapshot {
        nodes: vec![
            make_node("b::main", "Rust", true),
            make_node("tokio::sync::mpsc::Sender", "Rust", false),
            make_node("serde::Serialize", "Rust", false),
            make_node("clap::ValueEnum", "Rust", false),
        ],
        links: vec![
            make_link("b::main", "tokio::sync::mpsc::Sender", 25),
            make_link("b::main", "serde::Serialize", 50),
            make_link("b::main", "clap::ValueEnum", 25),
        ],
    };
    let inputs = vec![
        ProjectInput {
            name: "proj-a",
            local_prefix: "a",
            current_stubs: &empty,
            graph: &g_a,
        },
        ProjectInput {
            name: "proj-b",
            local_prefix: "b",
            current_stubs: &empty,
            graph: &g_b,
        },
    ];

    let report = score_stubs(&inputs, 1);
    let prefixes: Vec<&str> = report.settings_candidates.iter().map(|c| c.prefix.as_str()).collect();
    assert_eq!(prefixes, vec!["serde", "clap", "tokio"]);
    // serde first (weight 100); clap and tokio tied at 50 → asc by prefix → clap before tokio.
}
```

- [ ] **Step 6: Run all suggest tests**

Run: `cargo test -p graphify-report suggest`
Expected: 14 tests pass (4 prefix + 10 score).

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-report/src/suggest.rs
git commit -m "feat(report): score_stubs with threshold + auto-classify + shadowing (FEAT-043)"
```

---

## Task 4: Implement `render_markdown`

**Files:**
- Modify: `crates/graphify-report/src/suggest.rs`

- [ ] **Step 1: Write failing test for markdown structure**

Append to `tests` mod in `suggest.rs`:

```rust
#[test]
fn render_markdown_includes_all_sections() {
    use graphify_extract::stubs::ExternalStubs;
    let stubs = ExternalStubs::new(["std"]);

    let g_a = GraphSnapshot {
        nodes: vec![
            make_node("a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
            make_node("rmcp::ServerHandler", "Rust", false),
            make_node("std::collections::HashMap", "Rust", false),
        ],
        links: vec![
            make_link("a::main", "tokio::spawn", 5),
            make_link("a::main", "rmcp::ServerHandler", 3),
            make_link("a::main", "std::collections::HashMap", 7),
        ],
    };
    let g_b = GraphSnapshot {
        nodes: vec![
            make_node("b::main", "Rust", true),
            make_node("tokio::sync::mpsc::Sender", "Rust", false),
        ],
        links: vec![make_link("b::main", "tokio::sync::mpsc::Sender", 4)],
    };
    let inputs = vec![
        ProjectInput {
            name: "proj-a",
            local_prefix: "a",
            current_stubs: &stubs,
            graph: &g_a,
        },
        ProjectInput {
            name: "proj-b",
            local_prefix: "b",
            current_stubs: &stubs,
            graph: &g_b,
        },
    ];

    let report = score_stubs(&inputs, 1);
    let md = render_markdown(&report);

    assert!(md.contains("# Stub Suggestions"), "missing header");
    assert!(md.contains("## Promote to [settings].external_stubs"), "missing settings section");
    assert!(md.contains("`tokio`"), "missing tokio entry");
    assert!(md.contains("## Per-project candidates"), "missing per-project section");
    assert!(md.contains("### proj-a"), "missing proj-a subheader");
    assert!(md.contains("`rmcp`"), "missing rmcp entry");
    assert!(md.contains("## Already covered"), "missing already-covered section");
    assert!(md.contains("`std`"), "std should be listed as already covered");
}

#[test]
fn render_markdown_empty_report_emits_no_suggestions_note() {
    let report = SuggestReport {
        min_edges: 2,
        settings_candidates: vec![],
        per_project_candidates: BTreeMap::new(),
        already_covered_prefixes: vec![],
        shadowed_prefixes: vec![],
    };
    let md = render_markdown(&report);
    assert!(md.contains("No stub suggestions"), "missing empty-state message");
}
```

- [ ] **Step 2: Run tests to verify they fail (function not defined)**

Run: `cargo test -p graphify-report suggest::tests::render_markdown`
Expected: FAIL — `render_markdown` not found.

- [ ] **Step 3: Implement `render_markdown`**

In `suggest.rs`, above the `tests` mod:

```rust
use std::fmt::Write as _;

pub fn render_markdown(report: &SuggestReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Stub Suggestions");
    let _ = writeln!(out);

    let total_candidates = report.settings_candidates.len()
        + report
            .per_project_candidates
            .values()
            .map(|v| v.len())
            .sum::<usize>();

    if total_candidates == 0 {
        let _ = writeln!(
            out,
            "No stub suggestions above threshold (--min-edges={}).",
            report.min_edges
        );
        return finalize_md(out, report);
    }

    let _ = writeln!(
        out,
        "{} candidates above threshold (--min-edges={})",
        total_candidates, report.min_edges
    );
    let _ = writeln!(out);

    if !report.settings_candidates.is_empty() {
        let _ = writeln!(out, "## Promote to [settings].external_stubs (cross-project)");
        let _ = writeln!(out);
        let _ = writeln!(out, "| Prefix | Edges | Projects | Example |");
        let _ = writeln!(out, "|--------|-------|----------|---------|");
        for c in &report.settings_candidates {
            let projects_disp = format!("{} ({})", c.projects.len(), c.projects.join(", "));
            let example = c.example_nodes.first().map(String::as_str).unwrap_or("");
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} |",
                c.prefix, c.edge_weight, projects_disp, example
            );
        }
        let _ = writeln!(out);
    }

    if !report.per_project_candidates.is_empty() {
        let _ = writeln!(out, "## Per-project candidates");
        let _ = writeln!(out);
        for (proj, cands) in &report.per_project_candidates {
            let _ = writeln!(out, "### {}", proj);
            let _ = writeln!(out, "| Prefix | Edges | Example |");
            let _ = writeln!(out, "|--------|-------|---------|");
            for c in cands {
                let example = c.example_nodes.first().map(String::as_str).unwrap_or("");
                let _ = writeln!(out, "| `{}` | {} | {} |", c.prefix, c.edge_weight, example);
            }
            let _ = writeln!(out);
        }
    }

    finalize_md(out, report)
}

fn finalize_md(mut out: String, report: &SuggestReport) -> String {
    if !report.already_covered_prefixes.is_empty() {
        let _ = writeln!(out, "## Already covered (skipped)");
        let _ = writeln!(out);
        let list = report
            .already_covered_prefixes
            .iter()
            .map(|p| format!("`{}`", p))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "{} prefix(es) already in current external_stubs: {}", report.already_covered_prefixes.len(), list);
        let _ = writeln!(out);
    }

    if !report.shadowed_prefixes.is_empty() {
        let _ = writeln!(out, "## Skipped — shadowing local modules");
        let _ = writeln!(out);
        let list = report
            .shadowed_prefixes
            .iter()
            .map(|p| format!("`{}`", p))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "{} prefix(es) matching local_prefix or known module: {}", report.shadowed_prefixes.len(), list);
        let _ = writeln!(out);
    }

    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p graphify-report suggest::tests::render_markdown`
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/suggest.rs
git commit -m "feat(report): suggest::render_markdown with all sections (FEAT-043)"
```

---

## Task 5: Implement `render_toml`

**Files:**
- Modify: `crates/graphify-report/src/suggest.rs`

- [ ] **Step 1: Write failing test**

Append to `tests` mod:

```rust
#[test]
fn render_toml_emits_commented_snippet_with_settings_and_per_project() {
    use graphify_extract::stubs::ExternalStubs;
    let empty = ExternalStubs::default();

    let g_a = GraphSnapshot {
        nodes: vec![
            make_node("a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
            make_node("rmcp::ServerHandler", "Rust", false),
        ],
        links: vec![
            make_link("a::main", "tokio::spawn", 5),
            make_link("a::main", "rmcp::ServerHandler", 3),
        ],
    };
    let g_b = GraphSnapshot {
        nodes: vec![
            make_node("b::main", "Rust", true),
            make_node("tokio::sync::mpsc::Sender", "Rust", false),
        ],
        links: vec![make_link("b::main", "tokio::sync::mpsc::Sender", 4)],
    };
    let inputs = vec![
        ProjectInput {
            name: "proj-a",
            local_prefix: "a",
            current_stubs: &empty,
            graph: &g_a,
        },
        ProjectInput {
            name: "proj-b",
            local_prefix: "b",
            current_stubs: &empty,
            graph: &g_b,
        },
    ];

    let report = score_stubs(&inputs, 1);
    let toml_out = render_toml(&report);

    // Header comment with timestamp/threshold
    assert!(toml_out.contains("# Generated by `graphify suggest stubs`"));
    assert!(toml_out.contains("# Min edges per project: 1"));
    // Settings block (commented)
    assert!(toml_out.contains("# [settings]"));
    assert!(toml_out.contains("# external_stubs = [\"tokio\"]"));
    // Per-project block (commented)
    assert!(toml_out.contains("# [[project]]"));
    assert!(toml_out.contains("# name = \"proj-a\""));
    assert!(toml_out.contains("# external_stubs = [\"rmcp\"]"));
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p graphify-report suggest::tests::render_toml`
Expected: FAIL — `render_toml` not found.

- [ ] **Step 3: Implement `render_toml`**

In `suggest.rs`, above the `tests` mod, after `render_markdown`:

```rust
pub fn render_toml(report: &SuggestReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Generated by `graphify suggest stubs`");
    let _ = writeln!(out, "# Min edges per project: {}", report.min_edges);
    let _ = writeln!(out);

    if !report.settings_candidates.is_empty() {
        let _ = writeln!(out, "# Append to [settings] block:");
        let _ = writeln!(out, "# [settings]");
        let prefixes = report
            .settings_candidates
            .iter()
            .map(|c| format!("\"{}\"", c.prefix))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "# external_stubs = [{}]", prefixes);
        let _ = writeln!(out);
    }

    for (proj, cands) in &report.per_project_candidates {
        let _ = writeln!(out, "# Append to [[project]] block name=\"{}\":", proj);
        let _ = writeln!(out, "# [[project]]");
        let _ = writeln!(out, "# name = \"{}\"", proj);
        let prefixes = cands
            .iter()
            .map(|c| format!("\"{}\"", c.prefix))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "# external_stubs = [{}]", prefixes);
        let _ = writeln!(out);
    }

    if report.settings_candidates.is_empty() && report.per_project_candidates.is_empty() {
        let _ = writeln!(out, "# (no candidates above threshold)");
    }

    out
}
```

- [ ] **Step 4: Run test to verify pass**

Run: `cargo test -p graphify-report suggest::tests::render_toml`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/suggest.rs
git commit -m "feat(report): suggest::render_toml emits commented snippet (FEAT-043)"
```

---

## Task 6: Implement `render_json`

**Files:**
- Modify: `crates/graphify-report/src/suggest.rs`

- [ ] **Step 1: Write failing test**

Append to `tests` mod:

```rust
#[test]
fn render_json_round_trips() {
    use graphify_extract::stubs::ExternalStubs;
    let empty = ExternalStubs::default();

    let g = GraphSnapshot {
        nodes: vec![
            make_node("a::main", "Rust", true),
            make_node("tokio::spawn", "Rust", false),
        ],
        links: vec![make_link("a::main", "tokio::spawn", 4)],
    };
    let inputs = vec![ProjectInput {
        name: "proj-a",
        local_prefix: "a",
        current_stubs: &empty,
        graph: &g,
    }];

    let report = score_stubs(&inputs, 1);
    let value = render_json(&report);
    let s = serde_json::to_string(&value).unwrap();

    // Top-level keys present
    let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed["min_edges"], 1);
    assert!(parsed["per_project_candidates"]["proj-a"].is_array());
    assert_eq!(parsed["per_project_candidates"]["proj-a"][0]["prefix"], "tokio");
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p graphify-report suggest::tests::render_json`
Expected: FAIL — `render_json` not found.

- [ ] **Step 3: Implement `render_json`**

In `suggest.rs`, after `render_toml`:

```rust
pub fn render_json(report: &SuggestReport) -> serde_json::Value {
    serde_json::to_value(report).expect("SuggestReport must serialize")
}
```

- [ ] **Step 4: Run test to verify pass**

Run: `cargo test -p graphify-report suggest::tests::render_json`
Expected: PASS.

- [ ] **Step 5: Run full report-crate test suite**

Run: `cargo test -p graphify-report`
Expected: all existing tests pass plus the 18+ new suggest tests.

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-report/src/suggest.rs
git commit -m "feat(report): suggest::render_json (FEAT-043)"
```

---

## Task 7: Wire CLI subcommand `Commands::Suggest`

**Files:**
- Modify: `crates/graphify-cli/src/main.rs` (add `Commands::Suggest`, `SuggestKind`, `cmd_suggest_stubs` for read-only paths)

- [ ] **Step 1: Add `Suggest` variant to `Commands` enum**

In `crates/graphify-cli/src/main.rs`, locate the `Commands` enum (starts at line 219) and the `Consolidation` variant. After the `Consolidation { ... }` block (ends ~line 619, just before `Trend`), add:

```rust
    /// Suggest configuration additions (e.g. external_stubs) based on
    /// existing analysis output. Sub-kinds open the `suggest <kind>`
    /// namespace; `stubs` is currently the only kind.
    Suggest {
        #[command(subcommand)]
        kind: SuggestKind,
    },
```

- [ ] **Step 2: Add `SuggestKind` enum**

In `crates/graphify-cli/src/main.rs`, locate where other sub-action enums live (e.g. `SessionAction` around line 681). After the `SessionAction` enum's closing `}`, add:

```rust
#[derive(Subcommand)]
enum SuggestKind {
    /// Suggest prefixes to add to `[settings].external_stubs` and
    /// `[[project]].external_stubs`. Consumes `graph.json` from each
    /// configured project's output directory; run `graphify run` first.
    Stubs {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output format: `md` (default), `toml`, or `json`. Mutually
        /// exclusive with `--apply`.
        #[arg(long, default_value = "md", conflicts_with = "apply")]
        format: String,

        /// Edit graphify.toml in place, merging suggested prefixes into
        /// existing arrays (preserves comments via `toml_edit`).
        #[arg(long)]
        apply: bool,

        /// Minimum per-project edge weight sum for a prefix to be
        /// suggested. Default: 2.
        #[arg(long, default_value_t = 2)]
        min_edges: u64,

        /// Limit to a single project (suggests only for that project's
        /// per-project block; settings_candidates always empty in this mode).
        #[arg(long)]
        project: Option<String>,
    },
}
```

- [ ] **Step 3: Wire dispatch in main `match`**

Find the existing `Commands::Consolidation { ... }` match arm in `main()` (around line 1265). After the `cmd_consolidation(...)` call closes (with its `}`), add a new match arm above `Commands::InstallIntegrations`:

```rust
        Commands::Suggest { kind } => match kind {
            SuggestKind::Stubs {
                config,
                format,
                apply,
                min_edges,
                project,
            } => {
                cmd_suggest_stubs(&config, &format, apply, min_edges, project.as_deref());
            }
        },
```

- [ ] **Step 4: Implement `cmd_suggest_stubs` (read-only paths only — `--apply` lands in Task 8)**

Append to `crates/graphify-cli/src/main.rs`, near the bottom (after `cmd_consolidation`):

```rust
fn cmd_suggest_stubs(
    config_path: &Path,
    format: &str,
    apply: bool,
    min_edges: u64,
    project_filter: Option<&str>,
) {
    use graphify_extract::stubs::ExternalStubs;
    use graphify_report::suggest::{
        render_json, render_markdown, render_toml, score_stubs, GraphSnapshot, ProjectInput,
    };

    let format = format.to_ascii_lowercase();
    if !apply && !["md", "toml", "json"].contains(&format.as_str()) {
        eprintln!(
            "graphify suggest stubs: unknown --format '{}' (expected md, toml, or json)",
            format
        );
        std::process::exit(1);
    }

    let cfg = load_config(config_path);
    let out_dir = resolve_output(&cfg, None);

    if cfg.project.is_empty() {
        eprintln!(
            "graphify suggest stubs: no projects configured in {:?}",
            config_path
        );
        std::process::exit(1);
    }

    // If --project is specified, validate it exists.
    if let Some(name) = project_filter {
        if !cfg.project.iter().any(|p| p.name == name) {
            eprintln!(
                "graphify suggest stubs: project \"{}\" not found in {:?}",
                name, config_path
            );
            std::process::exit(1);
        }
    }

    // Settings-level shared stubs (FEAT-034 merge layer).
    let settings_stubs: Vec<String> = cfg
        .settings
        .external_stubs
        .clone()
        .unwrap_or_default();

    // Load each project's graph.json + build per-project ExternalStubs.
    struct Loaded {
        name: String,
        local_prefix: String,
        stubs: ExternalStubs,
        graph: GraphSnapshot,
    }
    let mut loaded: Vec<Loaded> = Vec::new();
    let mut any_skipped = false;

    for project in &cfg.project {
        if let Some(name) = project_filter {
            if project.name != name {
                continue;
            }
        }
        let proj_out = out_dir.join(&project.name);
        let graph_path = proj_out.join("graph.json");
        if !graph_path.exists() {
            eprintln!(
                "graphify suggest stubs: project \"{}\" has no graph.json at {} — run `graphify run` first; skipping",
                project.name,
                graph_path.display()
            );
            any_skipped = true;
            continue;
        }
        let text = match std::fs::read_to_string(&graph_path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Cannot read {:?}: {e}", graph_path);
                any_skipped = true;
                continue;
            }
        };
        let graph: GraphSnapshot = match serde_json::from_str(&text) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("Invalid graph.json {:?}: {e}", graph_path);
                any_skipped = true;
                continue;
            }
        };
        if graph.links.is_empty() {
            eprintln!(
                "graphify suggest stubs: project \"{}\" has empty graph.json; skipping",
                project.name
            );
            any_skipped = true;
            continue;
        }

        let local_prefix = project
            .local_prefix
            .clone()
            .unwrap_or_else(|| "src".to_string());

        let stubs = ExternalStubs::new(
            settings_stubs
                .iter()
                .chain(project.external_stubs.iter())
                .cloned(),
        );

        loaded.push(Loaded {
            name: project.name.clone(),
            local_prefix,
            stubs,
            graph,
        });
    }

    if loaded.is_empty() {
        eprintln!(
            "graphify suggest stubs: no graph.json found for any project; run `graphify run` first"
        );
        std::process::exit(1);
    }

    let inputs: Vec<ProjectInput<'_>> = loaded
        .iter()
        .map(|l| ProjectInput {
            name: l.name.as_str(),
            local_prefix: l.local_prefix.as_str(),
            current_stubs: &l.stubs,
            graph: &l.graph,
        })
        .collect();

    let report = score_stubs(&inputs, min_edges);

    if apply {
        // Implemented in Task 8.
        eprintln!("graphify suggest stubs: --apply not yet implemented (placeholder)");
        std::process::exit(1);
    }

    match format.as_str() {
        "md" => print!("{}", render_markdown(&report)),
        "toml" => print!("{}", render_toml(&report)),
        "json" => {
            let value = render_json(&report);
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        }
        _ => unreachable!(),
    }

    let _ = any_skipped; // surfaced via stderr above; exit 0 if at least one project loaded.
}
```

- [ ] **Step 5: Verify the new `[settings].external_stubs` field is reachable**

The `cfg.settings.external_stubs` access requires the `Settings` struct's `external_stubs` field to be in scope. Confirm: `grep -n "external_stubs" crates/graphify-cli/src/main.rs` shows the field defined at line 173 inside `Settings`. No additional change needed.

If the `Settings` field is private and accessed via a getter, fall back to the existing pattern used in `cmd_run` at line 2901–2906 (the `cfg.settings.external_stubs.iter().flatten().cloned()` chain).

- [ ] **Step 6: Build and smoke-test the read-only paths**

Run: `cargo build --workspace`
Expected: clean build.

Run: `cargo run -p graphify-cli -- suggest stubs --config graphify.toml --format md`
Expected: stdout begins with `# Stub Suggestions`. (May produce empty report on this repo since `external_stubs` is well-curated — that's fine.)

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat(cli): graphify suggest stubs read-only paths (FEAT-043)"
```

---

## Task 8: Implement `--apply` with `toml_edit`

**Files:**
- Modify: `crates/graphify-cli/src/main.rs` (replace the `--apply` placeholder in `cmd_suggest_stubs`)

- [ ] **Step 1: Replace the `--apply` placeholder block**

In `cmd_suggest_stubs`, locate this block:

```rust
    if apply {
        // Implemented in Task 8.
        eprintln!("graphify suggest stubs: --apply not yet implemented (placeholder)");
        std::process::exit(1);
    }
```

Replace with:

```rust
    if apply {
        apply_suggestions(config_path, &report);
        return;
    }
```

- [ ] **Step 2: Implement `apply_suggestions` helper**

After `cmd_suggest_stubs`, add:

```rust
fn apply_suggestions(config_path: &Path, report: &graphify_report::suggest::SuggestReport) {
    use toml_edit::{Array, DocumentMut, Item, Table, Value};

    let original = match std::fs::read_to_string(config_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Cannot read {:?}: {e}", config_path);
            std::process::exit(1);
        }
    };
    let mut doc: DocumentMut = match original.parse() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("graphify.toml parse error: {e}");
            std::process::exit(1);
        }
    };

    let mut applied_settings: Vec<String> = Vec::new();
    let mut applied_per_project: Vec<(String, Vec<String>)> = Vec::new();

    // ---- Settings ----
    if !report.settings_candidates.is_empty() {
        let settings = doc
            .as_table_mut()
            .entry("settings")
            .or_insert_with(|| Item::Table(Table::new()));
        let settings_table = settings
            .as_table_mut()
            .expect("settings should be a table");

        let arr_item = settings_table
            .entry("external_stubs")
            .or_insert_with(|| Item::Value(Value::Array(Array::new())));
        let arr = arr_item
            .as_array_mut()
            .expect("settings.external_stubs should be an array");

        let existing: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        for c in &report.settings_candidates {
            if !existing.contains(&c.prefix) {
                arr.push(c.prefix.as_str());
                applied_settings.push(c.prefix.clone());
            }
        }
    }

    // ---- Per-project ----
    if !report.per_project_candidates.is_empty() {
        let projects = doc
            .as_table_mut()
            .get_mut("project")
            .and_then(|i| i.as_array_of_tables_mut());
        let projects = match projects {
            Some(p) => p,
            None => {
                eprintln!(
                    "graphify suggest stubs --apply: graphify.toml has no [[project]] entries"
                );
                std::process::exit(1);
            }
        };

        for (proj_name, cands) in &report.per_project_candidates {
            let mut matched = false;
            for tbl in projects.iter_mut() {
                let name_matches = tbl
                    .get("name")
                    .and_then(|i| i.as_str())
                    .map(|n| n == proj_name)
                    .unwrap_or(false);
                if !name_matches {
                    continue;
                }
                matched = true;

                let arr_item = tbl
                    .entry("external_stubs")
                    .or_insert_with(|| Item::Value(Value::Array(Array::new())));
                let arr = arr_item
                    .as_array_mut()
                    .expect("project.external_stubs should be an array");

                let existing: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                let mut added: Vec<String> = Vec::new();
                for c in cands {
                    if !existing.contains(&c.prefix) {
                        arr.push(c.prefix.as_str());
                        added.push(c.prefix.clone());
                    }
                }
                if !added.is_empty() {
                    applied_per_project.push((proj_name.clone(), added));
                }
            }
            if !matched {
                eprintln!(
                    "graphify suggest stubs --apply: project \"{}\" not found in graphify.toml — config may have drifted since graph.json was generated",
                    proj_name
                );
                std::process::exit(1);
            }
        }
    }

    // ---- Atomic write ----
    let serialized = doc.to_string();
    let parent = config_path.parent().unwrap_or(Path::new("."));
    let mut tmp = match tempfile::NamedTempFile::new_in(parent) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Cannot create tempfile next to {:?}: {e}", config_path);
            std::process::exit(1);
        }
    };
    use std::io::Write as _;
    if let Err(e) = tmp.write_all(serialized.as_bytes()) {
        eprintln!("Cannot write tempfile: {e}");
        std::process::exit(1);
    }
    if let Err(e) = tmp.persist(config_path) {
        eprintln!(
            "Cannot rename tempfile over {:?}: {} (tempfile preserved at {:?})",
            config_path, e.error, e.file.path()
        );
        std::process::exit(1);
    }

    // ---- Summary ----
    let mut total = 0usize;
    println!("Applied stub suggestions to {}:", config_path.display());
    if !applied_settings.is_empty() {
        println!(
            "  + [settings]               {} prefix(es): {}",
            applied_settings.len(),
            applied_settings.join(", ")
        );
        total += applied_settings.len();
    }
    for (proj, added) in &applied_per_project {
        println!(
            "  + [[project]] {:<22} {} prefix(es): {}",
            proj,
            added.len(),
            added.join(", ")
        );
        total += added.len();
    }
    if total == 0 {
        println!("  (no changes — all suggestions already present)");
    } else {
        println!();
        println!(
            "  Total: {} prefix(es) added across {} block(s).",
            total,
            applied_settings.len().min(1) + applied_per_project.len()
        );
    }
}
```

- [ ] **Step 3: Verify `tempfile` is already a dev-dep AND a runtime-dep for the CLI**

Check: `grep "tempfile" crates/graphify-cli/Cargo.toml`. Currently `tempfile` is in `[dev-dependencies]` only. Move it to `[dependencies]` (it's already in `[workspace.dependencies]` so the change is one line):

In `crates/graphify-cli/Cargo.toml`, add to `[dependencies]`:

```toml
tempfile = { workspace = true }
```

Keep the dev-deps entry too (or remove the duplicate — Cargo accepts both forms).

- [ ] **Step 4: Sanity build**

Run: `cargo build --workspace`
Expected: clean build.

- [ ] **Step 5: Smoke-test `--apply` against a throwaway copy of `graphify.toml`**

```bash
cp graphify.toml /tmp/graphify-apply-test.toml
cargo run -p graphify-cli -- suggest stubs --config /tmp/graphify-apply-test.toml --apply
diff graphify.toml /tmp/graphify-apply-test.toml || true
```

Expected: either no-op (`(no changes — all suggestions already present)`) on this mature repo, or a small set of additions visible in the diff with comments preserved.

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-cli/Cargo.toml crates/graphify-cli/src/main.rs
git commit -m "feat(cli): graphify suggest stubs --apply via toml_edit (FEAT-043)"
```

---

## Task 9: Integration test fixture

**Files:**
- Create: `crates/graphify-cli/tests/fixtures/suggest/graphify.toml`
- Create: `crates/graphify-cli/tests/fixtures/suggest/proj-a/graph.json`
- Create: `crates/graphify-cli/tests/fixtures/suggest/proj-b/graph.json`

- [ ] **Step 1: Make the fixture directory**

```bash
mkdir -p crates/graphify-cli/tests/fixtures/suggest/proj-a crates/graphify-cli/tests/fixtures/suggest/proj-b
```

- [ ] **Step 2: Create `graphify.toml` fixture**

`crates/graphify-cli/tests/fixtures/suggest/graphify.toml`:

```toml
[settings]
output = "."
external_stubs = ["std"]

[[project]]
name = "proj-a"
repo = "."
lang = ["rust"]
local_prefix = "crate_a"

[[project]]
name = "proj-b"
repo = "."
lang = ["rust"]
local_prefix = "crate_b"
```

- [ ] **Step 3: Create `proj-a/graph.json` fixture**

`crates/graphify-cli/tests/fixtures/suggest/proj-a/graph.json`:

```json
{
  "directed": true,
  "multigraph": false,
  "nodes": [
    {"id": "crate_a::main", "kind": "Module", "file_path": "crate_a/src/main.rs", "language": "Rust", "line": 1, "is_local": true},
    {"id": "tokio::spawn", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false},
    {"id": "serde::Deserialize", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false},
    {"id": "rmcp::ServerHandler", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false},
    {"id": "std::collections::HashMap", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false}
  ],
  "links": [
    {"source": "crate_a::main", "target": "tokio::spawn", "kind": "Calls", "weight": 5, "line": 10, "confidence": 0.9, "confidence_kind": "Extracted"},
    {"source": "crate_a::main", "target": "serde::Deserialize", "kind": "Imports", "weight": 3, "line": 5, "confidence": 1.0, "confidence_kind": "Extracted"},
    {"source": "crate_a::main", "target": "rmcp::ServerHandler", "kind": "Calls", "weight": 7, "line": 20, "confidence": 0.8, "confidence_kind": "Inferred"},
    {"source": "crate_a::main", "target": "std::collections::HashMap", "kind": "Imports", "weight": 4, "line": 7, "confidence": 1.0, "confidence_kind": "ExpectedExternal"}
  ]
}
```

- [ ] **Step 4: Create `proj-b/graph.json` fixture**

`crates/graphify-cli/tests/fixtures/suggest/proj-b/graph.json`:

```json
{
  "directed": true,
  "multigraph": false,
  "nodes": [
    {"id": "crate_b::main", "kind": "Module", "file_path": "crate_b/src/main.rs", "language": "Rust", "line": 1, "is_local": true},
    {"id": "tokio::sync::mpsc::Sender", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false},
    {"id": "serde::Serialize", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false},
    {"id": "clap::Parser", "kind": "Module", "file_path": "", "language": "Rust", "line": 0, "is_local": false}
  ],
  "links": [
    {"source": "crate_b::main", "target": "tokio::sync::mpsc::Sender", "kind": "Calls", "weight": 4, "line": 12, "confidence": 0.9, "confidence_kind": "Extracted"},
    {"source": "crate_b::main", "target": "serde::Serialize", "kind": "Imports", "weight": 2, "line": 5, "confidence": 1.0, "confidence_kind": "Extracted"},
    {"source": "crate_b::main", "target": "clap::Parser", "kind": "Imports", "weight": 6, "line": 6, "confidence": 1.0, "confidence_kind": "Extracted"}
  ]
}
```

- [ ] **Step 5: Smoke-test the fixture by hand**

```bash
cargo run -p graphify-cli -- suggest stubs --config crates/graphify-cli/tests/fixtures/suggest/graphify.toml --format md
```

Expected: stdout shows `tokio` + `serde` under "Promote to [settings]" (cross-project), `rmcp` under proj-a, `clap` under proj-b, and `std` under "Already covered".

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-cli/tests/fixtures/suggest/
git commit -m "test(cli): suggest fixture with two-project graph.json (FEAT-043)"
```

---

## Task 10: Integration tests

**Files:**
- Create: `crates/graphify-cli/tests/suggest_integration.rs`

- [ ] **Step 1: Write the test file**

`crates/graphify-cli/tests/suggest_integration.rs`:

```rust
//! End-to-end tests for `graphify suggest stubs`.

use std::path::PathBuf;
use std::process::Command;

fn graphify_bin() -> PathBuf {
    // CARGO_BIN_EXE_<name> is set by Cargo during integration test build.
    PathBuf::from(env!("CARGO_BIN_EXE_graphify"))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("suggest")
}

#[test]
fn suggest_stubs_md_output_contains_expected_sections() {
    let cfg = fixture_dir().join("graphify.toml");
    let output = Command::new(graphify_bin())
        .args([
            "suggest",
            "stubs",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "md",
        ])
        .output()
        .expect("graphify binary should run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("# Stub Suggestions"));
    assert!(stdout.contains("`tokio`"), "expected tokio in cross-project section: {}", stdout);
    assert!(stdout.contains("`serde`"));
    assert!(stdout.contains("### proj-a"));
    assert!(stdout.contains("`rmcp`"));
    assert!(stdout.contains("### proj-b"));
    assert!(stdout.contains("`clap`"));
    assert!(stdout.contains("`std`"), "std should appear in Already covered: {}", stdout);
}

#[test]
fn suggest_stubs_json_output_is_well_formed() {
    let cfg = fixture_dir().join("graphify.toml");
    let output = Command::new(graphify_bin())
        .args([
            "suggest",
            "stubs",
            "--config",
            cfg.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("graphify binary should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(parsed["min_edges"], 2);
    assert!(parsed["settings_candidates"].is_array());
}

#[test]
fn suggest_stubs_apply_mutates_config_and_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let dst_cfg = tmp.path().join("graphify.toml");
    let dst_proj_a = tmp.path().join("proj-a");
    let dst_proj_b = tmp.path().join("proj-b");
    std::fs::create_dir(&dst_proj_a).unwrap();
    std::fs::create_dir(&dst_proj_b).unwrap();
    std::fs::copy(fixture_dir().join("graphify.toml"), &dst_cfg).unwrap();
    std::fs::copy(
        fixture_dir().join("proj-a/graph.json"),
        dst_proj_a.join("graph.json"),
    )
    .unwrap();
    std::fs::copy(
        fixture_dir().join("proj-b/graph.json"),
        dst_proj_b.join("graph.json"),
    )
    .unwrap();

    // First apply.
    let output = Command::new(graphify_bin())
        .args(["suggest", "stubs", "--config", dst_cfg.to_str().unwrap(), "--apply"])
        .output()
        .expect("graphify should run");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let after = std::fs::read_to_string(&dst_cfg).unwrap();
    assert!(after.contains("\"tokio\""), "expected tokio in [settings]: {}", after);
    assert!(after.contains("\"serde\""));
    assert!(after.contains("\"rmcp\""));
    assert!(after.contains("\"clap\""));

    // Second apply — should be a no-op.
    let output2 = Command::new(graphify_bin())
        .args(["suggest", "stubs", "--config", dst_cfg.to_str().unwrap(), "--apply"])
        .output()
        .expect("graphify should run");
    assert!(output2.status.success());
    let stdout2 = String::from_utf8(output2.stdout).unwrap();
    assert!(stdout2.contains("(no changes"), "second apply should be no-op: {}", stdout2);

    let after2 = std::fs::read_to_string(&dst_cfg).unwrap();
    assert_eq!(after, after2, "second apply must not change the file");
}

#[test]
fn suggest_stubs_format_and_apply_are_mutually_exclusive() {
    let cfg = fixture_dir().join("graphify.toml");
    let output = Command::new(graphify_bin())
        .args([
            "suggest", "stubs", "--config", cfg.to_str().unwrap(),
            "--format", "json", "--apply",
        ])
        .output()
        .expect("graphify should run");
    assert!(!output.status.success(), "clap should reject the combination");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("conflict") || stderr.contains("cannot be used"), "stderr: {}", stderr);
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test -p graphify-cli --test suggest_integration`
Expected: 4 tests pass.

- [ ] **Step 3: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass (existing + 18 unit + 4 integration = ~22+ new).

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-cli/tests/suggest_integration.rs
git commit -m "test(cli): suggest stubs e2e — md/json/apply/clap-conflict (FEAT-043)"
```

---

## Task 11: Dogfood the new subcommand

**Files:** none modified — this is a verification step.

- [ ] **Step 1: Run on the live repo**

```bash
cargo run -p graphify-cli -- suggest stubs --config graphify.toml --format md
```

- [ ] **Step 2: Inspect the output**

Expected behaviour on this mature repo:
- `Already covered` section lists most/all of the prelude entries already in `[settings].external_stubs` (std, format, writeln, assert*, Vec, Some, None, …)
- `settings_candidates` and `per_project_candidates` are likely small or empty — the codebase has been curated already
- If unexpected new candidates appear, evaluate each:
  - Genuinely missing prelude entry → add via `--apply` and ship as part of this FEAT
  - Real third-party dep that should have been in `external_stubs` from day 1 → add via `--apply`
  - False positive (something local that looks external) → file follow-up bug, do not apply

- [ ] **Step 3: Optionally `--apply` if dogfood reveals legitimate missing stubs**

```bash
cargo run -p graphify-cli -- suggest stubs --config graphify.toml --apply
```

If anything was added, inspect: `git diff graphify.toml`.

- [ ] **Step 4: Re-run `graphify check` to confirm no regressions**

Run: `cargo run -p graphify-cli -- check --config graphify.toml`
Expected: all 5 projects PASS, hotspot scores unchanged or marginally lower (more edges classified as ExpectedExternal → less weight on Ambiguous).

- [ ] **Step 5: Commit if dogfood applied changes**

```bash
git add graphify.toml report/  # if report was regenerated
git commit -m "chore: apply dogfood suggestions from graphify suggest stubs (FEAT-043)"
```

If no changes: skip the commit.

---

## Task 12: Update README and CHANGELOG

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Add a README section under `## Commands`**

Find the existing `## Commands` section in `README.md` (line 77). Pick a logical insertion point (after `### Render a PR summary for GitHub Actions` around line 316 — alongside `pr-summary` and `consolidation`-related commands). Add:

````markdown
### Suggest external_stubs additions

`graphify suggest stubs` analyses each project's `graph.json` and recommends prefixes to add to `[settings].external_stubs` (cross-project) or `[[project]].external_stubs` (single-project). Run after `graphify run`:

```bash
# Print suggestions as Markdown (default)
graphify suggest stubs --config graphify.toml

# JSON for tooling
graphify suggest stubs --format json

# Apply directly to graphify.toml (preserves comments via toml_edit)
graphify suggest stubs --apply

# Higher signal — only suggest prefixes with ≥10 edges in any project
graphify suggest stubs --min-edges 10
```

Auto-classification: a prefix that survives the `--min-edges` filter in **2 or more** projects is suggested for `[settings].external_stubs`; a single-project survivor lands in that project's `[[project]] external_stubs`. Prefixes already covered by current stubs (or that collide with a project's `local_prefix`) are skipped and listed separately for visibility.
````

- [ ] **Step 2: Add a CHANGELOG entry**

In `CHANGELOG.md`, replace the `## [Unreleased]` line with:

```markdown
## [Unreleased]

### Added
- feat(cli): `graphify suggest stubs` — post-analysis subcommand that scans each project's `graph.json`, groups external references by language-aware prefix, and recommends additions to `[settings].external_stubs` (cross-project) or `[[project]].external_stubs` (single-project). Auto-classifies cross-project hits via a per-project `--min-edges` threshold (default 2) before promotion; skips prefixes already covered or shadowing a `local_prefix`. Output formats: `md` (default), `toml`, `json`. `--apply` edits `graphify.toml` in place via `toml_edit`, preserving comments and ordering. Idempotent — re-running `--apply` is a no-op. FEAT-043.
```

- [ ] **Step 3: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: README + CHANGELOG for graphify suggest stubs (FEAT-043)"
```

---

## Task 13: Final CI gate

**Files:** none modified — gate-only.

- [ ] **Step 1: Format check**

Run: `cargo fmt --all -- --check`
Expected: no diff. If diff: run `cargo fmt --all` and commit (`style: cargo fmt`).

- [ ] **Step 2: Clippy gate**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings. If warnings: fix and commit (`fix: clippy warnings`).

- [ ] **Step 3: Full test suite**

Run: `cargo test --workspace`
Expected: all tests pass — pre-existing + new suggest unit tests + 4 integration tests.

- [ ] **Step 4: Final architectural check**

Run: `cargo run -p graphify-cli -- check --config graphify.toml`
Expected: all 5 projects PASS, 0 cycles. If new hotspot crosses any threshold, investigate before declaring complete.

- [ ] **Step 5: Mark task as done in `tn`**

```bash
tn new --type FEAT --priority normal --sprint --body-file - "graphify suggest stubs" <<'EOF'
# graphify suggest stubs

Shipped FEAT-043 — post-analysis subcommand that scans graph.json from each
project, groups external references by language-aware prefix, auto-classifies
cross-project candidates, and emits md/toml/json or applies to graphify.toml
in place via toml_edit.

## Description

See spec at `docs/superpowers/specs/2026-04-26-feat-043-suggest-stubs-design.md`
and plan at `docs/superpowers/plans/2026-04-26-feat-043-suggest-stubs.md`.

## Subtasks

- [x] Add toml_edit dep
- [x] graphify-report::suggest module (types + extract_prefix + score_stubs)
- [x] Renderers (markdown, toml, json)
- [x] CLI subcommand graphify suggest stubs
- [x] --apply via toml_edit
- [x] Integration tests + fixture
- [x] Dogfood + verify check still passes
- [x] README + CHANGELOG
EOF
tn done <FEAT-ID-PRINTED-ABOVE> --keep-subtasks
```

---

## Self-Review Checklist (run after writing the plan)

- [x] **Spec coverage:** every section of the spec maps to at least one task. Architecture → Tasks 2-8. Output formats → Tasks 4-6. `--apply` → Task 8. Edge cases → orchestration in Task 7 + integration tests in Task 10. Testing strategy → unit tests in Tasks 2-6, integration in Task 10, dogfood in Task 11.
- [x] **No placeholders:** every step has actual code or commands. No "TBD" or "implement later" except the deliberate Task 7 → Task 8 placeholder which is replaced in Task 8 Step 1.
- [x] **Type consistency:** `score_stubs(projects, min_edges)` signature stable across Tasks 3, 4, 5, 6. `StubCandidate` field names match between report types and renderers. `GraphSnapshot` / `GraphNode` / `GraphLink` shapes are defined once in Task 2 and consumed verbatim in Tasks 3, 7, 9.
- [x] **Spec deviation noted:** spec was corrected mid-flight (analysis.json → graph.json) and re-committed before plan-writing — plan reflects corrected spec.

---

## Out of scope (explicit non-tasks)

- Removing existing `external_stubs` entries (would need a different signal — not "is the prefix used?" but "is the prefix correctly classified as external?"). Out of scope per spec.
- Suggesting `[consolidation].allowlist` entries — separate FEAT, separate signal.
- Bootstrapping language-specific prelude defaults at `graphify init` time. The Rust prelude in this repo is hand-curated; users on other languages would benefit from an opinionated default but that's a follow-up.
- Multi-language single-project handling beyond what `extract_prefix` does — projects with `lang = ["python", "typescript"]` will get prefixes from each language's natural shape, since the language hint comes per-node from `graph.json`. No cross-language coalescing is attempted.
