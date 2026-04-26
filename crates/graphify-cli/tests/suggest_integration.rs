//! End-to-end tests for `graphify suggest stubs`.

use std::path::PathBuf;
use std::process::Command;

fn graphify_bin() -> PathBuf {
    // CARGO_BIN_EXE_<name> is set by Cargo during integration test build.
    PathBuf::from(env!("CARGO_BIN_EXE_graphify"))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("suggest")
}

#[test]
fn suggest_stubs_md_output_contains_expected_sections() {
    let dir = fixture_dir();
    // The fixture's `[settings].output = "."` is cwd-relative — run from the
    // fixture dir so `./proj-a/graph.json` resolves correctly.
    let output = Command::new(graphify_bin())
        .current_dir(&dir)
        .args([
            "suggest",
            "stubs",
            "--config",
            "graphify.toml",
            "--format",
            "md",
        ])
        .output()
        .expect("graphify binary should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("# Stub Suggestions"));
    assert!(
        stdout.contains("`tokio`"),
        "expected tokio in cross-project section: {}",
        stdout
    );
    assert!(stdout.contains("`serde`"));
    assert!(stdout.contains("### proj-a"));
    assert!(stdout.contains("`rmcp`"));
    assert!(stdout.contains("### proj-b"));
    assert!(stdout.contains("`clap`"));
    assert!(
        stdout.contains("`std`"),
        "std should appear in Already covered: {}",
        stdout
    );
}

#[test]
fn suggest_stubs_json_output_is_well_formed() {
    let dir = fixture_dir();
    let output = Command::new(graphify_bin())
        .current_dir(&dir)
        .args([
            "suggest",
            "stubs",
            "--config",
            "graphify.toml",
            "--format",
            "json",
        ])
        .output()
        .expect("graphify binary should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(parsed["min_edges"], 2);
    assert!(parsed["settings_candidates"].is_array());
}

#[test]
fn suggest_stubs_apply_mutates_config_and_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let dst_cfg = tmp.path().join("graphify.toml");
    let dst_proj_a = tmp.path().join("proj-a");
    let dst_proj_b = tmp.path().join("proj-b");
    std::fs::create_dir(&dst_proj_a).unwrap();
    std::fs::create_dir(&dst_proj_b).unwrap();
    std::fs::copy(fixture_dir().join("graphify.toml"), &dst_cfg).unwrap();
    std::fs::copy(
        fixture_dir().join("proj-a/graph.json"),
        dst_proj_a.join("graph.json"),
    )
    .unwrap();
    std::fs::copy(
        fixture_dir().join("proj-b/graph.json"),
        dst_proj_b.join("graph.json"),
    )
    .unwrap();

    // First apply.
    let output = Command::new(graphify_bin())
        .current_dir(tmp.path())
        .args(["suggest", "stubs", "--config", "graphify.toml", "--apply"])
        .output()
        .expect("graphify should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = std::fs::read_to_string(&dst_cfg).unwrap();
    assert!(
        after.contains("\"tokio\""),
        "expected tokio in [settings]: {}",
        after
    );
    assert!(after.contains("\"serde\""));
    assert!(after.contains("\"rmcp\""));
    assert!(after.contains("\"clap\""));

    // Second apply — should be a no-op.
    let output2 = Command::new(graphify_bin())
        .current_dir(tmp.path())
        .args(["suggest", "stubs", "--config", "graphify.toml", "--apply"])
        .output()
        .expect("graphify should run");
    assert!(output2.status.success());
    let stdout2 = String::from_utf8(output2.stdout).unwrap();
    assert!(
        stdout2.contains("(no changes"),
        "second apply should be no-op: {}",
        stdout2
    );

    let after2 = std::fs::read_to_string(&dst_cfg).unwrap();
    assert_eq!(after, after2, "second apply must not change the file");
}

#[test]
fn suggest_stubs_format_and_apply_are_mutually_exclusive() {
    let dir = fixture_dir();
    let output = Command::new(graphify_bin())
        .current_dir(&dir)
        .args([
            "suggest",
            "stubs",
            "--config",
            "graphify.toml",
            "--format",
            "json",
            "--apply",
        ])
        .output()
        .expect("graphify should run");
    assert!(
        !output.status.success(),
        "clap should reject the combination"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("conflict") || stderr.contains("cannot be used"),
        "stderr: {}",
        stderr
    );
}
