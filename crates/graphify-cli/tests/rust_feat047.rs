//! FEAT-047 integration test: Rust consumer-side `use_aliases`
//! canonicalization.
//!
//! Extends FEAT-046's fixture (a 3-file Rust crate where `lib.rs`
//! re-exports `Bar` from `foo` via `pub use foo::Bar;` and `consumer.rs`
//! does `use crate::Bar;` followed by `Bar::new()`).
//!
//! FEAT-046 already covers the consumer's Calls edge (`Bar::new` →
//! `src.foo.Bar.new`) via the post-resolve `barrel_to_canonical` rewrite.
//! FEAT-047 closes the loop on the consumer's IMPORTS edge — the
//! `use crate::Bar;` itself — and on any future code path that consumes
//! `use_aliases` without flowing through the post-resolve rewrite.
//!
//! Today the Imports edge ALREADY lands at `src.foo.Bar` because FEAT-046's
//! post-resolve rewrite catches the resolved barrel id. This test pins that
//! behaviour so a future regression in either FEAT-046's edge rewrite OR
//! FEAT-047's alias rewrite is caught at CI time.

use std::process::Command;

use serde_json::Value;

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
fn feat_047_consumer_imports_edge_targets_canonical() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("crate");
    write_fixture(&repo);
    run_graphify(tmp.path(), &repo);

    let graph_path = tmp.path().join("crate/graph.json");
    let graph: Value =
        serde_json::from_str(&std::fs::read_to_string(&graph_path).expect("read graph.json"))
            .expect("parse graph.json");

    let links = graph["links"].as_array().expect("graph links array");

    // The `use crate::Bar;` Imports edge from `src.consumer` must land on
    // the canonical declaration `src.foo.Bar`, not the barrel-scoped
    // `src.Bar`.
    let canonical_imports: Vec<_> = links
        .iter()
        .filter(|link| {
            link["kind"].as_str() == Some("Imports")
                && link["source"].as_str() == Some("src.consumer")
                && link["target"].as_str() == Some("src.foo.Bar")
        })
        .collect();

    assert!(
        !canonical_imports.is_empty(),
        "expected `Imports` edge from `src.consumer` to `src.foo.Bar` (canonical), got links:\n{graph:#}"
    );

    // And the barrel-scoped Imports target must NOT survive — the
    // re-export collapse repointed it at the canonical declaration.
    let barrel_imports: Vec<_> = links
        .iter()
        .filter(|link| {
            link["kind"].as_str() == Some("Imports")
                && link["source"].as_str() == Some("src.consumer")
                && link["target"].as_str() == Some("src.Bar")
        })
        .collect();

    assert!(
        barrel_imports.is_empty(),
        "barrel-scoped Imports edge `src.consumer` → `src.Bar` should have been collapsed to canonical, found: {barrel_imports:#?}"
    );
}
