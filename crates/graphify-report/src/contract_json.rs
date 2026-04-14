use std::path::PathBuf;

use graphify_core::contract::{ContractViolation, Severity};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ContractCheckResult {
    pub ok: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub pairs: Vec<ContractPairResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContractPairResult {
    pub name: String,
    pub orm: ContractSideInfo,
    pub ts: ContractSideInfo,
    pub violations: Vec<ViolationEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContractSideInfo {
    pub file: PathBuf,
    pub symbol: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViolationEntry {
    pub severity: Severity,
    #[serde(flatten)]
    pub violation: ContractViolation,
}

pub fn build_contract_check_result(
    pairs: Vec<ContractPairResult>,
    unmapped_severity: Severity,
) -> ContractCheckResult {
    let mut error_count = 0usize;
    let mut warning_count = 0usize;
    for p in &pairs {
        for v in &p.violations {
            match v.severity {
                Severity::Error => error_count += 1,
                Severity::Warning => warning_count += 1,
            }
        }
    }
    let _ = unmapped_severity; // reserved for future per-pair overrides
    ContractCheckResult {
        ok: error_count == 0,
        error_count,
        warning_count,
        pairs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::contract::{ContractViolation, FieldType, PrimitiveType, Severity};

    fn pair_fixture() -> ContractPairResult {
        ContractPairResult {
            name: "user".into(),
            orm: ContractSideInfo {
                file: PathBuf::from("packages/db/src/schema/user.ts"),
                symbol: "users".into(),
                line: 12,
            },
            ts: ContractSideInfo {
                file: PathBuf::from("packages/api/src/types/user.ts"),
                symbol: "UserDto".into(),
                line: 5,
            },
            violations: vec![
                ViolationEntry {
                    severity: Severity::Error,
                    violation: ContractViolation::ContractFieldMissingOnTs {
                        field: "phone".into(),
                        orm_type: FieldType::Primitive {
                            value: PrimitiveType::String,
                        },
                        orm_line: 18,
                    },
                },
                ViolationEntry {
                    severity: Severity::Warning,
                    violation: ContractViolation::ContractUnmappedOrmType {
                        field: "tags".into(),
                        raw_type: "tsvector".into(),
                        orm_line: 31,
                    },
                },
            ],
        }
    }

    #[test]
    fn counts_errors_and_warnings() {
        let result = build_contract_check_result(vec![pair_fixture()], Severity::Warning);
        assert_eq!(result.error_count, 1);
        assert_eq!(result.warning_count, 1);
        assert!(!result.ok);
    }

    #[test]
    fn serializes_with_expected_schema() {
        let result = build_contract_check_result(vec![pair_fixture()], Severity::Warning);
        let value: serde_json::Value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error_count"], 1);
        assert_eq!(value["warning_count"], 1);
        let pair0 = &value["pairs"][0];
        assert_eq!(pair0["name"], "user");
        assert_eq!(pair0["orm"]["symbol"], "users");
        assert_eq!(pair0["ts"]["symbol"], "UserDto");
        let v0 = &pair0["violations"][0];
        assert_eq!(v0["severity"], "error");
        assert_eq!(v0["kind"], "contract_field_missing_on_ts");
        assert_eq!(v0["field"], "phone");
        assert_eq!(v0["orm_line"], 18);
        let v1 = &pair0["violations"][1];
        assert_eq!(v1["kind"], "contract_unmapped_orm_type");
        assert_eq!(v1["raw_type"], "tsvector");
    }
}
