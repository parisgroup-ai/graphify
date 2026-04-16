use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

fn graphify_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("graphify")
}

#[test]
fn init_prints_next_steps_for_integrations_and_run() {
    let project = TempDir::new().unwrap();

    let output = std::process::Command::new(graphify_bin())
        .arg("init")
        .current_dir(project.path())
        .output()
        .expect("run graphify init");

    assert!(output.status.success(), "init should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("Created graphify.toml"),
        "stdout should mention the created config file: {stdout}"
    );
    assert!(
        stdout.contains("graphify install-integrations --project-local"),
        "stdout should mention project-local integrations install: {stdout}"
    );
    assert!(
        stdout.contains("graphify run"),
        "stdout should mention the next analysis step: {stdout}"
    );

    let graphify_toml = project.path().join("graphify.toml");
    assert!(graphify_toml.exists(), "init should create graphify.toml");
}

#[test]
fn init_help_mentions_integrations_are_separate() {
    let output = std::process::Command::new(graphify_bin())
        .args(["init", "--help"])
        .output()
        .expect("run graphify init --help");

    assert!(output.status.success(), "init --help should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("This command only creates graphify.toml"),
        "help should explain init scope: {stdout}"
    );
    assert!(
        stdout.contains("graphify install-integrations"),
        "help should point to integrations install: {stdout}"
    );
}

#[test]
fn init_does_not_overwrite_existing_config() {
    let project = TempDir::new().unwrap();
    let graphify_toml = project.path().join("graphify.toml");
    fs::write(&graphify_toml, "existing = true\n").unwrap();

    let output = std::process::Command::new(graphify_bin())
        .arg("init")
        .current_dir(project.path())
        .output()
        .expect("run graphify init");

    assert!(
        !output.status.success(),
        "init should fail when graphify.toml already exists"
    );

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("graphify.toml already exists"),
        "stderr should explain why init failed: {stderr}"
    );

    let preserved = fs::read_to_string(&graphify_toml).unwrap();
    assert_eq!(preserved, "existing = true\n");
}
