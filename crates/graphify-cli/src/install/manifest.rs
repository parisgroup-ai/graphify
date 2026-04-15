use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub graphify_version: String,
    pub installed_at: String,
    pub files: Vec<ManifestFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFile {
    pub path: PathBuf,
    pub sha256: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<McpEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex: Option<McpEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpEntry {
    pub path: PathBuf,
    pub key: String,
}

pub fn sha256_of_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_through_json() {
        let original = Manifest {
            graphify_version: "0.7.0".into(),
            installed_at: "2026-04-15T10:20:30Z".into(),
            files: vec![ManifestFile {
                path: PathBuf::from("/home/u/.claude/agents/graphify-analyst.md"),
                sha256: "abc123".into(),
                kind: "agent".into(),
            }],
            mcp: Some(McpRecord {
                claude_code: Some(McpEntry {
                    path: PathBuf::from("/home/u/.claude.json"),
                    key: "graphify".into(),
                }),
                codex: None,
            }),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn sha256_is_deterministic() {
        let a = sha256_of_bytes(b"hello");
        let b = sha256_of_bytes(b"hello");
        assert_eq!(a, b);
        assert_eq!(
            a,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
