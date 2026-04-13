use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::types::{EdgeKind, Node, NodeKind};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct PolicyConfig {
    #[serde(default)]
    pub group: Vec<PolicyGroup>,
    #[serde(default)]
    pub rule: Vec<PolicyRule>,
}

impl PolicyConfig {
    pub fn is_empty(&self) -> bool {
        self.group.is_empty() && self.rule.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PolicyGroup {
    pub name: String,
    #[serde(rename = "match")]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub partition_by: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PolicyRuleKind {
    Deny,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PolicyRule {
    pub name: String,
    pub kind: PolicyRuleKind,
    pub from: Vec<String>,
    pub to: Vec<String>,
    #[serde(default)]
    pub except_from: Vec<String>,
    #[serde(default)]
    pub except_to: Vec<String>,
    #[serde(default)]
    pub except_same_project: bool,
    #[serde(default)]
    pub allow_same_partition: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyError {
    message: String,
}

impl PolicyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PolicyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyViolation {
    pub rule: String,
    pub source_node: String,
    pub target_node: String,
    pub source_project: String,
    pub target_project: String,
    pub source_selectors: Vec<String>,
    pub target_selectors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPolicyResult {
    pub name: String,
    pub rules_evaluated: usize,
    pub violations: Vec<PolicyViolation>,
}

pub struct ProjectGraph<'a> {
    pub name: &'a str,
    pub graph: &'a CodeGraph,
}

#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    groups: HashMap<String, CompiledGroup>,
    rules: Vec<CompiledRule>,
}

#[derive(Debug, Clone)]
struct CompiledGroup {
    name: String,
    node_matchers: Vec<GlobMatcher>,
    project_matchers: Vec<GlobMatcher>,
    partition_by: Option<PartitionStrategy>,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    rule: PolicyRule,
    from: Vec<Selector>,
    to: Vec<Selector>,
    except_from: Vec<Selector>,
    except_to: Vec<Selector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Selector {
    Group(String),
    Project(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PartitionStrategy {
    Segment(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MatchedSelector {
    value: String,
    group_name: Option<String>,
    partition_key: Option<String>,
}

impl CompiledPolicy {
    pub fn compile(config: &PolicyConfig) -> Result<Self, PolicyError> {
        let mut groups = HashMap::new();

        for group in &config.group {
            if group.name.trim().is_empty() {
                return Err(PolicyError::new("policy group name must not be empty"));
            }
            if group.patterns.is_empty() {
                return Err(PolicyError::new(format!(
                    "policy group '{}' must define at least one match pattern",
                    group.name
                )));
            }
            if groups.contains_key(&group.name) {
                return Err(PolicyError::new(format!(
                    "duplicate policy group '{}'",
                    group.name
                )));
            }

            let partition_by = match &group.partition_by {
                Some(raw) => Some(parse_partition_strategy(raw)?),
                None => None,
            };

            groups.insert(
                group.name.clone(),
                CompiledGroup {
                    name: group.name.clone(),
                    node_matchers: group.patterns.iter().map(|p| GlobMatcher::new(p)).collect(),
                    project_matchers: group.projects.iter().map(|p| GlobMatcher::new(p)).collect(),
                    partition_by,
                },
            );
        }

        let mut seen_rule_names = HashSet::new();
        let mut rules = Vec::new();
        for rule in &config.rule {
            if rule.name.trim().is_empty() {
                return Err(PolicyError::new("policy rule name must not be empty"));
            }
            if !seen_rule_names.insert(rule.name.clone()) {
                return Err(PolicyError::new(format!(
                    "duplicate policy rule '{}'",
                    rule.name
                )));
            }
            if rule.from.is_empty() || rule.to.is_empty() {
                return Err(PolicyError::new(format!(
                    "policy rule '{}' must define both from and to selectors",
                    rule.name
                )));
            }

            rules.push(CompiledRule {
                rule: rule.clone(),
                from: parse_selectors(&rule.from, &groups)?,
                to: parse_selectors(&rule.to, &groups)?,
                except_from: parse_selectors(&rule.except_from, &groups)?,
                except_to: parse_selectors(&rule.except_to, &groups)?,
            });
        }

        Ok(Self { groups, rules })
    }

    pub fn evaluate(&self, projects: &[ProjectGraph<'_>]) -> Vec<ProjectPolicyResult> {
        let owner_index = build_local_owner_index(projects);
        let mut results = Vec::new();

        for project in projects {
            let mut violations = Vec::new();

            for (source_id, target_id, edge) in project.graph.edges() {
                if edge.kind != EdgeKind::Imports {
                    continue;
                }

                let Some(source_node) = project.graph.get_node(source_id) else {
                    continue;
                };
                let Some(target_node) = project.graph.get_node(target_id) else {
                    continue;
                };

                if source_node.kind != NodeKind::Module || target_node.kind != NodeKind::Module {
                    continue;
                }

                let target_projects =
                    target_projects_for_edge(project.name, target_id, &owner_index);

                for target_project in target_projects {
                    for rule in &self.rules {
                        let source_selectors =
                            self.match_selectors(&rule.from, source_node, project.name);
                        if source_selectors.is_empty() {
                            continue;
                        }

                        let target_selectors =
                            self.match_selectors(&rule.to, target_node, &target_project);
                        if target_selectors.is_empty() {
                            continue;
                        }

                        if rule.rule.except_same_project && project.name == target_project {
                            continue;
                        }

                        if !self
                            .match_selectors(&rule.except_from, source_node, project.name)
                            .is_empty()
                        {
                            continue;
                        }

                        if !self
                            .match_selectors(&rule.except_to, target_node, &target_project)
                            .is_empty()
                        {
                            continue;
                        }

                        if rule.rule.allow_same_partition
                            && shares_partition(
                                source_node,
                                target_node,
                                &source_selectors,
                                &target_selectors,
                            )
                        {
                            continue;
                        }

                        if matches!(rule.rule.kind, PolicyRuleKind::Deny) {
                            violations.push(PolicyViolation {
                                rule: rule.rule.name.clone(),
                                source_node: source_id.to_string(),
                                target_node: target_id.to_string(),
                                source_project: project.name.to_string(),
                                target_project: target_project.clone(),
                                source_selectors: selector_values(&source_selectors),
                                target_selectors: selector_values(&target_selectors),
                            });
                        }
                    }
                }
            }

            violations.sort_by(|a, b| {
                a.rule
                    .cmp(&b.rule)
                    .then(a.source_node.cmp(&b.source_node))
                    .then(a.target_node.cmp(&b.target_node))
                    .then(a.target_project.cmp(&b.target_project))
            });

            results.push(ProjectPolicyResult {
                name: project.name.to_string(),
                rules_evaluated: self.rules.len(),
                violations,
            });
        }

        results
    }

    fn match_selectors(
        &self,
        selectors: &[Selector],
        node: &Node,
        project_name: &str,
    ) -> Vec<MatchedSelector> {
        let mut matches = Vec::new();

        for selector in selectors {
            match selector {
                Selector::Project(pattern) => {
                    if GlobMatcher::new(pattern).is_match(project_name) {
                        matches.push(MatchedSelector {
                            value: format!("project:{pattern}"),
                            group_name: None,
                            partition_key: None,
                        });
                    }
                }
                Selector::Group(group_name) => {
                    let Some(group) = self.groups.get(group_name) else {
                        continue;
                    };
                    if !group.matches(node, project_name) {
                        continue;
                    }
                    matches.push(MatchedSelector {
                        value: format!("group:{group_name}"),
                        group_name: Some(group.name.clone()),
                        partition_key: group.partition_key(&node.id),
                    });
                }
            }
        }

        matches
    }
}

impl CompiledGroup {
    fn matches(&self, node: &Node, project_name: &str) -> bool {
        if !self.project_matchers.is_empty()
            && !self
                .project_matchers
                .iter()
                .any(|matcher| matcher.is_match(project_name))
        {
            return false;
        }

        self.node_matchers
            .iter()
            .any(|matcher| matcher.is_match(&node.id))
    }

    fn partition_key(&self, node_id: &str) -> Option<String> {
        self.partition_by
            .as_ref()
            .and_then(|partition| partition.key(node_id))
    }
}

impl PartitionStrategy {
    fn key(&self, node_id: &str) -> Option<String> {
        match self {
            Self::Segment(index) => node_id
                .split('.')
                .nth(*index)
                .map(|segment| segment.to_string()),
        }
    }
}

fn parse_selectors(
    raw_selectors: &[String],
    groups: &HashMap<String, CompiledGroup>,
) -> Result<Vec<Selector>, PolicyError> {
    raw_selectors
        .iter()
        .map(|selector| parse_selector(selector, groups))
        .collect()
}

fn parse_selector(
    raw_selector: &str,
    groups: &HashMap<String, CompiledGroup>,
) -> Result<Selector, PolicyError> {
    if let Some(group) = raw_selector.strip_prefix("group:") {
        if !groups.contains_key(group) {
            return Err(PolicyError::new(format!(
                "unknown policy group selector '{}'",
                raw_selector
            )));
        }
        return Ok(Selector::Group(group.to_string()));
    }

    if let Some(project) = raw_selector.strip_prefix("project:") {
        if project.is_empty() {
            return Err(PolicyError::new("project selector must not be empty"));
        }
        return Ok(Selector::Project(project.to_string()));
    }

    Err(PolicyError::new(format!(
        "unsupported policy selector '{}'; expected group:<name> or project:<glob>",
        raw_selector
    )))
}

fn parse_partition_strategy(raw: &str) -> Result<PartitionStrategy, PolicyError> {
    if let Some(index) = raw.strip_prefix("segment:") {
        let parsed = index.parse::<usize>().map_err(|_| {
            PolicyError::new(format!(
                "invalid partition_by '{}'; expected segment:<index>",
                raw
            ))
        })?;
        return Ok(PartitionStrategy::Segment(parsed));
    }

    Err(PolicyError::new(format!(
        "unsupported partition_by '{}'; expected segment:<index>",
        raw
    )))
}

fn build_local_owner_index(projects: &[ProjectGraph<'_>]) -> HashMap<String, HashSet<String>> {
    let mut owners = HashMap::new();

    for project in projects {
        for node_id in project.graph.local_node_ids() {
            owners
                .entry(node_id.to_string())
                .or_insert_with(HashSet::new)
                .insert(project.name.to_string());
        }
    }

    owners
}

fn target_projects_for_edge(
    current_project: &str,
    target_id: &str,
    owner_index: &HashMap<String, HashSet<String>>,
) -> Vec<String> {
    let mut owners: Vec<String> = owner_index
        .get(target_id)
        .map(|names| names.iter().cloned().collect())
        .unwrap_or_else(|| vec![current_project.to_string()]);
    owners.sort();
    owners.dedup();
    owners
}

fn shares_partition(
    source_node: &Node,
    target_node: &Node,
    source_matches: &[MatchedSelector],
    target_matches: &[MatchedSelector],
) -> bool {
    for source in source_matches {
        let Some(source_group) = &source.group_name else {
            continue;
        };
        let Some(source_partition) = &source.partition_key else {
            continue;
        };

        for target in target_matches {
            let Some(target_group) = &target.group_name else {
                continue;
            };
            let Some(target_partition) = &target.partition_key else {
                continue;
            };

            if source_group == target_group
                && source_partition == target_partition
                && source_node.id != target_node.id
            {
                return true;
            }
        }
    }

    false
}

fn selector_values(matches: &[MatchedSelector]) -> Vec<String> {
    let mut values: Vec<String> = matches.iter().map(|item| item.value.clone()).collect();
    values.sort();
    values.dedup();
    values
}

#[derive(Debug, Clone)]
struct GlobMatcher {
    pattern: Vec<u8>,
}

impl GlobMatcher {
    fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.as_bytes().to_vec(),
        }
    }

    fn is_match(&self, input: &str) -> bool {
        Self::do_match(&self.pattern, input.as_bytes())
    }

    fn do_match(pattern: &[u8], input: &[u8]) -> bool {
        match (pattern.first(), input.first()) {
            (None, None) => true,
            (Some(b'*'), _) => {
                Self::do_match(&pattern[1..], input)
                    || (!input.is_empty() && Self::do_match(pattern, &input[1..]))
            }
            (Some(b'?'), Some(_)) => Self::do_match(&pattern[1..], &input[1..]),
            (Some(&expected), Some(&actual)) if expected == actual => {
                Self::do_match(&pattern[1..], &input[1..])
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Edge, Language};

    fn module(id: &str) -> Node {
        Node::module(id, format!("{id}.ts"), Language::TypeScript, 1, true)
    }

    fn graph_with_imports(nodes: &[&str], edges: &[(&str, &str)]) -> CodeGraph {
        let mut graph = CodeGraph::new();
        graph.set_default_language(Language::TypeScript);
        for node_id in nodes {
            graph.add_node(module(node_id));
        }
        for (source, target) in edges {
            graph.add_edge(source, target, Edge::imports(1));
        }
        graph
    }

    #[test]
    fn compile_rejects_unknown_group_selector() {
        let config = PolicyConfig {
            group: vec![PolicyGroup {
                name: "feature".to_string(),
                patterns: vec!["src.features.*".to_string()],
                projects: vec![],
                partition_by: None,
            }],
            rule: vec![PolicyRule {
                name: "bad".to_string(),
                kind: PolicyRuleKind::Deny,
                from: vec!["group:missing".to_string()],
                to: vec!["group:feature".to_string()],
                except_from: vec![],
                except_to: vec![],
                except_same_project: false,
                allow_same_partition: false,
            }],
        };

        let err = CompiledPolicy::compile(&config).expect_err("expected compile error");
        assert!(err.to_string().contains("unknown policy group selector"));
    }

    #[test]
    fn evaluate_deny_rule_flags_cross_group_dependency() {
        let graph = graph_with_imports(
            &["src.app.entry", "src.infra.db"],
            &[("src.app.entry", "src.infra.db")],
        );
        let policy = CompiledPolicy::compile(&PolicyConfig {
            group: vec![
                PolicyGroup {
                    name: "app".to_string(),
                    patterns: vec!["src.app.*".to_string()],
                    projects: vec![],
                    partition_by: None,
                },
                PolicyGroup {
                    name: "infra".to_string(),
                    patterns: vec!["src.infra.*".to_string()],
                    projects: vec![],
                    partition_by: None,
                },
            ],
            rule: vec![PolicyRule {
                name: "app-cannot-hit-infra".to_string(),
                kind: PolicyRuleKind::Deny,
                from: vec!["group:app".to_string()],
                to: vec!["group:infra".to_string()],
                except_from: vec![],
                except_to: vec![],
                except_same_project: false,
                allow_same_partition: false,
            }],
        })
        .expect("compile policy");

        let results = policy.evaluate(&[ProjectGraph {
            name: "web",
            graph: &graph,
        }]);

        assert_eq!(results[0].violations.len(), 1);
        assert_eq!(results[0].violations[0].rule, "app-cannot-hit-infra");
    }

    #[test]
    fn allow_same_partition_skips_same_feature_and_flags_cross_feature() {
        let graph = graph_with_imports(
            &[
                "src.features.billing.api",
                "src.features.billing.service",
                "src.features.identity.api",
            ],
            &[
                ("src.features.billing.api", "src.features.billing.service"),
                ("src.features.billing.api", "src.features.identity.api"),
            ],
        );
        let policy = CompiledPolicy::compile(&PolicyConfig {
            group: vec![PolicyGroup {
                name: "feature".to_string(),
                patterns: vec!["src.features.*".to_string()],
                projects: vec![],
                partition_by: Some("segment:2".to_string()),
            }],
            rule: vec![PolicyRule {
                name: "no-cross-feature".to_string(),
                kind: PolicyRuleKind::Deny,
                from: vec!["group:feature".to_string()],
                to: vec!["group:feature".to_string()],
                except_from: vec![],
                except_to: vec![],
                except_same_project: false,
                allow_same_partition: true,
            }],
        })
        .expect("compile policy");

        let results = policy.evaluate(&[ProjectGraph {
            name: "web",
            graph: &graph,
        }]);

        assert_eq!(results[0].violations.len(), 1);
        assert_eq!(
            results[0].violations[0].target_node,
            "src.features.identity.api"
        );
    }

    #[test]
    fn except_same_project_only_flags_mapped_cross_project_edges() {
        let web = graph_with_imports(
            &["src.web.entry", "src.shared.types.user"],
            &[("src.web.entry", "src.shared.types.user")],
        );
        let shared = graph_with_imports(&["src.shared.types.user"], &[]);
        let policy = CompiledPolicy::compile(&PolicyConfig {
            group: vec![],
            rule: vec![PolicyRule {
                name: "web-project-allowlist".to_string(),
                kind: PolicyRuleKind::Deny,
                from: vec!["project:web".to_string()],
                to: vec!["project:*".to_string()],
                except_from: vec![],
                except_to: vec!["project:shared".to_string()],
                except_same_project: true,
                allow_same_partition: false,
            }],
        })
        .expect("compile policy");

        let results = policy.evaluate(&[
            ProjectGraph {
                name: "web",
                graph: &web,
            },
            ProjectGraph {
                name: "shared",
                graph: &shared,
            },
        ]);

        assert!(results[0].violations.is_empty());
    }
}
