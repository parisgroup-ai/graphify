//! Integration tests for `graphify consolidation`.
//!
//! Each test spins up a tiny fixture pair under a `TempDir` rather than
//! reusing the shared `tests/fixtures/` trees — keeps the cross-project
//! grouping assertions independent of whatever lives there today.

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
            assert!(status.success(), "cargo build failed");
            workspace_root.join("target/debug/graphify")
        })
        .clone()
}

/// Scaffolds two tiny Python "projects" that both define a `TokenUsage` class
/// and a project-local `Other` symbol. The shared leaf drives consolidation
/// candidate groups in both the per-project and aggregate outputs.
fn write_fixture(tmp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let root = tmp.path();
    let out_dir = root.join("report");

    // Project A
    let proj_a = root.join("apps/a");
    std::fs::create_dir_all(proj_a.join("app/models")).unwrap();
    std::fs::write(proj_a.join("app/__init__.py"), "").unwrap();
    std::fs::write(proj_a.join("app/models/__init__.py"), "").unwrap();
    std::fs::write(
        proj_a.join("app/models/tokens.py"),
        "class TokenUsage:\n    pass\n",
    )
    .unwrap();
    std::fs::write(
        proj_a.join("app/main.py"),
        "from app.models.tokens import TokenUsage\n\nclass Other:\n    pass\n",
    )
    .unwrap();

    // Project B
    let proj_b = root.join("apps/b");
    std::fs::create_dir_all(proj_b.join("app/models")).unwrap();
    std::fs::write(proj_b.join("app/__init__.py"), "").unwrap();
    std::fs::write(proj_b.join("app/models/__init__.py"), "").unwrap();
    std::fs::write(
        proj_b.join("app/models/tokens.py"),
        "class TokenUsage:\n    pass\n",
    )
    .unwrap();
    std::fs::write(
        proj_b.join("app/main.py"),
        "from app.models.tokens import TokenUsage\n\nclass Other:\n    pass\n",
    )
    .unwrap();

    (out_dir, proj_a, proj_b)
}

fn write_config(
    tmp: &TempDir,
    out_dir: &std::path::Path,
    proj_a: &std::path::Path,
    proj_b: &std::path::Path,
    extras: &str,
) -> PathBuf {
    let contents = format!(
        r#"[settings]
output = "{out}"

[[project]]
name = "svc-a"
repo = "{a}"
lang = ["python"]
local_prefix = "app"

[[project]]
name = "svc-b"
repo = "{b}"
lang = ["python"]
local_prefix = "app"

{extras}
"#,
        out = out_dir.to_str().unwrap().replace('\\', "/"),
        a = proj_a.to_str().unwrap().replace('\\', "/"),
        b = proj_b.to_str().unwrap().replace('\\', "/"),
        extras = extras,
    );
    let path = tmp.path().join("graphify.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

fn run(config_path: &std::path::Path, cmd: &str, extra: &[&str]) {
    let status = Command::new(graphify_bin())
        .arg(cmd)
        .arg("--config")
        .arg(config_path)
        .args(extra)
        .status()
        .expect("spawn graphify");
    assert!(status.success(), "graphify {cmd} failed: {status:?}");
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {:?}: {}", path, e));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {:?}: {}", path, e))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn emits_per_project_and_aggregate_reports() {
    let tmp = TempDir::new().unwrap();
    let (out_dir, a, b) = write_fixture(&tmp);
    let cfg = write_config(&tmp, &out_dir, &a, &b, "");

    run(&cfg, "run", &[]);
    run(&cfg, "consolidation", &[]);

    let a_path = out_dir.join("svc-a/consolidation-candidates.json");
    let b_path = out_dir.join("svc-b/consolidation-candidates.json");
    let agg_path = out_dir.join("consolidation-candidates.json");

    assert!(a_path.exists(), "per-project report A missing");
    assert!(b_path.exists(), "per-project report B missing");
    assert!(agg_path.exists(), "aggregate report missing");

    let agg = read_json(&agg_path);
    assert_eq!(agg["schema_version"], 1);
    let candidates = agg["candidates"].as_array().unwrap();
    let token_group = candidates
        .iter()
        .find(|c| c["leaf_name"] == "TokenUsage")
        .expect("TokenUsage group missing from aggregate");
    assert_eq!(token_group["project_count"], 2);
    assert!(token_group["group_size"].as_u64().unwrap() >= 2);
    // Every member must carry alternative_paths (even if empty).
    for m in token_group["members"].as_array().unwrap() {
        assert!(m.get("alternative_paths").is_some());
    }
}

#[test]
fn allowlist_hit_removes_group_by_default() {
    let tmp = TempDir::new().unwrap();
    let (out_dir, a, b) = write_fixture(&tmp);
    let extras = r#"
[consolidation]
allowlist = ["TokenUsage"]
"#;
    let cfg = write_config(&tmp, &out_dir, &a, &b, extras);

    run(&cfg, "run", &[]);
    run(&cfg, "consolidation", &[]);

    let agg = read_json(&out_dir.join("consolidation-candidates.json"));
    assert_eq!(agg["schema_version"], 1);
    let has_token = agg["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["leaf_name"] == "TokenUsage");
    assert!(!has_token, "TokenUsage should be filtered by allowlist");
    assert!(agg["allowlist_applied"].as_u64().unwrap() >= 1);
}

#[test]
fn ignore_allowlist_bypass_retains_group() {
    let tmp = TempDir::new().unwrap();
    let (out_dir, a, b) = write_fixture(&tmp);
    let extras = r#"
[consolidation]
allowlist = ["TokenUsage"]
"#;
    let cfg = write_config(&tmp, &out_dir, &a, &b, extras);

    run(&cfg, "run", &[]);
    run(&cfg, "consolidation", &["--ignore-allowlist"]);

    let agg = read_json(&out_dir.join("consolidation-candidates.json"));
    let token = agg["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["leaf_name"] == "TokenUsage")
        .expect("TokenUsage should reappear under --ignore-allowlist");
    assert_eq!(token["allowlisted"], true);
}

#[test]
fn min_group_size_filter_drops_small_groups() {
    let tmp = TempDir::new().unwrap();
    let (out_dir, a, b) = write_fixture(&tmp);
    let cfg = write_config(&tmp, &out_dir, &a, &b, "");

    run(&cfg, "run", &[]);
    // TokenUsage group in the aggregate has 2 project members + 2 uses across
    // the import paths (definition + consumer) = should survive size=2 but be
    // filtered at size=5.
    run(&cfg, "consolidation", &["--min-group-size", "99"]);

    let agg = read_json(&out_dir.join("consolidation-candidates.json"));
    assert!(agg["candidates"].as_array().unwrap().is_empty());
}

#[test]
fn markdown_format_writes_md_file() {
    let tmp = TempDir::new().unwrap();
    let (out_dir, a, b) = write_fixture(&tmp);
    let cfg = write_config(&tmp, &out_dir, &a, &b, "");

    run(&cfg, "run", &[]);
    run(&cfg, "consolidation", &["--format", "md"]);

    let a_md = out_dir.join("svc-a/consolidation-candidates.md");
    let agg_md = out_dir.join("consolidation-candidates.md");
    assert!(a_md.exists());
    assert!(agg_md.exists());
    let text = std::fs::read_to_string(&a_md).unwrap();
    assert!(text.contains("# Consolidation Candidates"));
}

#[test]
fn single_project_config_skips_aggregate() {
    let tmp = TempDir::new().unwrap();
    let (out_dir, a, _b) = write_fixture(&tmp);

    // Hand-roll a single-project config.
    let contents = format!(
        r#"[settings]
output = "{out}"

[[project]]
name = "svc-a"
repo = "{a}"
lang = ["python"]
local_prefix = "app"
"#,
        out = out_dir.to_str().unwrap().replace('\\', "/"),
        a = a.to_str().unwrap().replace('\\', "/"),
    );
    let cfg = tmp.path().join("graphify.toml");
    std::fs::write(&cfg, contents).unwrap();

    run(&cfg, "run", &[]);
    run(&cfg, "consolidation", &[]);

    let proj = out_dir.join("svc-a/consolidation-candidates.json");
    let agg = out_dir.join("consolidation-candidates.json");
    assert!(proj.exists());
    assert!(
        !agg.exists(),
        "aggregate should not be emitted for 1 project"
    );
}
