use std::fs;
use std::path::PathBuf;

use include_dir::{include_dir, Dir};

use crate::install::manifest::{sha256_of_bytes, ManifestFile};

pub static INTEGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../integrations");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyAction {
    Write {
        dest: PathBuf,
        bytes: Vec<u8>,
        kind: String,
    },
    Skip {
        dest: PathBuf,
        reason: SkipReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Identical,
    Conflict, // different content at destination; --force required
}

#[derive(Debug, Default)]
pub struct PlanResult {
    pub actions: Vec<CopyAction>,
}

/// Builds a copy plan for a set of (source_path_in_embed, dest_path) pairs.
/// `force` suppresses conflict checks and always writes.
pub fn build_plan(pairs: &[(String, PathBuf, String)], force: bool) -> PlanResult {
    let mut actions = Vec::new();
    for (src_key, dest, kind) in pairs {
        let Some(file) = INTEGRATIONS.get_file(src_key.as_str()) else {
            continue; // caller is responsible for listing valid keys
        };
        let bytes = file.contents().to_vec();
        if dest.exists() && !force {
            let existing = fs::read(dest).unwrap_or_default();
            if sha256_of_bytes(&existing) == sha256_of_bytes(&bytes) {
                actions.push(CopyAction::Skip {
                    dest: dest.clone(),
                    reason: SkipReason::Identical,
                });
            } else {
                actions.push(CopyAction::Skip {
                    dest: dest.clone(),
                    reason: SkipReason::Conflict,
                });
            }
        } else {
            actions.push(CopyAction::Write {
                dest: dest.clone(),
                bytes,
                kind: kind.clone(),
            });
        }
    }
    PlanResult { actions }
}

/// Executes the plan. Returns the list of written `ManifestFile`s.
pub fn execute(plan: &PlanResult, dry_run: bool) -> std::io::Result<Vec<ManifestFile>> {
    let mut manifest_files = Vec::new();
    for action in &plan.actions {
        match action {
            CopyAction::Write { dest, bytes, kind } => {
                if !dry_run {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(dest, bytes)?;
                }
                manifest_files.push(ManifestFile {
                    path: dest.clone(),
                    sha256: sha256_of_bytes(bytes),
                    kind: kind.clone(),
                });
            }
            CopyAction::Skip { .. } => {}
        }
    }
    Ok(manifest_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn plan_writes_when_dest_absent() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("agents/graphify-analyst.md");
        let pairs = vec![(
            "claude-code/agents/graphify-analyst.md".into(),
            dest.clone(),
            "agent".into(),
        )];
        let plan = build_plan(&pairs, false);
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0] {
            CopyAction::Write { .. } => {}
            other => panic!("expected Write, got {:?}", other),
        }
    }

    #[test]
    fn plan_skips_when_dest_identical() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("agents/graphify-analyst.md");
        let src = INTEGRATIONS
            .get_file("claude-code/agents/graphify-analyst.md")
            .expect("embedded agent file must exist");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, src.contents()).unwrap();

        let pairs = vec![(
            "claude-code/agents/graphify-analyst.md".into(),
            dest.clone(),
            "agent".into(),
        )];
        let plan = build_plan(&pairs, false);
        assert_eq!(plan.actions.len(), 1);
        match &plan.actions[0] {
            CopyAction::Skip {
                reason: SkipReason::Identical,
                ..
            } => {}
            other => panic!("expected Skip(Identical), got {:?}", other),
        }
    }

    #[test]
    fn plan_reports_conflict_when_content_differs() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("agents/graphify-analyst.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"different content").unwrap();

        let pairs = vec![(
            "claude-code/agents/graphify-analyst.md".into(),
            dest.clone(),
            "agent".into(),
        )];
        let plan = build_plan(&pairs, false);
        match &plan.actions[0] {
            CopyAction::Skip {
                reason: SkipReason::Conflict,
                ..
            } => {}
            other => panic!("expected Skip(Conflict), got {:?}", other),
        }
    }

    #[test]
    fn force_overrides_conflict() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("agents/graphify-analyst.md");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"different content").unwrap();

        let pairs = vec![(
            "claude-code/agents/graphify-analyst.md".into(),
            dest.clone(),
            "agent".into(),
        )];
        let plan = build_plan(&pairs, true);
        assert!(matches!(plan.actions[0], CopyAction::Write { .. }));
    }

    #[test]
    fn execute_writes_files_and_returns_manifest_entries() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("agents/graphify-analyst.md");
        let pairs = vec![(
            "claude-code/agents/graphify-analyst.md".into(),
            dest.clone(),
            "agent".into(),
        )];
        let plan = build_plan(&pairs, false);
        let manifest = execute(&plan, false).unwrap();
        assert_eq!(manifest.len(), 1);
        assert!(dest.exists());
        assert_eq!(manifest[0].kind, "agent");
    }

    #[test]
    fn dry_run_does_not_write() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("agents/graphify-analyst.md");
        let pairs = vec![(
            "claude-code/agents/graphify-analyst.md".into(),
            dest.clone(),
            "agent".into(),
        )];
        let plan = build_plan(&pairs, false);
        let manifest = execute(&plan, true).unwrap();
        assert_eq!(manifest.len(), 1); // manifest shape reported
        assert!(!dest.exists()); // but nothing written
    }
}
