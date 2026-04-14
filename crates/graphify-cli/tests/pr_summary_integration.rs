use std::fs;
use std::process::Command;

#[test]
fn pr_summary_prints_markdown_given_analysis_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Project name comes from the directory basename, so create a named subdir.
    let project_dir = dir.path().join("my-app");
    fs::create_dir(&project_dir).unwrap();

    // Minimal valid AnalysisSnapshot JSON (matches graphify-core/src/diff.rs shape).
    let analysis_json = r#"{
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
    fs::write(project_dir.join("analysis.json"), analysis_json).expect("write analysis.json");

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
