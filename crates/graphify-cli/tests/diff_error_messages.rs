use std::fs;
use std::process::Command;

// Minimal HistoricalSnapshot JSON — must match crates/graphify-core/src/history.rs.
const TREND_SNAPSHOT_JSON: &str = r#"{
    "captured_at": 1776203987090556000,
    "project": "demo",
    "summary": {
        "total_nodes": 0,
        "total_edges": 0,
        "total_communities": 0,
        "total_cycles": 0
    },
    "top_hotspots": [],
    "confidence_summary": {
        "extracted_count": 0,
        "extracted_pct": 0.0,
        "inferred_count": 0,
        "inferred_pct": 0.0,
        "ambiguous_count": 0,
        "ambiguous_pct": 0.0,
        "mean_confidence": 1.0
    },
    "nodes": [],
    "communities": []
}"#;

// Minimal AnalysisSnapshot JSON — must match crates/graphify-core/src/diff.rs.
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
fn diff_rejects_trend_snapshot_with_explanatory_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let trend_path = dir.path().join("trend.json");
    let analysis_path = dir.path().join("analysis.json");
    fs::write(&trend_path, TREND_SNAPSHOT_JSON).unwrap();
    fs::write(&analysis_path, MINIMAL_ANALYSIS_JSON).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "diff",
            "--before",
            trend_path.to_str().unwrap(),
            "--after",
            analysis_path.to_str().unwrap(),
        ])
        .output()
        .expect("run diff");

    assert!(!output.status.success(), "diff unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("trend-format history snapshot"),
        "stderr did not mention trend-format: {stderr}"
    );
    assert!(
        stderr.contains("graphify trend"),
        "stderr did not mention graphify trend: {stderr}"
    );
    assert!(
        stderr.contains("baseline.json"),
        "stderr did not mention baseline.json recipe: {stderr}"
    );
    // The raw serde error must NOT leak — that was the BUG-014 symptom.
    assert!(
        !stderr.contains("missing field `betweenness`"),
        "raw serde error leaked into stderr: {stderr}"
    );
}

#[test]
fn diff_rejects_malformed_json_with_generic_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bogus = dir.path().join("bogus.json");
    let analysis_path = dir.path().join("analysis.json");
    fs::write(&bogus, "{ not valid json").unwrap();
    fs::write(&analysis_path, MINIMAL_ANALYSIS_JSON).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "diff",
            "--before",
            bogus.to_str().unwrap(),
            "--after",
            analysis_path.to_str().unwrap(),
        ])
        .output()
        .expect("run diff");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid analysis JSON"),
        "stderr did not contain generic error: {stderr}"
    );
    // Malformed JSON must NOT be misclassified as a trend snapshot.
    assert!(
        !stderr.contains("trend-format"),
        "malformed JSON was misclassified as trend: {stderr}"
    );
}
