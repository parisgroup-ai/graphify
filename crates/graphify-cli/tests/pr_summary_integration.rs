use std::fs;
use std::process::Command;

// Minimal valid AnalysisSnapshot JSON (matches graphify-core/src/diff.rs shape).
const MINIMAL_ANALYSIS_JSON: &str = r#"{
    "nodes": [],
    "communities": [],
    "cycles": [],
    "summary": {
        "total_nodes": 0,
        "total_edges": 0,
        "total_communities": 0,
        "total_cycles": 0
    }
}"#;

#[test]
fn pr_summary_prints_markdown_given_analysis_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Project name comes from the directory basename, so create a named subdir.
    let project_dir = dir.path().join("my-app");
    fs::create_dir(&project_dir).unwrap();

    fs::write(project_dir.join("analysis.json"), MINIMAL_ANALYSIS_JSON)
        .expect("write analysis.json");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", project_dir.to_str().unwrap()])
        .output()
        .expect("run pr-summary");

    assert!(
        output.status.success(),
        "pr-summary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("### Graphify — Architecture Delta for `my-app`"));
    assert!(stdout.contains("#### Drift in this PR"));
    assert!(stdout.contains("_No drift baseline"));
    // No outstanding-issues section when no check-report.json
    assert!(!stdout.contains("#### Outstanding issues"));
    // Footer present
    assert!(stdout.contains("graphify pr-summary <dir>"));
}

#[test]
fn pr_summary_exits_non_zero_when_directory_missing() {
    let missing = std::path::Path::new("/tmp/graphify-pr-summary-does-not-exist-xyz");
    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", missing.to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn pr_summary_exits_non_zero_when_analysis_json_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", dir.path().to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing analysis.json"));
}

#[test]
fn pr_summary_detects_multi_project_root() {
    let root = tempfile::tempdir().expect("tempdir");
    // Create two project subdirs, each with its own analysis.json.
    for project in &["web", "api"] {
        let p = root.path().join(project);
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("analysis.json"), MINIMAL_ANALYSIS_JSON).unwrap();
    }
    // Root itself has no analysis.json.
    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", root.path().to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("multi-project output root"));
}

#[test]
fn pr_summary_warns_and_continues_on_malformed_drift_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("analysis.json"), MINIMAL_ANALYSIS_JSON).unwrap();
    std::fs::write(dir.path().join("drift-report.json"), "{not valid json").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", dir.path().to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(
        output.status.success(),
        "pr-summary should not fail on malformed optional input"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to parse drift-report.json"));
}
