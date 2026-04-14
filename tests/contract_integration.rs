use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn graphify_bin() -> PathBuf {
    static GRAPHIFY_BIN: OnceLock<PathBuf> = OnceLock::new();
    GRAPHIFY_BIN
        .get_or_init(|| {
            let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let status = Command::new("cargo")
                .current_dir(&workspace_root)
                .args(["build", "-q", "-p", "graphify-cli", "--bin", "graphify"])
                .status()
                .expect("build graphify binary for integration tests");
            assert!(status.success(), "cargo build failed");
            workspace_root.join("target/debug/graphify")
        })
        .clone()
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract_drift/monorepo")
}

fn run_check(args: &[&str]) -> std::process::Output {
    Command::new(graphify_bin())
        .current_dir(fixture_dir())
        .arg("check")
        .args(args)
        .output()
        .expect("run graphify check")
}

#[test]
fn drifted_pair_fails_with_expected_violations() {
    let out = run_check(&["--config", "graphify.toml", "--json"]);
    assert!(!out.status.success(), "exit code should be non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"contracts\""), "missing contracts block");
    assert!(
        stdout.contains("contract_unmapped_orm_type"),
        "missing unmapped violation"
    );
    assert!(
        stdout.contains("contract_relation_missing_on_ts")
            || stdout.contains("contract_relation_missing_on_orm"),
        "missing relation violation"
    );
}

#[test]
fn no_contracts_flag_skips_gate() {
    let out = run_check(&["--config", "graphify.toml", "--json", "--no-contracts"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // `contracts` should be omitted entirely.
    assert!(!stdout.contains("\"contracts\""));
}

#[test]
fn warnings_as_errors_escalates_unmapped() {
    let out = run_check(&[
        "--config",
        "graphify.toml",
        "--json",
        "--contracts-warnings-as-errors",
    ]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Every unmapped entry should now be counted as an error.
    assert!(stdout.contains("\"severity\": \"error\""));
    // warning_count for the pair containing tsvector must drop to 0 because it was escalated.
    // A coarse check: find the "warning_count" line with value 0 anywhere in the contracts block.
    assert!(
        stdout.contains("\"warning_count\": 0"),
        "warnings should be escalated to errors"
    );
}

#[test]
fn human_output_prints_contracts_section() {
    let out = run_check(&["--config", "graphify.toml"]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[contracts] FAILED"));
}

#[test]
fn idempotent_json_output() {
    let a = run_check(&["--config", "graphify.toml", "--json"]);
    let b = run_check(&["--config", "graphify.toml", "--json"]);
    assert_eq!(
        a.stdout, b.stdout,
        "JSON output must be deterministic across runs"
    );
}

#[test]
fn missing_orm_file_produces_clear_error() {
    let broken = fixture_dir().join("graphify.broken.toml");
    std::fs::write(
        &broken,
        r#"
[settings]
output = "./report"

[[project]]
name = "db"
repo = "./packages/db"
lang = ["typescript"]

[[contract.pair]]
name = "user"
orm  = { source = "drizzle", file = "packages/db/src/schema/does_not_exist.ts", table = "users" }
ts   = { file   = "packages/api/src/types/user.ts", export = "UserDto" }
"#,
    )
    .unwrap();
    let out = Command::new(graphify_bin())
        .current_dir(fixture_dir())
        .args(["check", "--config", "graphify.broken.toml"])
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot read ORM file"),
        "expected clear error, got: {stderr}"
    );
    std::fs::remove_file(&broken).ok();
}
