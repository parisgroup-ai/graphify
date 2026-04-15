use std::path::PathBuf;

use serde_json::{Map, Value};

/// Merges a graphify MCP server entry into an existing Claude Code config.
/// Preserves all other keys and `mcpServers.*` entries untouched.
pub fn merge_claude_config(existing: &str, graphify_binary: &str) -> Result<String, serde_json::Error> {
    let mut root: Value = if existing.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(existing)?
    };
    let obj = root.as_object_mut().expect("root must be object");
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("mcpServers must be object");

    let mut entry = Map::new();
    entry.insert("command".into(), Value::String(graphify_binary.to_string()));
    entry.insert("args".into(), Value::Array(vec![]));
    entry.insert("_graphify_managed".into(), Value::Bool(true));

    servers.insert("graphify".into(), Value::Object(entry));
    Ok(serde_json::to_string_pretty(&root)?)
}

/// Returns true if the existing `graphify` entry (if any) is ours.
pub fn is_self_managed(existing: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(existing) else { return false };
    v.pointer("/mcpServers/graphify/_graphify_managed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct McpTarget {
    pub path: PathBuf,
    pub kind: McpTargetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTargetKind {
    ClaudeCode,
    Codex,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_graphify_to_empty_config() {
        let merged = merge_claude_config("", "/usr/local/bin/graphify-mcp").unwrap();
        assert!(merged.contains("\"graphify\""));
        assert!(merged.contains("\"_graphify_managed\": true"));
    }

    #[test]
    fn preserves_unrelated_top_level_keys() {
        let existing = r#"{ "theme": "dark", "mcpServers": { "other": { "command": "foo" } } }"#;
        let merged = merge_claude_config(existing, "/bin/graphify-mcp").unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcpServers"]["other"]["command"], "foo");
        assert_eq!(v["mcpServers"]["graphify"]["_graphify_managed"], true);
    }

    #[test]
    fn replaces_self_managed_entry() {
        let existing = r#"{ "mcpServers": { "graphify": { "command": "old", "_graphify_managed": true } } }"#;
        let merged = merge_claude_config(existing, "/new/graphify-mcp").unwrap();
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["mcpServers"]["graphify"]["command"], "/new/graphify-mcp");
    }

    #[test]
    fn self_managed_flag_detection() {
        assert!(!is_self_managed(""));
        assert!(!is_self_managed(r#"{ "mcpServers": { "graphify": { "command": "foo" } } }"#));
        assert!(is_self_managed(
            r#"{ "mcpServers": { "graphify": { "command": "foo", "_graphify_managed": true } } }"#
        ));
    }
}
