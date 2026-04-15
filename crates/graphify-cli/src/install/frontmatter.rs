use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// TODO(feat-018-follow-up): wire min_graphify_version check into run_install
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub min_graphify_version: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum FrontmatterError {
    #[error("missing frontmatter delimiters (expected `---` at top of file)")]
    Missing,
    #[error("unterminated frontmatter block")]
    Unterminated,
    #[error("invalid YAML in frontmatter: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),
}

// TODO(feat-018-follow-up): wire min_graphify_version check into run_install
#[allow(dead_code)]
pub fn parse(content: &str) -> Result<(Frontmatter, String), FrontmatterError> {
    let mut lines = content.lines();
    let first = lines.next().ok_or(FrontmatterError::Missing)?;
    if first.trim() != "---" {
        return Err(FrontmatterError::Missing);
    }

    let mut yaml_buf = String::new();
    let mut found_end = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_end = true;
            break;
        }
        yaml_buf.push_str(line);
        yaml_buf.push('\n');
    }
    if !found_end {
        return Err(FrontmatterError::Unterminated);
    }

    let fm: Frontmatter = serde_yaml::from_str(&yaml_buf)?;
    let body = lines.collect::<Vec<_>>().join("\n");
    Ok((fm, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_frontmatter() {
        let input = "---\nname: foo\ndescription: a thing\n---\nbody here\n";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm.name, "foo");
        assert_eq!(fm.description, "a thing");
        assert_eq!(body.trim(), "body here");
    }

    #[test]
    fn parses_agent_frontmatter_with_tools_list() {
        let input = r#"---
name: graphify-analyst
description: "Does stuff"
model: opus
tools:
  - Bash
  - Read
min_graphify_version: "0.6.0"
---
## System Prompt
..."#;
        let (fm, _) = parse(input).unwrap();
        assert_eq!(fm.model.as_deref(), Some("opus"));
        assert_eq!(
            fm.tools.as_ref().unwrap(),
            &["Bash".to_string(), "Read".into()]
        );
        assert_eq!(fm.min_graphify_version.as_deref(), Some("0.6.0"));
    }

    #[test]
    fn rejects_file_without_opening_fence() {
        let input = "# no frontmatter\n";
        assert!(matches!(parse(input), Err(FrontmatterError::Missing)));
    }

    #[test]
    fn rejects_unterminated_frontmatter() {
        let input = "---\nname: foo\ndescription: oops";
        assert!(matches!(parse(input), Err(FrontmatterError::Unterminated)));
    }
}
