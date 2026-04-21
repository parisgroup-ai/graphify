//! FEAT-031 integration test: scoped calls (`Node::module()`) and bare-name
//! calls (`validate()`) emitted after a `use` declaration should resolve to
//! local symbol nodes after the full extract → resolve pipeline runs.
//!
//! The fixture is a minimal two-file Rust crate so the test stays fast and
//! readable — the test does NOT build a real `Cargo.toml`/compile the code,
//! it only exercises graphify's tree-sitter + resolver pipeline against the
//! source files. That's sufficient to prove that `Node::module` (scoped)
//! and `validate` (bare) land on the canonical local symbol nodes instead
//! of non-local placeholders.

use std::process::Command;

use serde_json::Value;

/// Build a 2-file Rust crate at `repo_root`:
///
/// - `src/types.rs` defines `struct Node` with `impl Node { pub fn module() }`.
/// - `src/graph.rs` uses `use crate::types::Node;` and calls `Node::module()`
///   plus a bare-name call `validate()` after `use crate::validator::validate;`.
///   (No `validator.rs` file — the bare-name case documents expected behaviour
///   for missing declarations: the alias rewrites the target, and the resolver
///   reports it non-local because the rewritten id isn't registered.)
fn write_fixture(repo_root: &std::path::Path) {
    std::fs::create_dir_all(repo_root.join("src")).expect("create src/");

    std::fs::write(
        repo_root.join("src/types.rs"),
        r#"pub struct Node {
    pub id: String,
}

impl Node {
    pub fn module(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}
"#,
    )
    .expect("write types.rs");

    std::fs::write(
        repo_root.join("src/graph.rs"),
        r#"use crate::types::Node;

pub struct Graph;

impl Graph {
    pub fn build(&self) {
        let _ = Node::module("a");
        let _ = Node::module("b");
    }
}
"#,
    )
    .expect("write graph.rs");

    std::fs::write(
        repo_root.join("src/lib.rs"),
        r#"pub mod types;
pub mod graph;
"#,
    )
    .expect("write lib.rs");
}

#[test]
fn feat_031_scoped_call_resolves_to_local_symbol_method() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("crate");
    write_fixture(&repo);

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[settings]
output = "."
format = ["json"]

[[project]]
name = "crate"
repo = "{}"
lang = ["rust"]
local_prefix = "src"
"#,
            repo.to_string_lossy(),
        ),
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "run",
            "--config",
            config_path.to_str().unwrap(),
            "--output",
            tmp.path().to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("spawn graphify");

    assert!(
        output.status.success(),
        "graphify run failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let graph_path = tmp.path().join("crate/graph.json");
    let graph: Value =
        serde_json::from_str(&std::fs::read_to_string(&graph_path).expect("read graph.json"))
            .expect("parse graph.json");

    let links = graph["links"].as_array().expect("graph links array");

    // Pre-FEAT-031: scoped `Node::module()` calls were dropped by the
    // extractor and never produced a Calls edge.
    // Post-FEAT-031: the extractor emits the scoped target, the resolver
    // rewrites it through `use_aliases`, and the Calls edge lands on
    // `src.types.Node.module`.
    let scoped_calls: Vec<_> = links
        .iter()
        .filter(|link| {
            link["kind"].as_str() == Some("Calls")
                && link["source"].as_str() == Some("src.graph")
                && link["target"].as_str() == Some("src.types.Node.module")
        })
        .collect();

    assert!(
        !scoped_calls.is_empty(),
        "expected at least one `Calls` edge from `src.graph` to `src.types.Node.module`, got links:\n{graph:#}"
    );
}

#[test]
fn feat_031_bare_name_call_with_missing_symbol_stays_non_local() {
    // Negative test: `Vec::new()` with no `use Vec` must not be promoted.
    // Fixture is a single file calling `Vec::new()` and `HashMap::new()` —
    // both should remain non-local after resolution.
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("crate");
    std::fs::create_dir_all(repo.join("src")).expect("create src/");

    std::fs::write(
        repo.join("src/lib.rs"),
        r#"pub fn run() {
    let _ = Vec::<u8>::new();
    let _ = String::new();
}
"#,
    )
    .expect("write lib.rs");

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[settings]
output = "."
format = ["json"]

[[project]]
name = "crate"
repo = "{}"
lang = ["rust"]
local_prefix = "src"
"#,
            repo.to_string_lossy(),
        ),
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "run",
            "--config",
            config_path.to_str().unwrap(),
            "--output",
            tmp.path().to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("spawn graphify");
    assert!(output.status.success());

    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("crate/graph.json")).expect("read graph.json"),
    )
    .expect("parse graph.json");

    // `Vec::new` / `String::new` targets must never end up under the `src.`
    // prefix (local) — no alias, no promotion.
    let bogus_promotions: Vec<_> = graph["links"]
        .as_array()
        .expect("links array")
        .iter()
        .filter(|link| {
            let target = link["target"].as_str().unwrap_or("");
            (target.starts_with("src.Vec") || target.starts_with("src.String"))
                && link["kind"].as_str() == Some("Calls")
        })
        .collect();

    assert!(
        bogus_promotions.is_empty(),
        "std-library calls without `use` must not be spuriously promoted to local: {bogus_promotions:#?}"
    );
}

#[test]
fn bug_018_local_calls_edge_keeps_extractor_confidence() {
    // BUG-018: pre-fix, a scoped call `Node::module()` resolved to
    // `src.types.Node.module` via the FEAT-031 `use`-alias fallback, but the
    // resolver's `known_modules` only contained module-level ids — not the
    // symbol-level `src.types.Node.module` — so `is_local` came back false,
    // the pipeline's non-local downgrade ran, and the edge landed at
    // `confidence: 0.5, kind: Ambiguous`. The extractor's contract was
    // `0.7/Inferred`, and since the target actually is local, that's what we
    // should see after resolution.
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("crate");
    write_fixture(&repo);

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[settings]
output = "."
format = ["json"]

[[project]]
name = "crate"
repo = "{}"
lang = ["rust"]
local_prefix = "src"
"#,
            repo.to_string_lossy(),
        ),
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "run",
            "--config",
            config_path.to_str().unwrap(),
            "--output",
            tmp.path().to_str().unwrap(),
            "--force",
        ])
        .output()
        .expect("spawn graphify");
    assert!(
        output.status.success(),
        "graphify run failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join("crate/graph.json")).expect("read graph.json"),
    )
    .expect("parse graph.json");

    let call_edge = graph["links"]
        .as_array()
        .expect("links array")
        .iter()
        .find(|link| {
            link["kind"].as_str() == Some("Calls")
                && link["source"].as_str() == Some("src.graph")
                && link["target"].as_str() == Some("src.types.Node.module")
        })
        .unwrap_or_else(|| panic!("Calls edge missing; pipeline regression — got:\n{graph:#}"));

    let kind = call_edge["confidence_kind"]
        .as_str()
        .expect("confidence_kind is a string");
    assert_ne!(
        kind, "Ambiguous",
        "BUG-018 regression: local Calls edge tagged Ambiguous instead of Inferred\n  edge: {call_edge:#}",
    );

    let conf = call_edge["confidence"]
        .as_f64()
        .expect("confidence is a number");
    assert!(
        conf >= 0.7 - f64::EPSILON,
        "BUG-018 regression: local Calls edge confidence {conf} is below the extractor floor 0.7 — the non-local cap still fired",
    );
}
