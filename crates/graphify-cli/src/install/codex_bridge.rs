use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::install::copy_plan::INTEGRATIONS;

/// Returns the path to the user's claude-agent-bridge sync script, if present.
pub fn find_bridge_script(home: &Path) -> Option<PathBuf> {
    let p = home.join(".codex/claude-agent-bridge/sync.sh");
    p.exists().then_some(p)
}

/// Runs the bridge script. Returns Err if invocation fails (not if script itself returns non-zero).
pub fn run_bridge(script: &Path, install_root: &Path) -> std::io::Result<()> {
    let status = Command::new("bash")
        .arg(script)
        .arg("--install-root")
        .arg(install_root)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "bridge script exited with {status}"
        )));
    }
    Ok(())
}

/// Fallback: writes `~/.agents/skills/claude-agent-<name>/SKILL.md` wrappers inline.
/// Each wrapper is a thin skill that delegates to the Claude agent body.
pub fn write_inline_wrappers(install_root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let agents_dir = INTEGRATIONS
        .get_dir("claude-code/agents")
        .expect("embedded agents dir");
    let mut written = Vec::new();

    for file in agents_dir.files() {
        let filename = file.path().file_name().unwrap().to_string_lossy();
        let agent_name = filename.trim_end_matches(".md");
        let wrapper_dir = install_root
            .join("skills")
            .join(format!("claude-agent-{}", agent_name));
        fs::create_dir_all(&wrapper_dir)?;
        let wrapper_path = wrapper_dir.join("SKILL.md");
        let body = String::from_utf8_lossy(file.contents()).to_string();
        let wrapper_content = format!(
            "---\nname: claude-agent-{}\ndescription: Codex bridge wrapper for the {} Claude agent.\nversion: 1.0.0\n---\n\n{}\n",
            agent_name, agent_name, strip_frontmatter(&body)
        );
        fs::write(&wrapper_path, wrapper_content)?;
        written.push(wrapper_path);
    }
    Ok(written)
}

fn strip_frontmatter(content: &str) -> String {
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim()) != Some("---") {
        return content.to_string();
    }
    for line in lines.by_ref() {
        if line.trim() == "---" {
            break;
        }
    }
    lines.collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn find_bridge_returns_none_when_absent() {
        let tmp = TempDir::new().unwrap();
        assert!(find_bridge_script(tmp.path()).is_none());
    }

    #[test]
    fn find_bridge_returns_path_when_present() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".codex/claude-agent-bridge");
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("sync.sh");
        fs::write(&script, "#!/bin/bash\necho hi\n").unwrap();
        assert_eq!(find_bridge_script(tmp.path()), Some(script));
    }

    #[test]
    fn inline_wrappers_created_per_agent() {
        let tmp = TempDir::new().unwrap();
        let written = write_inline_wrappers(tmp.path()).unwrap();
        assert!(written.len() >= 2); // analyst + ci-guardian
        let analyst_wrapper = tmp
            .path()
            .join("skills/claude-agent-graphify-analyst/SKILL.md");
        assert!(analyst_wrapper.exists());
        let content = fs::read_to_string(&analyst_wrapper).unwrap();
        assert!(content.starts_with("---"));
        assert!(content.contains("name: claude-agent-graphify-analyst"));
    }

    #[test]
    fn strip_frontmatter_removes_yaml_block() {
        let input = "---\nname: x\n---\nbody\n";
        assert_eq!(strip_frontmatter(input).trim(), "body");
    }
}
