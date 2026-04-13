use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: path to the compiled graphify binary
// ---------------------------------------------------------------------------

/// Returns the path to `target/debug/graphify`.
/// `CARGO_MANIFEST_DIR` resolves to the workspace root at compile time.
fn graphify_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/graphify")
}

// ---------------------------------------------------------------------------
// Test 1: full pipeline with the Python fixture
// ---------------------------------------------------------------------------

#[test]
fn test_python_fixture_pipeline() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python_project");

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
        repo = fixture_dir
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
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
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_project");

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
        repo = fixture_dir
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
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
    assert!(
        !nodes.is_empty(),
        "nodes array must not be empty for TypeScript fixture"
    );
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

    assert!(
        status.success(),
        "graphify init exited with non-zero status"
    );

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

// ---------------------------------------------------------------------------
// Test 4: full pipeline with the Go fixture
// ---------------------------------------------------------------------------

#[test]
fn test_go_fixture_pipeline() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/go_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "go_fixture"
repo = "{repo}"
lang = ["go"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = fixture_dir
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
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

    let graph_json_path = out_dir.join("go_fixture").join("graph.json");
    assert!(
        graph_json_path.exists(),
        "graph.json not found at {}",
        graph_json_path.display()
    );

    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let nodes = graph["nodes"].as_array().expect("nodes must be an array");
    assert!(
        !nodes.is_empty(),
        "nodes array must not be empty for Go fixture"
    );

    let links = graph["links"].as_array().expect("links must be an array");
    assert!(
        !links.is_empty(),
        "links array must not be empty for Go fixture"
    );
}

// ---------------------------------------------------------------------------
// Test 5: full pipeline with the Rust fixture
// ---------------------------------------------------------------------------

#[test]
fn test_rust_fixture_pipeline() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "rust_fixture"
repo = "{repo}"
lang = ["rust"]
local_prefix = "src"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = fixture_dir
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
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

    let graph_json_path = out_dir.join("rust_fixture").join("graph.json");
    assert!(
        graph_json_path.exists(),
        "graph.json not found at {}",
        graph_json_path.display()
    );

    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let nodes = graph["nodes"].as_array().expect("nodes must be an array");
    assert!(
        !nodes.is_empty(),
        "nodes array must not be empty for Rust fixture"
    );

    let links = graph["links"].as_array().expect("links must be an array");
    assert!(
        !links.is_empty(),
        "links array must not be empty for Rust fixture"
    );
}

#[test]
fn test_multi_project_summary_includes_communities_per_project() {
    let python_fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python_project");
    let ts_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "python_fixture"
repo = "{python_repo}"
lang = ["python"]
local_prefix = "app"

[[project]]
name = "ts_fixture"
repo = "{ts_repo}"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        python_repo = python_fixture
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        ts_repo = ts_fixture
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
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

    let summary_path = out_dir.join("graphify-summary.json");
    assert!(
        summary_path.exists(),
        "summary file not found at {}",
        summary_path.display()
    );

    let raw = std::fs::read_to_string(&summary_path).expect("read graphify-summary.json");
    let summary: serde_json::Value =
        serde_json::from_str(&raw).expect("parse graphify-summary.json");

    let projects = summary["projects"]
        .as_array()
        .expect("projects must be an array");
    assert_eq!(projects.len(), 2, "expected 2 project entries in summary");

    for project in projects {
        let name = project["name"].as_str().unwrap_or("<unknown>");
        let communities = project["communities"].as_u64();
        assert!(
            communities.is_some(),
            "expected communities count for project {name}, got {project:#?}"
        );
        assert!(
            communities.unwrap() > 0,
            "expected positive communities count for project {name}, got {project:#?}"
        );
    }
}

#[test]
fn test_run_prunes_stale_project_output_directories() {
    let python_fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python_project");
    let ts_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");

    let initial_config = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "python_fixture"
repo = "{python_repo}"
lang = ["python"]
local_prefix = "app"

[[project]]
name = "ts_fixture"
repo = "{ts_repo}"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        python_repo = python_fixture
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        ts_repo = ts_fixture
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, initial_config).expect("write initial config");

    let first_status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch graphify binary");
    assert!(
        first_status.success(),
        "initial graphify run exited with non-zero status"
    );

    let stale_project_dir = out_dir.join("ts_fixture");
    assert!(
        stale_project_dir.exists(),
        "expected initial stale candidate directory at {}",
        stale_project_dir.display()
    );

    let updated_config = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "python_fixture"
repo = "{python_repo}"
lang = ["python"]
local_prefix = "app"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        python_repo = python_fixture
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, updated_config).expect("write updated config");

    let second_status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch graphify binary");
    assert!(
        second_status.success(),
        "second graphify run exited with non-zero status"
    );

    assert!(
        !stale_project_dir.exists(),
        "expected stale project directory to be pruned: {}",
        stale_project_dir.display()
    );
    assert!(
        out_dir.join("python_fixture").exists(),
        "expected active project output to remain after pruning"
    );
}

#[test]
fn test_run_auto_detects_local_prefix_when_config_omits_it() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("repo");
    let src = repo.join("src");
    let lib = repo.join("lib");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::create_dir_all(&lib).expect("create lib dir");
    std::fs::write(
        src.join("index.ts"),
        b"import { api } from './api'; export const root = api;",
    )
    .expect("write src/index.ts");
    std::fs::write(src.join("api.ts"), b"export const api = 1;").expect("write src/api.ts");
    std::fs::write(src.join("feature.ts"), b"export const feature = 1;")
        .expect("write src/feature.ts");
    std::fs::write(lib.join("helper.ts"), b"export const helper = 1;")
        .expect("write lib/helper.ts");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "auto_prefix_fixture"
repo = "{repo}"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = repo
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, config_content).expect("write config");

    let status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch graphify binary");

    assert!(status.success(), "graphify run exited with non-zero status");

    let graph_json_path = out_dir.join("auto_prefix_fixture").join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");
    let node_ids: Vec<&str> = graph["nodes"]
        .as_array()
        .expect("nodes must be an array")
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect();

    assert!(
        node_ids.contains(&"src"),
        "expected src package node in auto-detected project: {:?}",
        node_ids
    );
    assert!(
        node_ids.contains(&"src.api"),
        "expected src-prefixed node after auto-detection: {:?}",
        node_ids
    );
    assert!(
        node_ids.contains(&"src.lib.helper"),
        "expected non-src file to inherit detected prefix: {:?}",
        node_ids
    );
}

#[test]
fn test_run_respects_explicit_empty_local_prefix() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("repo");
    let src = repo.join("src");
    let lib = repo.join("lib");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::create_dir_all(&lib).expect("create lib dir");
    std::fs::write(
        src.join("index.ts"),
        b"import { api } from './api'; export const root = api;",
    )
    .expect("write src/index.ts");
    std::fs::write(src.join("api.ts"), b"export const api = 1;").expect("write src/api.ts");
    std::fs::write(src.join("feature.ts"), b"export const feature = 1;")
        .expect("write src/feature.ts");
    std::fs::write(lib.join("helper.ts"), b"export const helper = 1;")
        .expect("write lib/helper.ts");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "explicit_empty_prefix_fixture"
repo = "{repo}"
lang = ["typescript"]
local_prefix = ""
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = repo
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, config_content).expect("write config");

    let status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch graphify binary");

    assert!(status.success(), "graphify run exited with non-zero status");

    let graph_json_path = out_dir
        .join("explicit_empty_prefix_fixture")
        .join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");
    let node_ids: Vec<&str> = graph["nodes"]
        .as_array()
        .expect("nodes must be an array")
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect();

    assert!(
        node_ids.contains(&"lib.helper"),
        "expected explicit empty local_prefix to preserve root-level naming: {:?}",
        node_ids
    );
    assert!(
        !node_ids.contains(&"src.lib.helper"),
        "explicit local_prefix must remain sovereign over auto-detection: {:?}",
        node_ids
    );
}

#[test]
fn test_check_passes_with_permissive_limits() {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "ts_fixture"
repo = "{repo}"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = fixture_dir
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, config_content).expect("write config");

    let output = Command::new(graphify_bin())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--max-cycles")
        .arg("10")
        .arg("--max-hotspot-score")
        .arg("10")
        .output()
        .expect("launch graphify binary");

    assert!(
        output.status.success(),
        "graphify check should succeed with permissive limits.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PASS") && stdout.contains("All checks passed"),
        "expected human-readable PASS summary, got:\n{stdout}"
    );
}

#[test]
fn test_check_fails_when_cycles_exceed_limit() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("cycle_repo");
    let src = repo.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("a.ts"), b"import './b'; export const a = 1;").expect("write a.ts");
    std::fs::write(src.join("b.ts"), b"import './a'; export const b = 1;").expect("write b.ts");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "cycle_fixture"
repo = "{repo}"
lang = ["typescript"]
local_prefix = "src"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = repo
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, config_content).expect("write config");

    let output = Command::new(graphify_bin())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--max-cycles")
        .arg("0")
        .output()
        .expect("launch graphify binary");

    assert!(
        !output.status.success(),
        "graphify check should fail when cycles exceed the gate.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("FAIL") && stdout.contains("max_cycles"),
        "expected failure summary mentioning max_cycles, got:\n{stdout}"
    );
}

#[test]
fn test_check_json_reports_violations() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("cycle_repo");
    let src = repo.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("a.ts"), b"import './b'; export const a = 1;").expect("write a.ts");
    std::fs::write(src.join("b.ts"), b"import './a'; export const b = 1;").expect("write b.ts");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "cycle_fixture"
repo = "{repo}"
lang = ["typescript"]
local_prefix = "src"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = repo
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, config_content).expect("write config");

    let output = Command::new(graphify_bin())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--max-cycles")
        .arg("0")
        .arg("--json")
        .output()
        .expect("launch graphify binary");

    assert!(
        !output.status.success(),
        "graphify check should fail in JSON mode when gate is violated.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse JSON output from graphify check");
    assert_eq!(
        value["ok"],
        serde_json::Value::Bool(false),
        "expected root ok=false, got {value:#?}"
    );
    assert_eq!(
        value["violations"].as_u64(),
        Some(1),
        "expected exactly one violation, got {value:#?}"
    );
    assert_eq!(
        value["projects"][0]["violations"][0]["kind"].as_str(),
        Some("max_cycles"),
        "expected max_cycles violation, got {value:#?}"
    );
}

#[test]
fn test_check_multi_project_fails_if_one_project_violates() {
    let healthy_fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_project");

    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("cycle_repo");
    let src = repo.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::write(src.join("a.ts"), b"import './b'; export const a = 1;").expect("write a.ts");
    std::fs::write(src.join("b.ts"), b"import './a'; export const b = 1;").expect("write b.ts");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "healthy_fixture"
repo = "{healthy_repo}"
lang = ["typescript"]

[[project]]
name = "cycle_fixture"
repo = "{cycle_repo}"
lang = ["typescript"]
local_prefix = "src"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        healthy_repo = healthy_fixture
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        cycle_repo = repo
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
    );
    std::fs::write(&config_path, config_content).expect("write config");

    let output = Command::new(graphify_bin())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--max-cycles")
        .arg("0")
        .output()
        .expect("launch graphify binary");

    assert!(
        !output.status.success(),
        "graphify check should fail when any project violates the gate.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("healthy_fixture") && stdout.contains("cycle_fixture"),
        "expected both projects in check output, got:\n{stdout}"
    );
}
