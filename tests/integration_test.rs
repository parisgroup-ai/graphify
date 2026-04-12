use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: path to the compiled graphify binary
// ---------------------------------------------------------------------------

/// Returns the path to `target/debug/graphify`.
/// `CARGO_MANIFEST_DIR` resolves to the workspace root at compile time.
fn graphify_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/debug/graphify")
}

// ---------------------------------------------------------------------------
// Test 1: full pipeline with the Python fixture
// ---------------------------------------------------------------------------

#[test]
fn test_python_fixture_pipeline() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/python_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    // Write a minimal graphify.toml pointing at the Python fixture.
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "python_fixture"
repo = "{repo}"
lang = ["python"]
local_prefix = "app"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = fixture_dir.canonicalize().unwrap().to_str().unwrap().replace('\\', "/"),
    );

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(&config_path, config_content).expect("write config");

    // Run the full pipeline.
    let status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch graphify binary");

    assert!(status.success(), "graphify run exited with non-zero status");

    // All 5 expected output files must exist under out_dir/python_fixture/.
    let proj_out = out_dir.join("python_fixture");
    let expected_files = [
        "graph.json",
        "analysis.json",
        "graph_nodes.csv",
        "graph_edges.csv",
        "architecture_report.md",
    ];
    for file_name in &expected_files {
        let path = proj_out.join(file_name);
        assert!(
            path.exists(),
            "expected output file missing: {}",
            path.display()
        );
    }

    // Parse graph.json and verify structure.
    let graph_json_path = proj_out.join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    assert_eq!(
        graph["directed"],
        serde_json::Value::Bool(true),
        "graph.json must have directed=true"
    );

    let nodes = graph["nodes"].as_array().expect("nodes must be an array");
    assert!(!nodes.is_empty(), "nodes array must not be empty");

    let links = graph["links"].as_array().expect("links must be an array");
    assert!(!links.is_empty(), "links array must not be empty");
}

// ---------------------------------------------------------------------------
// Test 2: full pipeline with the TypeScript fixture
// ---------------------------------------------------------------------------

#[test]
fn test_typescript_fixture_pipeline() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ts_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "ts_fixture"
repo = "{repo}"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = fixture_dir.canonicalize().unwrap().to_str().unwrap().replace('\\', "/"),
    );

    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(&config_path, config_content).expect("write config");

    let status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch graphify binary");

    assert!(status.success(), "graphify run exited with non-zero status");

    // graph.json must exist and nodes must not be empty.
    let graph_json_path = out_dir.join("ts_fixture").join("graph.json");
    assert!(
        graph_json_path.exists(),
        "graph.json not found at {}",
        graph_json_path.display()
    );

    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let nodes = graph["nodes"].as_array().expect("nodes must be an array");
    assert!(!nodes.is_empty(), "nodes array must not be empty for TypeScript fixture");
}

// ---------------------------------------------------------------------------
// Test 3: `graphify init` creates a config template
// ---------------------------------------------------------------------------

#[test]
fn test_init_creates_config() {
    let tmp = TempDir::new().expect("create temp dir");

    let status = Command::new(graphify_bin())
        .arg("init")
        .current_dir(tmp.path())
        .status()
        .expect("launch graphify binary");

    assert!(status.success(), "graphify init exited with non-zero status");

    let config_path = tmp.path().join("graphify.toml");
    assert!(
        config_path.exists(),
        "graphify.toml was not created by `graphify init`"
    );

    let contents = std::fs::read_to_string(&config_path).expect("read graphify.toml");
    assert!(
        contents.contains("[[project]]"),
        "graphify.toml must contain [[project]] section, got:\n{contents}"
    );
}
