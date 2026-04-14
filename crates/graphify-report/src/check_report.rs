//! Check report types: shared data model for `graphify check` output.
//!
//! Types are moved from graphify-cli to allow external consumers (for example the
//! `pr_summary` renderer) to deserialize `check-report.json` files produced by
//! `graphify check`. The JSON shape is preserved exactly to match the stdout
//! output historically emitted by `graphify check --json`.

use serde::{Deserialize, Serialize};

use crate::contract_json::ContractCheckResult;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CheckLimits {
    pub max_cycles: Option<usize>,
    pub max_hotspot_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCheckSummary {
    pub nodes: usize,
    pub edges: usize,
    pub communities: usize,
    pub cycles: usize,
    pub max_hotspot_score: f64,
    pub max_hotspot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheckSummary {
    pub rules_evaluated: usize,
    pub policy_violations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CheckViolation {
    #[serde(rename = "limit")]
    Limit {
        kind: String,
        actual: serde_json::Value,
        expected_max: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        node_id: Option<String>,
    },
    #[serde(rename = "policy")]
    Policy {
        kind: String,
        rule: String,
        source_node: String,
        target_node: String,
        source_project: String,
        target_project: String,
        source_selectors: Vec<String>,
        target_selectors: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCheckResult {
    pub name: String,
    pub ok: bool,
    pub summary: ProjectCheckSummary,
    pub limits: CheckLimits,
    pub policy_summary: PolicyCheckSummary,
    pub violations: Vec<CheckViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckReport {
    pub ok: bool,
    pub violations: usize,
    pub projects: Vec<ProjectCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub contracts: Option<ContractCheckResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_report_minimal_roundtrips_json() {
        let report = CheckReport {
            ok: true,
            violations: 0,
            projects: vec![],
            contracts: None,
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let back: CheckReport = serde_json::from_str(&json).expect("deserialize");
        assert!(back.ok);
        assert_eq!(back.violations, 0);
        assert!(back.contracts.is_none());
    }

    #[test]
    fn check_violation_limit_roundtrips_with_node_id() {
        let v = CheckViolation::Limit {
            kind: "max_hotspot_score".into(),
            actual: serde_json::json!(0.9),
            expected_max: serde_json::json!(0.5),
            node_id: Some("app.core".into()),
        };
        let json = serde_json::to_string(&v).expect("serialize");
        let back: CheckViolation = serde_json::from_str(&json).expect("deserialize");
        match back {
            CheckViolation::Limit { kind, node_id, .. } => {
                assert_eq!(kind, "max_hotspot_score");
                assert_eq!(node_id, Some("app.core".into()));
            }
            _ => panic!("expected Limit variant"),
        }
    }

    #[test]
    fn check_violation_policy_roundtrips() {
        let v = CheckViolation::Policy {
            kind: "policy_rule".into(),
            rule: "no_cross_layer".into(),
            source_node: "app.api".into(),
            target_node: "app.repo".into(),
            source_project: "web".into(),
            target_project: "web".into(),
            source_selectors: vec!["api.*".into()],
            target_selectors: vec!["repo.*".into()],
        };
        let json = serde_json::to_string(&v).expect("serialize");
        let back: CheckViolation = serde_json::from_str(&json).expect("deserialize");
        match back {
            CheckViolation::Policy { rule, .. } => assert_eq!(rule, "no_cross_layer"),
            _ => panic!("expected Policy variant"),
        }
    }

    #[test]
    fn check_report_with_project_and_contract_roundtrips() {
        let json_in = r#"{
          "ok": false,
          "violations": 2,
          "projects": [{
            "name": "web",
            "ok": false,
            "summary": {
              "nodes": 10, "edges": 20, "communities": 3, "cycles": 1,
              "max_hotspot_score": 0.6, "max_hotspot_id": "app.core"
            },
            "limits": { "max_cycles": 0, "max_hotspot_score": 0.5 },
            "policy_summary": { "rules_evaluated": 0, "policy_violations": 0 },
            "violations": [
              {"type": "limit", "kind": "max_cycles", "actual": 1, "expected_max": 0}
            ]
          }],
          "contracts": {
            "ok": false, "error_count": 1, "warning_count": 0, "pairs": []
          }
        }"#;
        let back: CheckReport = serde_json::from_str(json_in).expect("deserialize");
        assert!(!back.ok);
        assert_eq!(back.projects.len(), 1);
        assert_eq!(back.projects[0].violations.len(), 1);
        assert!(back.contracts.as_ref().unwrap().error_count == 1);
    }

    #[test]
    fn check_report_missing_contracts_field_deserializes_as_none() {
        let json_in = r#"{"ok": true, "violations": 0, "projects": []}"#;
        let back: CheckReport = serde_json::from_str(json_in).expect("deserialize");
        assert!(back.contracts.is_none());
    }

    #[test]
    fn check_limits_default_is_none_all() {
        let limits = CheckLimits::default();
        assert!(limits.max_cycles.is_none());
        assert!(limits.max_hotspot_score.is_none());
    }
}
