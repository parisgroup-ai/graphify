use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tempfile::TempDir;

// Helper: exercise the library surface without shelling out.
// The install module is pub; we reach into it via the binary crate's src/.
// For bin crates, Cargo generates a test harness; we can run the binary via assert_cmd if needed.
// Here we invoke the binary via Command for a true end-to-end test.

fn graphify_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // test exe dir
    if p.ends_with("deps") { p.pop(); }
    p.join("graphify")
}

#[test]
fn install_to_empty_home_creates_all_artifacts() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();
    fs::create_dir_all(home.path().join(".agents/skills")).unwrap();

    let status = std::process::Command::new(graphify_bin())
        .args(["install-integrations", "--claude-code", "--codex", "--skip-mcp"])
        .env("HOME", home.path())
        .current_dir(project.path())
        .status()
        .expect("run graphify");
    assert!(status.success());

    // Agents
    assert!(home.path().join(".claude/agents/graphify-analyst.md").exists());
    assert!(home.path().join(".claude/agents/graphify-ci-guardian.md").exists());
    // Skills
    assert!(home.path().join(".claude/skills/graphify-onboarding/SKILL.md").exists());
    assert!(home.path().join(".claude/skills/graphify-refactor-plan/SKILL.md").exists());
    assert!(home.path().join(".claude/skills/graphify-drift-check/SKILL.md").exists());
    // Commands
    for cmd in &["gf-analyze", "gf-onboard", "gf-refactor-plan", "gf-drift-check"] {
        assert!(home.path().join(".claude/commands").join(format!("{}.md", cmd)).exists());
    }
    // Manifest
    assert!(home.path().join(".claude/.graphify-install.json").exists());
    // Codex skills wrappers (inline fallback since no bridge script present)
    assert!(home
        .path()
        .join(".agents/skills/claude-agent-graphify-analyst/SKILL.md")
        .exists());
}

#[test]
fn second_install_is_noop() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();

    let mut run = || {
        std::process::Command::new(graphify_bin())
            .args(["install-integrations", "--claude-code", "--skip-mcp"])
            .env("HOME", home.path())
            .current_dir(project.path())
            .status()
            .unwrap()
    };
    assert!(run().success());
    let mtime_before = fs::metadata(home.path().join(".claude/agents/graphify-analyst.md"))
        .unwrap()
        .modified()
        .unwrap();
    // small delay to make mtime comparable on coarse filesystems
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(run().success());
    let mtime_after = fs::metadata(home.path().join(".claude/agents/graphify-analyst.md"))
        .unwrap()
        .modified()
        .unwrap();
    // Identical content should short-circuit write (mtime unchanged)
    assert_eq!(mtime_before, mtime_after);
}

#[test]
fn dry_run_writes_nothing() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();

    let status = std::process::Command::new(graphify_bin())
        .args(["install-integrations", "--claude-code", "--skip-mcp", "--dry-run"])
        .env("HOME", home.path())
        .current_dir(project.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!home.path().join(".claude/agents/graphify-analyst.md").exists());
    assert!(!home.path().join(".claude/.graphify-install.json").exists());
}

#[test]
fn uninstall_removes_only_installed_files() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude/agents")).unwrap();
    // User-authored file: must survive uninstall
    let user_file = home.path().join(".claude/agents/user-custom.md");
    fs::write(&user_file, "my stuff").unwrap();

    let run = |args: &[&str]| -> bool {
        std::process::Command::new(graphify_bin())
            .args(args)
            .env("HOME", home.path())
            .current_dir(project.path())
            .status()
            .unwrap()
            .success()
    };
    assert!(run(&["install-integrations", "--claude-code", "--skip-mcp"]));
    assert!(home.path().join(".claude/agents/graphify-analyst.md").exists());

    assert!(run(&["install-integrations", "--claude-code", "--skip-mcp", "--uninstall"]));
    assert!(!home.path().join(".claude/agents/graphify-analyst.md").exists());
    assert!(user_file.exists(), "user-authored file must not be removed");
}

#[test]
fn force_overwrites_modified_file() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude/agents")).unwrap();
    let analyst = home.path().join(".claude/agents/graphify-analyst.md");
    fs::write(&analyst, "user-modified content").unwrap();

    let run = |args: &[&str]| -> bool {
        std::process::Command::new(graphify_bin())
            .args(args)
            .env("HOME", home.path())
            .current_dir(project.path())
            .status()
            .unwrap()
            .success()
    };
    // Without --force: skipped as conflict
    assert!(run(&["install-integrations", "--claude-code", "--skip-mcp"]));
    let content = fs::read_to_string(&analyst).unwrap();
    assert_eq!(content, "user-modified content");
    // With --force: overwritten
    assert!(run(&["install-integrations", "--claude-code", "--skip-mcp", "--force"]));
    let after = fs::read_to_string(&analyst).unwrap();
    assert_ne!(after, "user-modified content");
    assert!(after.contains("graphify-analyst"));
}

#[test]
fn manifest_lock_is_up_to_date() {
    // Walk the real integrations/ dir (relative to workspace root) and
    // compute content hashes. Compare against integrations/.manifest.lock.json.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap() // crates/
        .parent().unwrap() // workspace root
        .to_path_buf();
    let integrations = workspace_root.join("integrations");
    let lock_path = integrations.join(".manifest.lock.json");

    let mut computed: BTreeMap<String, String> = BTreeMap::new();
    walk_collect(&integrations, &integrations, &mut computed);

    // If env UPDATE_LOCK=1, rewrite the lock file; useful when authors change artifacts.
    if std::env::var("UPDATE_LOCK").ok().as_deref() == Some("1") {
        let pretty = serde_json::to_string_pretty(&computed).unwrap();
        fs::write(&lock_path, pretty).unwrap();
        return;
    }

    let existing: BTreeMap<String, String> = if lock_path.exists() {
        serde_json::from_str(&fs::read_to_string(&lock_path).unwrap()).unwrap()
    } else {
        BTreeMap::new()
    };

    assert_eq!(
        computed, existing,
        "integrations/.manifest.lock.json is out of date; regenerate with UPDATE_LOCK=1 cargo test -p graphify-cli --test install_integrations manifest_lock_is_up_to_date"
    );
}

fn walk_collect(root: &std::path::Path, base: &std::path::Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name == ".manifest.lock.json" { continue; }
        if path.is_dir() {
            walk_collect(&path, base, out);
        } else {
            let bytes = fs::read(&path).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            out.insert(rel, format!("{:x}", hasher.finalize()));
        }
    }
}

#[test]
fn mcp_registration_preserves_existing_config() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();

    // Seed an existing Claude config with unrelated entries
    let existing = r#"{
  "theme": "dark",
  "mcpServers": {
    "notion": { "command": "/opt/notion-mcp" }
  }
}"#;
    fs::write(home.path().join(".claude.json"), existing).unwrap();

    let status = std::process::Command::new(graphify_bin())
        .args(["install-integrations", "--claude-code"])
        .env("HOME", home.path())
        .current_dir(project.path())
        .status()
        .unwrap();
    assert!(status.success());

    let merged = fs::read_to_string(home.path().join(".claude.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
    assert_eq!(v["theme"], "dark");
    assert_eq!(v["mcpServers"]["notion"]["command"], "/opt/notion-mcp");
    assert_eq!(v["mcpServers"]["graphify"]["_graphify_managed"], true);
}

#[test]
fn uninstall_removes_mcp_entry_preserves_others() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();
    let existing = r#"{
  "theme": "dark",
  "mcpServers": {
    "notion": { "command": "/opt/notion-mcp" }
  }
}"#;
    fs::write(home.path().join(".claude.json"), existing).unwrap();

    let run = |args: &[&str]| -> bool {
        std::process::Command::new(graphify_bin())
            .args(args)
            .env("HOME", home.path())
            .current_dir(project.path())
            .status()
            .unwrap()
            .success()
    };
    assert!(run(&["install-integrations", "--claude-code"]));
    assert!(run(&["install-integrations", "--claude-code", "--uninstall"]));

    let after = fs::read_to_string(home.path().join(".claude.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert_eq!(v["mcpServers"]["notion"]["command"], "/opt/notion-mcp");
    assert!(v["mcpServers"].get("graphify").is_none());
}
