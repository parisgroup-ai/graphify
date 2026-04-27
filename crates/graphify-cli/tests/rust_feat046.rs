//! FEAT-046 integration test: per-project `pub use` re-export collapse for
//! Rust. A 2-file crate where `lib.rs` re-exports `Bar` from `foo` should
//! cause a consumer's `Bar::new()` Calls edge to land on the canonical
//! declaration `src.foo.Bar`, not the barrel-scoped placeholder `src.Bar`.
//!
//! Pre-FEAT-046: the resolver's FEAT-031 use-alias fallback rewrote
//! `Bar::new` → `crate::Bar` → `src.Bar`. That id has no Defines, so the
//! edge landed at `src.Bar` and dragged a non-local downgrade with it (or,
//! after BUG-018, the lack of a registered symbol still left the call
//! orphaned at the barrel scope).
//!
//! Post-FEAT-046: the per-project `ReExportGraph` walks the `pub use`
//! chain to its canonical declaration and the edge-resolution loop repoints
//! `src.Bar` → `src.foo.Bar` after `resolver.resolve()`.

use std::process::Command;

use serde_json::Value;

/// Build a 3-file Rust crate at `repo_root`:
///
/// - `src/foo.rs` defines `pub struct Bar` with `impl Bar { pub fn new() }`.
/// - `src/lib.rs` declares `pub mod foo;` and re-exports `pub use foo::Bar;`.
/// - `src/consumer.rs` does `use crate::Bar;` and calls `Bar::new()`.
fn write_fixture(repo_root: &std::path::Path) {
    std::fs::create_dir_all(repo_root.join("src")).expect("create src/");

    std::fs::write(
        repo_root.join("src/foo.rs"),
        r#"pub struct Bar {
    pub id: String,
}

impl Bar {
    pub fn new() -> Self {
        Self { id: String::new() }
    }
}
"#,
    )
    .expect("write foo.rs");

    std::fs::write(
        repo_root.join("src/consumer.rs"),
        r#"use crate::Bar;

pub fn build() {
    let _ = Bar::new();
}
"#,
    )
    .expect("write consumer.rs");

    std::fs::write(
        repo_root.join("src/lib.rs"),
        r#"pub mod foo;
pub mod consumer;

pub use foo::Bar;
"#,
    )
    .expect("write lib.rs");
}

fn run_graphify(tmp: &std::path::Path, repo: &std::path::Path) {
    let config_path = tmp.join("graphify.toml");
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
            tmp.to_str().unwrap(),
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
}

#[test]
fn feat_046_pub_use_collapses_consumer_call_to_canonical() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("crate");
    write_fixture(&repo);
    run_graphify(tmp.path(), &repo);

    let graph_path = tmp.path().join("crate/graph.json");
    let graph: Value =
        serde_json::from_str(&std::fs::read_to_string(&graph_path).expect("read graph.json"))
            .expect("parse graph.json");

    let links = graph["links"].as_array().expect("graph links array");

    // Post-FEAT-046: the Calls edge from `src.consumer` (or whichever module
    // contains `Bar::new()`) should land on the canonical declaration
    // `src.foo.Bar.new`, not the barrel-scoped `src.Bar.new`.
    let canonical_calls: Vec<_> = links
        .iter()
        .filter(|link| {
            link["kind"].as_str() == Some("Calls")
                && link["source"].as_str() == Some("src.consumer")
                && link["target"].as_str() == Some("src.foo.Bar.new")
        })
        .collect();

    assert!(
        !canonical_calls.is_empty(),
        "expected `Calls` edge from `src.consumer` to `src.foo.Bar.new` (canonical), got links:\n{graph:#}"
    );

    // And the barrel-scoped target must NOT appear as a Calls target — the
    // re-export collapse repointed it at the canonical declaration.
    let barrel_calls: Vec<_> = links
        .iter()
        .filter(|link| {
            link["kind"].as_str() == Some("Calls") && link["target"].as_str() == Some("src.Bar.new")
        })
        .collect();

    assert!(
        barrel_calls.is_empty(),
        "barrel-scoped Calls edge to `src.Bar.new` should have been collapsed to canonical, found: {barrel_calls:#?}"
    );
}
