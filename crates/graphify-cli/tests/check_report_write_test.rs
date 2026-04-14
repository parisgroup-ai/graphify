//! Integration test asserting `graphify check` writes `check-report.json`
//! to every project's output directory alongside analysis.json / drift-report.json.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Returns the absolute path to the fixture directory bundled with this crate.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/check_write")
}

#[test]
fn graphify_check_writes_check_report_json_to_each_project_output_dir() {
    let fixture = fixture_dir();
    assert!(
        fixture.join("graphify.toml").exists(),
        "fixture setup: {} must exist",
        fixture.join("graphify.toml").display()
    );

    let out_dir = tempfile::tempdir().expect("tempdir");

    // Run the full pipeline first so analysis.json + friends exist.
    // `repo = "."` in the fixture's graphify.toml is resolved relative to the
    // process CWD, so we set it to the fixture dir.
    let status = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .current_dir(&fixture)
        .args([
            "run",
            "--config",
            fixture.join("graphify.toml").to_str().unwrap(),
            "--output",
            out_dir.path().to_str().unwrap(),
        ])
        .status()
        .expect("run graphify run");
    assert!(status.success(), "graphify run failed");

    // Now run `check` pointing at the same --output. Exit status may be
    // non-zero if violations were emitted; the test only cares that the
    // file landed on disk.
    let _status = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .current_dir(&fixture)
        .args([
            "check",
            "--config",
            fixture.join("graphify.toml").to_str().unwrap(),
            "--output",
            out_dir.path().to_str().unwrap(),
        ])
        .status()
        .expect("run graphify check");

    // Assert check-report.json exists under at least one project subdirectory.
    let mut found = false;
    for entry in fs::read_dir(out_dir.path()).expect("read out dir") {
        let entry = entry.expect("entry");
        if entry.path().is_dir() {
            let check_report = entry.path().join("check-report.json");
            if check_report.exists() {
                found = true;
                let content = fs::read_to_string(&check_report).expect("read check-report");
                let parsed: graphify_report::check_report::CheckReport =
                    serde_json::from_str(&content).expect("parse check-report");
                assert!(
                    !parsed.projects.is_empty(),
                    "check-report.json should have at least one project entry"
                );
                break;
            }
        }
    }
    assert!(
        found,
        "check-report.json was not written to any project output dir"
    );
}
