use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the path to `target/debug/graphify`.
fn graphify_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/graphify")
}

/// Creates a TempDir with a graphify.toml pointing at the Python fixture.
/// Returns (TempDir, config_path).
fn setup_config() -> (TempDir, PathBuf) {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/python_project");

    let tmp = TempDir::new().expect("create temp dir");
    let out_dir = tmp.path().join("output");

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
    std::fs::write(&config_path, &config_content).expect("write config");

    (tmp, config_path)
}

// ---------------------------------------------------------------------------
// Test 1: query command runs and finds nodes
// ---------------------------------------------------------------------------

#[test]
fn query_command_runs_and_finds_nodes() {
    let (_tmp, config_path) = setup_config();

    let output = Command::new(graphify_bin())
        .arg("query")
        .arg("app.*")
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("launch graphify binary");

    assert!(
        output.status.success(),
        "graphify query exited with non-zero status: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("match"),
        "stdout should contain match info, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: query command with --json returns valid JSON array
// ---------------------------------------------------------------------------

#[test]
fn query_command_json_output() {
    let (_tmp, config_path) = setup_config();

    let output = Command::new(graphify_bin())
        .arg("query")
        .arg("app.*")
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("launch graphify binary");

    assert!(
        output.status.success(),
        "graphify query --json exited with non-zero status: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert!(
        parsed.is_array(),
        "JSON output should be an array, got: {parsed}"
    );

    let arr = parsed.as_array().unwrap();
    assert!(
        !arr.is_empty(),
        "JSON array should not be empty for 'app.*' pattern"
    );
}

// ---------------------------------------------------------------------------
// Test 3: explain command shows report for existing node
// ---------------------------------------------------------------------------

#[test]
fn explain_command_shows_report() {
    let (_tmp, config_path) = setup_config();

    let output = Command::new(graphify_bin())
        .arg("explain")
        .arg("app.main")
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("launch graphify binary");

    assert!(
        output.status.success(),
        "graphify explain exited with non-zero status: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("app.main"),
        "stdout should contain the node ID 'app.main', got:\n{stdout}"
    );
    assert!(
        stdout.contains("Metrics"),
        "stdout should contain 'Metrics' section, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Test 4: explain unknown node exits non-zero with error message
// ---------------------------------------------------------------------------

#[test]
fn explain_unknown_node_exits_nonzero() {
    let (_tmp, config_path) = setup_config();

    let output = Command::new(graphify_bin())
        .arg("explain")
        .arg("nonexistent.module")
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("launch graphify binary");

    assert!(
        !output.status.success(),
        "graphify explain for nonexistent node should exit non-zero"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("Not found") || stderr.contains("Node"),
        "stderr should indicate node was not found, got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Test 5: path command runs without panicking
// ---------------------------------------------------------------------------

#[test]
fn path_command_runs() {
    let (_tmp, config_path) = setup_config();

    let output = Command::new(graphify_bin())
        .arg("path")
        .arg("app.main")
        .arg("app.services.llm")
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("launch graphify binary");

    // The command may or may not find a path depending on edge direction in
    // the fixture, but it must not panic.  We just verify the process ran
    // (exit code 0 for found, 1 for not found — both are non-panics).
    // A panic would produce exit code 101 on Unix.
    let code = output.status.code().unwrap_or(-1);
    assert!(
        code == 0 || code == 1,
        "graphify path should exit 0 or 1, not panic (code={code}), stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
