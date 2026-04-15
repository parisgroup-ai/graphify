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

/// Merges a graphify MCP server entry into an existing Codex config.toml.
/// Preserves other sections untouched.
pub fn merge_codex_config(existing: &str, graphify_binary: &str) -> Result<String, toml::de::Error> {
    let mut root: toml::Value = if existing.trim().is_empty() {
        toml::Value::Table(toml::value::Table::new())
    } else {
        existing.parse()?
    };

    let table = root.as_table_mut().expect("root must be table");
    let servers_entry = table
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
    let servers = servers_entry.as_table_mut().expect("mcp_servers must be a table");

    let mut graphify_table = toml::value::Table::new();
    graphify_table.insert("command".into(), toml::Value::String(graphify_binary.into()));
    graphify_table.insert("args".into(), toml::Value::Array(vec![]));
    graphify_table.insert("_graphify_managed".into(), toml::Value::Boolean(true));

    servers.insert("graphify".into(), toml::Value::Table(graphify_table));

    Ok(toml::to_string_pretty(&root).expect("serialize TOML"))
}

pub fn is_self_managed_codex(existing: &str) -> bool {
    let Ok(v) = existing.parse::<toml::Value>() else { return false };
    v.get("mcp_servers")
        .and_then(|s| s.get("graphify"))
        .and_then(|g| g.get("_graphify_managed"))
        .and_then(|b| b.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod codex_tests {
    use super::*;

    #[test]
    fn adds_graphify_to_empty_toml() {
        let merged = merge_codex_config("", "/bin/graphify-mcp").unwrap();
        assert!(merged.contains("[mcp_servers.graphify]"));
        assert!(merged.contains("_graphify_managed = true"));
    }

    #[test]
    fn preserves_unrelated_sections() {
        let existing = r#"
model = "gpt-5.4"

[features]
rmcp_client = true

[mcp_servers.other]
command = "foo"
"#;
        let merged = merge_codex_config(existing, "/bin/graphify-mcp").unwrap();
        let v: toml::Value = merged.parse().unwrap();
        assert_eq!(v["model"].as_str(), Some("gpt-5.4"));
        assert_eq!(v["features"]["rmcp_client"].as_bool(), Some(true));
        assert_eq!(v["mcp_servers"]["other"]["command"].as_str(), Some("foo"));
        assert_eq!(v["mcp_servers"]["graphify"]["command"].as_str(), Some("/bin/graphify-mcp"));
    }

    #[test]
    fn codex_self_managed_flag_detection() {
        assert!(!is_self_managed_codex(""));
        let managed = r#"[mcp_servers.graphify]
command = "x"
_graphify_managed = true
"#;
        assert!(is_self_managed_codex(managed));
    }
}
