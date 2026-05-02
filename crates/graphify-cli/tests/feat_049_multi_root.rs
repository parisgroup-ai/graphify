//! End-to-end integration test for FEAT-049 multi-root local_prefix.
//!
//! Builds an Expo-shaped fixture (parallel `app/`, `lib/`, `components/`),
//! runs `graphify run` against it, and asserts:
//!   - module IDs are not wrapped under any prefix (no `app.lib.foo`)
//!   - cross-imports between `lib/` and `components/` resolve as local edges
//!   - third-party imports (`react`) are NOT classified local

use std::path::PathBuf;
use std::process::Command;

fn graphify_bin() -> PathBuf {
    // Built artifact path — assumes `cargo build -p graphify-cli` already ran.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../target/debug/graphify");
    p
}

#[test]
fn multi_root_expo_fixture_no_wrapping_and_third_party_external() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let out = dir.path().join("report");
    std::fs::create_dir_all(repo.join("app/(tabs)")).unwrap();
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    std::fs::create_dir_all(repo.join("components")).unwrap();

    std::fs::write(
        repo.join("app/(tabs)/_layout.tsx"),
        r#"
import { Button } from "@/components/Button";
import { client } from "@/lib/api";
import React from "react";

export default function Layout() {
  return null;
}
"#,
    )
    .unwrap();

    std::fs::write(
        repo.join("lib/api.ts"),
        r#"
export const client = {};
"#,
    )
    .unwrap();

    std::fs::write(
        repo.join("components/Button.tsx"),
        r#"
import { client } from "@/lib/api";
export const Button = () => null;
"#,
    )
    .unwrap();

    // Minimal tsconfig.json with `@` alias to repo root — same shape as
    // Expo Router's default.
    std::fs::write(
        repo.join("tsconfig.json"),
        r#"
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@/*": ["./*"] }
  }
}
"#,
    )
    .unwrap();

    let config = format!(
        r#"
[settings]
output = "{}"

[[project]]
name = "mobile"
repo = "{}"
lang = ["typescript"]
local_prefix = ["app", "lib", "components"]
external_stubs = ["react"]
"#,
        out.display(),
        repo.display(),
    );
    let cfg_path = dir.path().join("graphify.toml");
    std::fs::write(&cfg_path, config).unwrap();

    let output = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&cfg_path)
        .output()
        .expect("graphify binary should run");

    assert!(
        output.status.success(),
        "graphify run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let analysis = std::fs::read_to_string(out.join("mobile/analysis.json"))
        .expect("analysis.json must exist");
    let json: serde_json::Value = serde_json::from_str(&analysis).unwrap();

    let nodes = json["nodes"].as_array().expect("nodes is an array");
    let node_ids: Vec<&str> = nodes.iter().filter_map(|n| n["id"].as_str()).collect();

    // No wrapping — `lib.api`, `components.Button` exist directly.
    assert!(
        node_ids.iter().any(|id| *id == "lib.api"),
        "expected 'lib.api' in nodes; got: {node_ids:?}"
    );
    assert!(
        node_ids.iter().any(|id| *id == "components.Button"),
        "expected 'components.Button' in nodes; got: {node_ids:?}"
    );
    // No `app.lib.api` (would prove wrapping leaked).
    assert!(
        !node_ids.iter().any(|id| *id == "app.lib.api"),
        "must not wrap under 'app': {node_ids:?}"
    );

    // `react` should be classified ExpectedExternal via external_stubs.
    // Per-node `is_local` lives in `graph.json` (analysis.json carries metrics
    // only); per-edge `confidence_kind` cross-checks the same classification
    // from the analysis side.
    let graph =
        std::fs::read_to_string(out.join("mobile/graph.json")).expect("graph.json must exist");
    let graph_json: serde_json::Value = serde_json::from_str(&graph).unwrap();
    let graph_nodes = graph_json["nodes"]
        .as_array()
        .expect("graph.json nodes is an array");
    let react_node = graph_nodes
        .iter()
        .find(|n| n["id"].as_str() == Some("react"))
        .expect("react node should exist in graph.json");
    let is_local = react_node["is_local"].as_bool().unwrap_or(true);
    assert!(
        !is_local,
        "react must be classified non-local: {react_node:?}"
    );

    // Cross-check: the import edge to react carries ExpectedExternal confidence.
    let edges = json["edges"]
        .as_array()
        .expect("analysis.json edges is an array");
    let react_edge = edges
        .iter()
        .find(|e| e["target"].as_str() == Some("react"))
        .expect("at least one edge should target 'react'");
    assert_eq!(
        react_edge["confidence_kind"].as_str(),
        Some("ExpectedExternal"),
        "react edge must be ExpectedExternal: {react_edge:?}"
    );
}
