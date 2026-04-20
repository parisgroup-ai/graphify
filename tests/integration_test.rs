use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: path to the compiled graphify binary
// ---------------------------------------------------------------------------

/// Returns the path to `target/debug/graphify`.
/// `CARGO_MANIFEST_DIR` resolves to the workspace root at compile time.
fn graphify_bin() -> PathBuf {
    static GRAPHIFY_BIN: OnceLock<PathBuf> = OnceLock::new();

    GRAPHIFY_BIN
        .get_or_init(|| {
            let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let status = Command::new("cargo")
                .current_dir(&workspace_root)
                .arg("build")
                .arg("-q")
                .arg("-p")
                .arg("graphify-cli")
                .arg("--bin")
                .arg("graphify")
                .status()
                .expect("build graphify binary for integration tests");

            assert!(
                status.success(),
                "cargo build -p graphify-cli --bin graphify exited with non-zero status"
            );

            workspace_root.join("target/debug/graphify")
        })
        .clone()
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
// Test 1b: aliased Python imports do not create placeholder alias nodes
// ---------------------------------------------------------------------------

#[test]
fn test_python_alias_import_resolves_to_canonical_target() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo_dir = tmp.path().join("repo");
    let out_dir = tmp.path().join("output");

    std::fs::create_dir_all(repo_dir.join("app/models")).expect("create models dir");
    std::fs::create_dir_all(repo_dir.join("app/routers")).expect("create routers dir");

    std::fs::write(
        repo_dir.join("app/models/tokens.py"),
        "class TokenUsageWithCost:\n    pass\n",
    )
    .expect("write tokens.py");
    std::fs::write(
        repo_dir.join("app/routers/course_metadata.py"),
        "from app.models.tokens import TokenUsageWithCost as TokenUsage\n\n\
def make():\n    return TokenUsage()\n",
    )
    .expect("write course_metadata.py");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "alias_fixture"
repo = "{repo}"
lang = ["python"]
local_prefix = "app"
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        repo = repo_dir
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

    let graph_json_path = out_dir.join("alias_fixture").join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let nodes = graph["nodes"].as_array().expect("nodes must be an array");
    let node_ids: Vec<&str> = nodes
        .iter()
        .filter_map(|node| node["id"].as_str())
        .collect();

    assert!(
        node_ids.contains(&"app.models.tokens.TokenUsageWithCost"),
        "canonical target node must exist, got {:?}",
        node_ids
    );
    assert!(
        !node_ids.contains(&"TokenUsage"),
        "alias placeholder node must not be created, got {:?}",
        node_ids
    );

    let links = graph["links"].as_array().expect("links must be an array");
    let has_canonical_call = links.iter().any(|link| {
        link["source"] == "app.routers.course_metadata"
            && link["target"] == "app.models.tokens.TokenUsageWithCost"
    });
    assert!(
        has_canonical_call,
        "expected edge to canonical alias target in graph.json"
    );
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
#[ignore] // FEAT-003: Go extractor not yet implemented
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
#[ignore] // FEAT-003: Rust extractor not yet implemented
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
#[ignore] // BUG-012: summary JSON missing communities count
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
#[ignore] // BUG-013: stale project directory pruning not yet implemented
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
#[ignore] // FEAT-011: auto-detect local_prefix not yet implemented
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

#[test]
fn test_check_fails_on_policy_rule_for_cross_feature_imports() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("feature_repo");
    let src = repo.join("src");
    let features = src.join("features");

    std::fs::create_dir_all(&features).expect("create features dir");

    std::fs::write(
        features.join("billing.ts"),
        b"import './identity'; export const billing = 1;",
    )
    .expect("write billing module");
    std::fs::write(features.join("identity.ts"), b"export const identity = 1;")
        .expect("write identity module");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "feature_fixture"
repo = "{repo}"
lang = ["typescript"]
local_prefix = "src"

[[policy.group]]
name = "feature"
match = ["src.features.*"]
partition_by = "segment:2"

[[policy.rule]]
name = "no-cross-feature-imports"
kind = "deny"
from = ["group:feature"]
to = ["group:feature"]
allow_same_partition = true
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
        .output()
        .expect("launch graphify binary");

    assert!(
        !output.status.success(),
        "graphify check should fail when policy rules are violated.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("no-cross-feature-imports"),
        "expected policy rule name in output, got:\n{stdout}"
    );
}

#[test]
fn test_check_policy_json_reports_violations() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("feature_repo");
    let src = repo.join("src");
    let features = src.join("features");

    std::fs::create_dir_all(&features).expect("create features dir");
    std::fs::write(
        features.join("billing.ts"),
        b"import './identity'; export const billing = 1;",
    )
    .expect("write billing module");
    std::fs::write(features.join("identity.ts"), b"export const identity = 1;")
        .expect("write identity module");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "feature_fixture"
repo = "{repo}"
lang = ["typescript"]
local_prefix = "src"

[[policy.group]]
name = "feature"
match = ["src.features.*"]
partition_by = "segment:2"

[[policy.rule]]
name = "no-cross-feature-imports"
kind = "deny"
from = ["group:feature"]
to = ["group:feature"]
allow_same_partition = true
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
        .arg("--json")
        .output()
        .expect("launch graphify binary");

    assert!(
        !output.status.success(),
        "graphify check should fail in JSON mode when a policy rule is violated.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse JSON output from graphify check");
    assert_eq!(value["ok"], serde_json::Value::Bool(false));
    assert_eq!(
        value["projects"][0]["policy_summary"]["policy_violations"].as_u64(),
        Some(1)
    );
    assert_eq!(
        value["projects"][0]["violations"][0]["type"].as_str(),
        Some("policy")
    );
    assert_eq!(
        value["projects"][0]["violations"][0]["rule"].as_str(),
        Some("no-cross-feature-imports")
    );
}

#[test]
fn test_check_policy_allows_same_partition_imports() {
    let tmp = TempDir::new().expect("create temp dir");
    let repo = tmp.path().join("feature_repo");
    let src = repo.join("src");
    let billing = src.join("features").join("billing");

    std::fs::create_dir_all(&billing).expect("create billing dir");
    std::fs::write(
        billing.join("api.ts"),
        b"import './service'; export const billingApi = 1;",
    )
    .expect("write api module");
    std::fs::write(
        billing.join("service.ts"),
        b"export const billingService = 1;",
    )
    .expect("write service module");

    let out_dir = tmp.path().join("output");
    let config_path = tmp.path().join("graphify.toml");
    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "feature_fixture"
repo = "{repo}"
lang = ["typescript"]
local_prefix = "src"

[[policy.group]]
name = "feature"
match = ["src.features.*"]
partition_by = "segment:2"

[[policy.rule]]
name = "no-cross-feature-imports"
kind = "deny"
from = ["group:feature"]
to = ["group:feature"]
allow_same_partition = true
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
        .output()
        .expect("launch graphify binary");

    assert!(
        output.status.success(),
        "graphify check should allow imports within the same feature partition.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("All checks passed"),
        "expected successful output, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// FEAT-021 Part B: barrel re-exports collapse to canonical declaration
// ---------------------------------------------------------------------------

/// The `ts_barrel_project` fixture has a 2-level barrel chain:
///
/// ```text
/// consumer.ts  ── import { Course } from './domain'
/// domain/index.ts         ── export { Course } from './entities'
/// domain/entities/index.ts ── export { Course } from './course'
/// domain/entities/course.ts (canonical declaration)
/// ```
///
/// Before Part B each barrel minted its own `Course` Function node
/// (`domain.Course`, `domain.entities.Course`), inflating the symbol count
/// and splitting fan-in. After Part B, only `domain.entities.course.Course`
/// should remain, and it should carry the dropped barrel ids as
/// `alternative_paths`.
#[test]
fn feat_021_part_b_collapses_barrel_chain_to_canonical_node() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_barrel_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "barrel_fixture"
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

    let graph_json_path = out_dir.join("barrel_fixture").join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let nodes = graph["nodes"].as_array().expect("nodes must be an array");
    let node_ids: Vec<&str> = nodes.iter().filter_map(|n| n["id"].as_str()).collect();

    // Canonical declaration survives.
    assert!(
        node_ids.contains(&"src.domain.entities.course.Course"),
        "canonical Course node must exist, got {:?}",
        node_ids
    );
    // Barrel-scoped duplicates must have been collapsed away.
    assert!(
        !node_ids.contains(&"src.domain.Course"),
        "outer barrel Course node must have been collapsed, got {:?}",
        node_ids
    );
    assert!(
        !node_ids.contains(&"src.domain.entities.Course"),
        "inner barrel Course node must have been collapsed, got {:?}",
        node_ids
    );

    // Canonical node carries the dropped barrel ids on `alternative_paths`.
    let canonical = nodes
        .iter()
        .find(|n| n["id"] == "src.domain.entities.course.Course")
        .expect("canonical node present");
    let alts = canonical["alternative_paths"]
        .as_array()
        .expect("alternative_paths array on canonical node");
    let alt_strs: Vec<&str> = alts.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        alt_strs.contains(&"src.domain.Course"),
        "outer barrel id must appear on alternative_paths, got {:?}",
        alt_strs
    );
    assert!(
        alt_strs.contains(&"src.domain.entities.Course"),
        "inner barrel id must appear on alternative_paths, got {:?}",
        alt_strs
    );
}

// ---------------------------------------------------------------------------
// FEAT-026: TS named-import edges target canonical modules, not barrels
// ---------------------------------------------------------------------------

/// The `ts_barrel_project` fixture also exercises the module-level fan-out
/// side of the barrel-collapse story:
///
/// ```text
/// consumer.ts ── import { Course } from './domain'
/// domain/index.ts         ── export { Course } from './entities'
/// domain/entities/index.ts ── export { Course } from './course'
/// entities/course.ts       ── export class Course {}   ← canonical
/// ```
///
/// Pre-FEAT-026 the module-level `Imports` edge from `src.consumer` landed
/// on `src.domain` (the barrel), inflating the barrel's module-layer
/// fan-in even though FEAT-021 Part B already canonicalised the symbol
/// layer. Post-FEAT-026 the edge fans out per specifier through the
/// re-export graph to the canonical declaration module.
#[test]
fn feat_026_named_imports_fan_out_to_canonical_modules() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_barrel_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "barrel_fixture"
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

    let graph_json_path = out_dir.join("barrel_fixture").join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let links = graph["links"]
        .as_array()
        .expect("links must be an array in graph.json");

    // Collect every `Imports` edge as `(source, target)` pairs.
    let imports: Vec<(&str, &str)> = links
        .iter()
        .filter(|link| link["kind"].as_str() == Some("Imports"))
        .filter_map(|link| {
            let s = link["source"].as_str()?;
            let t = link["target"].as_str()?;
            Some((s, t))
        })
        .collect();

    // The consumer's `import { Course } from './domain'` must resolve to a
    // module-level Imports edge targeting the canonical declaration module
    // (`src.domain.entities.course`), not the barrel `src.domain`.
    assert!(
        imports
            .iter()
            .any(|(s, t)| *s == "src.consumer" && *t == "src.domain.entities.course"),
        "expected Imports edge src.consumer -> src.domain.entities.course, got {:?}",
        imports
    );

    // Conversely, the consumer must NOT have an Imports edge to either
    // barrel module — FEAT-026's whole point is that the barrel stops
    // aggregating module-level fan-in from named imports.
    assert!(
        !imports
            .iter()
            .any(|(s, t)| *s == "src.consumer" && *t == "src.domain"),
        "barrel edge src.consumer -> src.domain must have been fanned out, got {:?}",
        imports
    );
    assert!(
        !imports
            .iter()
            .any(|(s, t)| *s == "src.consumer" && *t == "src.domain.entities"),
        "barrel edge src.consumer -> src.domain.entities must have been fanned out, got {:?}",
        imports
    );
}

// ---------------------------------------------------------------------------
// FEAT-027: tsconfig.json `paths` aliases interacting with barrel collapse
// ---------------------------------------------------------------------------

/// Spike verification: when a tsconfig `paths` alias points at a *same-project*
/// barrel (`@app/*` → `src/*` with a re-exporting `index.ts`), FEAT-026's
/// module-level fan-out already walks through it. The alias resolves to a
/// module id inside the project's walker-discovered set (`is_local = true`),
/// the barrel sits in the per-project `ReExportGraph`, and the named-import
/// specifier lands on the canonical module.
///
/// Fixture shape:
/// ```text
/// tsconfig.json   paths: { "@app/*": ["src/*"] }
/// src/consumer.ts              ── import { Course } from '@app/domain'
/// src/domain/index.ts          ── export { Course } from './entities'
/// src/domain/entities/index.ts ── export { Course } from './course'
/// src/domain/entities/course.ts (canonical declaration)
/// ```
#[test]
fn feat_027_same_project_tsconfig_alias_fans_out_to_canonical() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_tsconfig_alias_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "alias_fixture"
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

    let graph_json_path = out_dir.join("alias_fixture").join("graph.json");
    let raw = std::fs::read_to_string(&graph_json_path).expect("read graph.json");
    let graph: serde_json::Value = serde_json::from_str(&raw).expect("parse graph.json");

    let links = graph["links"]
        .as_array()
        .expect("links must be an array in graph.json");

    let imports: Vec<(&str, &str)> = links
        .iter()
        .filter(|link| link["kind"].as_str() == Some("Imports"))
        .filter_map(|link| {
            let s = link["source"].as_str()?;
            let t = link["target"].as_str()?;
            Some((s, t))
        })
        .collect();

    assert!(
        imports
            .iter()
            .any(|(s, t)| *s == "src.consumer" && *t == "src.domain.entities.course"),
        "same-project alias: expected canonical Imports edge, got {:?}",
        imports
    );
    assert!(
        !imports
            .iter()
            .any(|(s, t)| *s == "src.consumer" && *t == "src.domain"),
        "same-project alias: barrel edge must have been fanned out, got {:?}",
        imports
    );
}

/// Spike verification: when a tsconfig `paths` alias crosses project
/// boundaries (`@repo/*` → `../../packages/*/src` with consumer and core
/// configured as separate `[[project]]`s), FEAT-026's fan-out does NOT walk
/// through the barrel — the alias target lands outside the consumer's
/// walker-discovered set, the resolver returns the raw alias string, and
/// the per-project `ReExportGraph` has no entries for the core barrel.
///
/// This test pins the current v1 contract. It is the intentional tripwire
/// for a future FEAT-028 that would merge re-export graphs across
/// FEAT-028 contract: cross-project alias-through-barrel now fans out to the
/// canonical declaration in the sibling project.
///
/// Previously (FEAT-027 spike / v1 tripwire) the same fixture terminated at
/// the raw alias string `@repo/core` because each `[[project]]` built its
/// own per-project `ReExportGraph` that couldn't cross workspace boundaries.
/// FEAT-028 step 5 introduces a workspace-wide `WorkspaceReExportGraph` —
/// aggregated in `graphify-cli::main::collect_workspace_reexport_graph`
/// before any project's fan-out — and the TS named-import fan-out at
/// `run_extract_with_workspace` now consults
/// `ModuleResolver::apply_ts_alias_workspace` +
/// `WorkspaceReExportGraph::resolve_canonical_cross_project` for non-local
/// barrels. The consumer's `import { Foo } from '@repo/core'` therefore
/// emits an `Imports` edge directly to the sibling's canonical module
/// (`src.foo` in the `core` project), not to the raw alias.
///
/// Fixture shape (pnpm/turbo-style workspace):
/// ```text
/// apps/consumer/tsconfig.json   paths: { "@repo/*": ["../../packages/*/src"] }
/// apps/consumer/src/main.ts     ── import { Foo } from '@repo/core'
/// packages/core/src/index.ts    ── export { Foo } from './foo'
/// packages/core/src/foo.ts      (canonical declaration in a separate project)
/// ```
#[test]
fn feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_cross_project_alias");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let fixture_canonical = fixture_dir
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .replace('\\', "/");

    let config_content = format!(
        r#"[settings]
output = "{output}"

[[project]]
name = "consumer"
repo = "{fixture}/apps/consumer"
lang = ["typescript"]

[[project]]
name = "core"
repo = "{fixture}/packages/core"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        fixture = fixture_canonical,
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

    // Consumer project: cross-project fan-out should emit an `Imports`
    // edge directly to the sibling's canonical module (`src.foo` in the
    // `core` project), bypassing the `@repo/core` barrel entirely.
    let consumer_graph: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join("consumer").join("graph.json"))
            .expect("read consumer graph.json"),
    )
    .expect("parse consumer graph.json");

    let consumer_imports: Vec<(&str, &str)> = consumer_graph["links"]
        .as_array()
        .expect("links array")
        .iter()
        .filter(|link| link["kind"].as_str() == Some("Imports"))
        .filter_map(|link| Some((link["source"].as_str()?, link["target"].as_str()?)))
        .collect();

    assert!(
        consumer_imports
            .iter()
            .any(|(s, t)| *s == "src.main" && *t == "src.foo"),
        "cross-project fan-out: expected canonical edge src.main -> src.foo, got {:?}",
        consumer_imports
    );

    // The raw `@repo/core` alias must no longer appear as an edge target on
    // the consumer side — FEAT-028 eliminates it in favour of the canonical.
    assert!(
        !consumer_imports.iter().any(|(_, t)| *t == "@repo/core"),
        "cross-project fan-out: raw alias '@repo/core' must not appear as an \
         edge target post-FEAT-028, got {:?}",
        consumer_imports
    );

    // The `@repo/core` node should be gone from the consumer's node set —
    // no edges reach it, and the extractor doesn't synthesise it as a
    // standalone node (the placeholder is created implicitly by
    // `CodeGraph::add_edge`, so eliminating the edge eliminates the node).
    let consumer_node_ids: Vec<&str> = consumer_graph["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();
    assert!(
        !consumer_node_ids.iter().any(|id| *id == "@repo/core"),
        "cross-project fan-out: '@repo/core' node must be removed post-FEAT-028, \
         got {:?}",
        consumer_node_ids
    );

    // Core project side is unchanged: the canonical `Foo` class node still
    // exists in its own project. FEAT-028 does NOT promote the canonical
    // into the consumer's node set — cross-project edges target the
    // sibling's id verbatim per option-2 namespacing ADR (slice 1).
    let core_graph: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join("core").join("graph.json"))
            .expect("read core graph.json"),
    )
    .expect("parse core graph.json");

    let core_node_ids: Vec<&str> = core_graph["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();

    assert!(
        core_node_ids.contains(&"src.foo.Foo"),
        "core canonical Foo class node must exist in its own project, got {:?}",
        core_node_ids
    );
}

/// FEAT-030: the opt-out `[settings] workspace_reexport_graph = false`
/// must force the legacy per-project fan-out path — cross-project aliases
/// stay at the raw `@repo/*` barrel instead of resolving to the sibling's
/// canonical module. This is the exact inverse of
/// `feat_028_cross_project_alias_fans_out_to_canonical_workspace_scope`.
#[test]
fn feat_030_opt_out_flag_restores_legacy_cross_project_path() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_cross_project_alias");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

    let fixture_canonical = fixture_dir
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .replace('\\', "/");

    let config_content = format!(
        r#"[settings]
output = "{output}"
workspace_reexport_graph = false

[[project]]
name = "consumer"
repo = "{fixture}/apps/consumer"
lang = ["typescript"]

[[project]]
name = "core"
repo = "{fixture}/packages/core"
lang = ["typescript"]
"#,
        output = out_dir.to_str().unwrap().replace('\\', "/"),
        fixture = fixture_canonical,
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

    let consumer_graph: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join("consumer").join("graph.json"))
            .expect("read consumer graph.json"),
    )
    .expect("parse consumer graph.json");

    let consumer_imports: Vec<(&str, &str)> = consumer_graph["links"]
        .as_array()
        .expect("links array")
        .iter()
        .filter(|link| link["kind"].as_str() == Some("Imports"))
        .filter_map(|link| Some((link["source"].as_str()?, link["target"].as_str()?)))
        .collect();

    // Legacy path: the raw `@repo/core` alias is the import target — no
    // canonical resolution across projects.
    assert!(
        consumer_imports.iter().any(|(_, t)| *t == "@repo/core"),
        "opt-out: legacy path must keep '@repo/core' as an edge target, got {:?}",
        consumer_imports
    );

    // Canonical `src.foo` must NOT appear — that's the FEAT-028 behaviour
    // the flag explicitly suppresses.
    assert!(
        !consumer_imports
            .iter()
            .any(|(s, t)| *s == "src.main" && *t == "src.foo"),
        "opt-out: canonical edge src.main -> src.foo must not appear when \
         workspace_reexport_graph = false, got {:?}",
        consumer_imports
    );
}
