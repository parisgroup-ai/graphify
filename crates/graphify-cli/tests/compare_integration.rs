use std::fs;
use std::process::Command;

const EMPTY_ANALYSIS_JSON: &str = r#"{
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

const ONE_NODE_ANALYSIS_JSON: &str = r#"{
    "nodes": [
        {
            "id": "app.new",
            "betweenness": 0.0,
            "pagerank": 0.0,
            "in_degree": 0,
            "out_degree": 0,
            "in_cycle": false,
            "score": 0.10,
            "community_id": 0
        }
    ],
    "communities": [
        { "id": 0, "members": ["app.new"] }
    ],
    "cycles": [],
    "summary": {
        "total_nodes": 1,
        "total_edges": 0,
        "total_communities": 1,
        "total_cycles": 0
    }
}"#;

#[test]
fn compare_accepts_analysis_file_inputs_and_writes_labeled_reports() {
    let dir = tempfile::tempdir().expect("tempdir");
    let left = dir.path().join("before.json");
    let right = dir.path().join("after.json");
    let out = dir.path().join("out");
    fs::write(&left, EMPTY_ANALYSIS_JSON).unwrap();
    fs::write(&right, ONE_NODE_ANALYSIS_JSON).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "compare",
            left.to_str().unwrap(),
            right.to_str().unwrap(),
            "--left-label",
            "PR-123",
            "--right-label",
            "PR-456",
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("run compare");

    assert!(
        output.status.success(),
        "compare failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Architecture Compare Report"));
    assert!(stdout.contains("Left:        PR-123"));
    assert!(stdout.contains("Right:       PR-456"));
    assert!(stdout.contains("Nodes:       0 → 1 (+1)"));

    let md = fs::read_to_string(out.join("compare-report.md")).unwrap();
    assert!(md.contains("# Architecture Compare Report"));
    assert!(md.contains("| Metric | PR-123 | PR-456 | Change |"));
    assert!(md.contains("| Nodes | 0 | 1 | +1 |"));

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("compare-report.json")).unwrap())
            .unwrap();
    assert_eq!(json["left_label"], "PR-123");
    assert_eq!(json["right_label"], "PR-456");
    assert_eq!(json["diff"]["summary_delta"]["nodes"]["change"], 1);
}

#[test]
fn compare_accepts_directories_containing_analysis_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let left_dir = dir.path().join("left-pr");
    let right_dir = dir.path().join("right-pr");
    let out = dir.path().join("out");
    fs::create_dir(&left_dir).unwrap();
    fs::create_dir(&right_dir).unwrap();
    fs::write(left_dir.join("analysis.json"), EMPTY_ANALYSIS_JSON).unwrap();
    fs::write(right_dir.join("analysis.json"), ONE_NODE_ANALYSIS_JSON).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "compare",
            left_dir.to_str().unwrap(),
            right_dir.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("run compare");

    assert!(
        output.status.success(),
        "compare failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Left:        left-pr"));
    assert!(stdout.contains("Right:       right-pr"));

    let md = fs::read_to_string(out.join("compare-report.md")).unwrap();
    assert!(md.contains("| Metric | left-pr | right-pr | Change |"));
}

#[test]
fn compare_rejects_directory_without_analysis_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let left_dir = dir.path().join("left-pr");
    let right_dir = dir.path().join("right-pr");
    fs::create_dir(&left_dir).unwrap();
    fs::create_dir(&right_dir).unwrap();
    fs::write(right_dir.join("analysis.json"), ONE_NODE_ANALYSIS_JSON).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "compare",
            left_dir.to_str().unwrap(),
            right_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run compare");

    assert!(!output.status.success(), "compare unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("graphify compare"));
    assert!(stderr.contains("left directory"));
    assert!(stderr.contains("missing analysis.json"));
}
