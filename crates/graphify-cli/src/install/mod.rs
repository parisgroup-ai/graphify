//! Install integrations subcommand: copies agent/skill/command artifacts to
//! the user's AI-client directories and registers the graphify-mcp server.

pub mod manifest;
pub mod frontmatter;
pub mod mcp_merge;
pub mod copy_plan;
pub mod codex_bridge;

use std::fs;
use std::path::{Path, PathBuf};

use crate::install::copy_plan::{build_plan, execute, INTEGRATIONS};
use crate::install::manifest::{Manifest, McpEntry, McpRecord};

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub claude_code: bool,
    pub codex: bool,
    pub project_local: bool,
    pub skip_mcp: bool,
    pub dry_run: bool,
    pub force: bool,
    pub home: PathBuf,
    pub project_root: PathBuf,
    pub graphify_version: String,
    pub graphify_mcp_binary: PathBuf,
}

#[derive(Debug)]
pub struct InstallReport {
    pub manifest: Manifest,
    pub conflicts: Vec<PathBuf>,
    pub skipped_identical: Vec<PathBuf>,
    pub mcp_changes: Vec<PathBuf>,
}

pub fn claude_install_root(opts: &InstallOptions) -> PathBuf {
    if opts.project_local {
        opts.project_root.join(".claude")
    } else {
        opts.home.join(".claude")
    }
}

pub fn codex_install_root(opts: &InstallOptions) -> PathBuf {
    // Codex ignores --project-local per spec §2.4
    opts.home.join(".agents")
}

/// Produces the list of (embed-key, dest-path, kind) triples to copy for Claude Code.
pub fn claude_pairs(opts: &InstallOptions) -> Vec<(String, PathBuf, String)> {
    let root = claude_install_root(opts);
    let mut pairs = Vec::new();

    // agents
    for file in INTEGRATIONS.get_dir("claude-code/agents").unwrap().files() {
        let name = file.path().file_name().unwrap().to_string_lossy().to_string();
        pairs.push((
            format!("claude-code/agents/{}", name),
            root.join("agents").join(&name),
            "agent".into(),
        ));
    }
    // skills
    for subdir in INTEGRATIONS
        .get_dir("claude-code/skills")
        .unwrap()
        .dirs()
    {
        let skill_name = subdir.path().file_name().unwrap().to_string_lossy().to_string();
        for file in subdir.files() {
            let fname = file.path().file_name().unwrap().to_string_lossy().to_string();
            pairs.push((
                format!("claude-code/skills/{}/{}", skill_name, fname),
                root.join("skills").join(&skill_name).join(&fname),
                "skill".into(),
            ));
        }
    }
    // commands
    for file in INTEGRATIONS
        .get_dir("claude-code/commands")
        .unwrap()
        .files()
    {
        let name = file.path().file_name().unwrap().to_string_lossy().to_string();
        pairs.push((
            format!("claude-code/commands/{}", name),
            root.join("commands").join(&name),
            "command".into(),
        ));
    }
    pairs
}

/// Returns (embed-key, dest-path, kind) for Codex skills (pre-bridge; bridge/inline is separate).
pub fn codex_skill_pairs(opts: &InstallOptions) -> Vec<(String, PathBuf, String)> {
    let root = codex_install_root(opts);
    let mut pairs = Vec::new();
    // Skills and commands are copied to ~/.agents/skills/ for Codex consumption.
    // Skills follow the Claude Code layout; Codex reads them as-is.
    for subdir in INTEGRATIONS
        .get_dir("claude-code/skills")
        .unwrap()
        .dirs()
    {
        let skill_name = subdir.path().file_name().unwrap().to_string_lossy().to_string();
        for file in subdir.files() {
            let fname = file.path().file_name().unwrap().to_string_lossy().to_string();
            pairs.push((
                format!("claude-code/skills/{}/{}", skill_name, fname),
                root.join("skills").join(&skill_name).join(&fname),
                "skill".into(),
            ));
        }
    }
    // Codex prompts
    for file in INTEGRATIONS.get_dir("codex/prompts").unwrap().files() {
        let name = file.path().file_name().unwrap().to_string_lossy().to_string();
        pairs.push((
            format!("codex/prompts/{}", name),
            opts.home.join(".codex/prompts").join(&name),
            "command".into(),
        ));
    }
    pairs
}

fn merge_mcp_for_claude(opts: &InstallOptions) -> std::io::Result<Option<McpEntry>> {
    if opts.skip_mcp { return Ok(None); }
    let dest = if opts.project_local {
        opts.project_root.join(".mcp.json")
    } else {
        opts.home.join(".claude.json")
    };
    let existing = fs::read_to_string(&dest).unwrap_or_default();
    let merged = crate::install::mcp_merge::merge_claude_config(
        &existing,
        &opts.graphify_mcp_binary.display().to_string(),
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if !opts.dry_run {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &merged)?;
    }
    Ok(Some(McpEntry { path: dest, key: "graphify".into() }))
}

fn merge_mcp_for_codex(opts: &InstallOptions) -> std::io::Result<Option<McpEntry>> {
    if opts.skip_mcp { return Ok(None); }
    let dest = opts.home.join(".codex/config.toml");
    let existing = fs::read_to_string(&dest).unwrap_or_default();
    let merged = crate::install::mcp_merge::merge_codex_config(
        &existing,
        &opts.graphify_mcp_binary.display().to_string(),
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if !opts.dry_run {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &merged)?;
    }
    Ok(Some(McpEntry { path: dest, key: "graphify".into() }))
}

pub fn run_install(opts: &InstallOptions) -> std::io::Result<InstallReport> {
    let mut all_manifest_files = Vec::new();
    let mut conflicts = Vec::new();
    let mut skipped_identical = Vec::new();
    let mut mcp = McpRecord { claude_code: None, codex: None };
    let mut mcp_changes = Vec::new();

    if opts.claude_code {
        let pairs = claude_pairs(opts);
        let plan = build_plan(&pairs, opts.force);
        for act in &plan.actions {
            match act {
                copy_plan::CopyAction::Skip { dest, reason: copy_plan::SkipReason::Conflict } => {
                    conflicts.push(dest.clone());
                }
                copy_plan::CopyAction::Skip { dest, reason: copy_plan::SkipReason::Identical } => {
                    skipped_identical.push(dest.clone());
                }
                _ => {}
            }
        }
        let written = execute(&plan, opts.dry_run)?;
        all_manifest_files.extend(written);

        if let Some(entry) = merge_mcp_for_claude(opts)? {
            mcp_changes.push(entry.path.clone());
            mcp.claude_code = Some(entry);
        }
    }

    if opts.codex {
        let pairs = codex_skill_pairs(opts);
        let plan = build_plan(&pairs, opts.force);
        for act in &plan.actions {
            match act {
                copy_plan::CopyAction::Skip { dest, reason: copy_plan::SkipReason::Conflict } => {
                    conflicts.push(dest.clone());
                }
                copy_plan::CopyAction::Skip { dest, reason: copy_plan::SkipReason::Identical } => {
                    skipped_identical.push(dest.clone());
                }
                _ => {}
            }
        }
        let written = execute(&plan, opts.dry_run)?;
        all_manifest_files.extend(written);

        // Bridge or inline fallback for agents → Codex skills
        let bridge = codex_bridge::find_bridge_script(&opts.home);
        if !opts.dry_run {
            match bridge {
                Some(script) => codex_bridge::run_bridge(&script, &codex_install_root(opts))?,
                None => {
                    codex_bridge::write_inline_wrappers(&codex_install_root(opts))?;
                }
            }
        }

        if let Some(entry) = merge_mcp_for_codex(opts)? {
            mcp_changes.push(entry.path.clone());
            mcp.codex = Some(entry);
        }
    }

    let manifest = Manifest {
        graphify_version: opts.graphify_version.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        files: all_manifest_files,
        mcp: if mcp.claude_code.is_some() || mcp.codex.is_some() { Some(mcp) } else { None },
    };

    if !opts.dry_run {
        let install_root = if opts.claude_code {
            claude_install_root(opts)
        } else {
            codex_install_root(opts)
        };
        fs::create_dir_all(&install_root)?;
        let manifest_path = install_root.join(".graphify-install.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap())?;
    }

    Ok(InstallReport { manifest, conflicts, skipped_identical, mcp_changes })
}

pub fn run_uninstall(opts: &InstallOptions) -> std::io::Result<()> {
    for install_root in [claude_install_root(opts), codex_install_root(opts)] {
        let manifest_path = install_root.join(".graphify-install.json");
        if !manifest_path.exists() { continue; }
        let manifest: Manifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        for f in &manifest.files {
            if !f.path.exists() { continue; }
            let current = fs::read(&f.path)?;
            let current_sha = manifest::sha256_of_bytes(&current);
            if current_sha == f.sha256 {
                fs::remove_file(&f.path)?;
            } else {
                eprintln!(
                    "graphify uninstall: {} was modified, skipping (edit sha: {}, expected: {})",
                    f.path.display(),
                    current_sha,
                    f.sha256
                );
            }
        }
        if let Some(mcp) = &manifest.mcp {
            if let Some(entry) = &mcp.claude_code { remove_mcp_entry_json(&entry.path)?; }
            if let Some(entry) = &mcp.codex       { remove_mcp_entry_toml(&entry.path)?; }
        }
        fs::remove_file(&manifest_path)?;
    }
    Ok(())
}

fn remove_mcp_entry_json(path: &Path) -> std::io::Result<()> {
    if !path.exists() { return Ok(()); }
    let content = fs::read_to_string(path)?;
    let mut v: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(servers) = v.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
        servers.remove("graphify");
    }
    fs::write(path, serde_json::to_string_pretty(&v).unwrap())?;
    Ok(())
}

fn remove_mcp_entry_toml(path: &Path) -> std::io::Result<()> {
    if !path.exists() { return Ok(()); }
    let content = fs::read_to_string(path)?;
    let mut v: toml::Value = content.parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if let Some(servers) = v.get_mut("mcp_servers").and_then(|s| s.as_table_mut()) {
        servers.remove("graphify");
    }
    fs::write(path, toml::to_string_pretty(&v).unwrap())?;
    Ok(())
}
