//! E2E test: `graphify run` against the PHP fixture.
//!
//! Verifies that the full extract → analyze → report pipeline produces an
//! `analysis.json` that references the expected PHP module nodes.

use std::path::PathBuf;
use std::process::Command;

fn fixture_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to crates/graphify-cli; the shared fixtures
    // live two levels up at the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/php_project")
}

#[test]
fn graphify_run_against_php_fixture_produces_expected_nodes() {
    let fixture = fixture_root();
    assert!(
        fixture.join("composer.json").exists(),
        "fixture setup: composer.json must exist at {:?}",
        fixture.join("composer.json")
    );

    let tmp = tempfile::tempdir().expect("tempdir");

    // Write a minimal config that points repo at the fixture with an absolute
    // path so the binary can be invoked from any CWD.
    let config_path = tmp.path().join("graphify.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"[settings]
output = "."
exclude = ["vendor"]
format = ["json"]

[[project]]
name = "php-fixture"
repo = "{}"
lang = ["php"]
local_prefix = ""
"#,
            fixture.to_string_lossy(),
        ),
    )
    .expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "run",
            "--config",
            config_path.to_str().unwrap(),
            "--output",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .expect("spawn graphify");

    assert!(
        output.status.success(),
        "graphify run failed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let analysis_path = tmp.path().join("php-fixture/analysis.json");
    assert!(
        analysis_path.exists(),
        "analysis.json must exist at {:?}",
        analysis_path
    );

    let analysis_text = std::fs::read_to_string(&analysis_path).expect("read analysis.json");

    assert!(
        analysis_text.contains("App.Controllers.HomeController"),
        "analysis.json must reference App.Controllers.HomeController\n  got: {}",
        &analysis_text[..analysis_text.len().min(500)]
    );
    assert!(
        analysis_text.contains("App.Services.Llm"),
        "analysis.json must reference App.Services.Llm"
    );
    assert!(
        analysis_text.contains("App.Models.User"),
        "analysis.json must reference App.Models.User"
    );
}
