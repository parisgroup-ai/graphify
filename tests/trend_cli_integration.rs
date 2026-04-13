use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use tempfile::TempDir;

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

            assert!(status.success(), "cargo build graphify binary failed");
            workspace_root.join("target/debug/graphify")
        })
        .clone()
}

#[test]
fn run_persists_history_and_trend_generates_reports() {
    let fixture_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/python_project");

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
    std::fs::write(&config_path, config_content).expect("write config");

    let first_status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch first graphify run");
    assert!(first_status.success(), "first graphify run failed");

    let second_status = Command::new(graphify_bin())
        .arg("run")
        .arg("--config")
        .arg(&config_path)
        .status()
        .expect("launch second graphify run");
    assert!(second_status.success(), "second graphify run failed");

    let project_out = out_dir.join("python_fixture");
    let history_dir = project_out.join("history");
    assert!(history_dir.exists(), "history directory was not created");

    let history_count = std::fs::read_dir(&history_dir)
        .expect("read history dir")
        .count();
    assert!(
        history_count >= 2,
        "expected at least two history snapshots, got {history_count}"
    );

    let trend_status = Command::new(graphify_bin())
        .arg("trend")
        .arg("--config")
        .arg(&config_path)
        .arg("--project")
        .arg("python_fixture")
        .status()
        .expect("launch graphify trend");
    assert!(trend_status.success(), "graphify trend failed");

    let trend_json = project_out.join("trend-report.json");
    let trend_md = project_out.join("trend-report.md");
    assert!(trend_json.exists(), "missing {}", trend_json.display());
    assert!(trend_md.exists(), "missing {}", trend_md.display());

    let raw = std::fs::read_to_string(&trend_json).expect("read trend-report.json");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse trend-report.json");
    assert_eq!(value["project"], "python_fixture");
    assert!(
        value["snapshot_count"].as_u64().unwrap_or_default() >= 2,
        "trend report should include at least two snapshots"
    );
}
