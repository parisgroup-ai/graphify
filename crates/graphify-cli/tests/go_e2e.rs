use std::process::Command;

use serde_json::Value;

#[test]
fn graphify_run_against_go_module_keeps_canonical_package_ids() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("daemon");

    std::fs::create_dir_all(repo.join("cmd/server")).expect("create cmd/server");
    std::fs::create_dir_all(repo.join("internal/bridge")).expect("create internal/bridge");
    std::fs::create_dir_all(repo.join("internal/types")).expect("create internal/types");

    std::fs::write(repo.join("go.mod"), "module daemon\n\ngo 1.22\n").expect("write go.mod");
    std::fs::write(
        repo.join("cmd/server/main.go"),
        r#"package main

import "daemon/internal/bridge"

func main() {
    _ = bridge.Make()
}
"#,
    )
    .expect("write main.go");
    std::fs::write(
        repo.join("internal/bridge/bridge.go"),
        r#"package bridge

import "daemon/internal/types"

func Make() types.Payload {
    return types.Payload{}
}
"#,
    )
    .expect("write bridge.go");
    std::fs::write(
        repo.join("internal/types/types.go"),
        r#"package types

type Payload struct{}
"#,
    )
    .expect("write types.go");

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[settings]
output = "."
format = ["json"]

[[project]]
name = "daemon"
repo = "{}"
lang = ["go"]
local_prefix = "daemon"
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

    let graph_path = tmp.path().join("daemon/graph.json");
    let analysis_path = tmp.path().join("daemon/analysis.json");
    let graph: Value =
        serde_json::from_str(&std::fs::read_to_string(&graph_path).expect("read graph.json"))
            .expect("parse graph.json");
    let analysis: Value =
        serde_json::from_str(&std::fs::read_to_string(&analysis_path).expect("read analysis.json"))
            .expect("parse analysis.json");

    let nodes = graph["nodes"].as_array().expect("graph nodes array");
    assert!(
        nodes
            .iter()
            .all(|node| { !node["id"].as_str().expect("node id").starts_with('.') }),
        "graph.json should not contain dot-prefixed placeholder IDs: {graph:#}"
    );

    let links = graph["links"].as_array().expect("graph links array");
    let define_links: Vec<_> = links
        .iter()
        .filter(|link| link["kind"].as_str() == Some("Defines"))
        .collect();
    assert!(
        define_links.iter().all(|link| {
            link["confidence_kind"].as_str() == Some("Extracted")
                && !link["target"]
                    .as_str()
                    .expect("define target")
                    .starts_with('.')
        }),
        "define edges should preserve canonical symbol IDs and extracted confidence: {graph:#}"
    );

    let bridge = analysis["nodes"]
        .as_array()
        .expect("analysis nodes array")
        .iter()
        .find(|node| node["id"].as_str() == Some("daemon.internal.bridge"))
        .expect("bridge package metric");
    assert!(
        bridge["betweenness"].as_f64().unwrap_or(0.0) > 0.0,
        "bridge package should keep non-zero betweenness: {analysis:#}"
    );
}
