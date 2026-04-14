use graphify_core::contract::{ContractViolation, Severity};

use crate::contract_json::ContractCheckResult;

pub fn write_contract_markdown_section(result: &ContractCheckResult) -> String {
    if result.pairs.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("## Contract Drift\n\n");
    out.push_str(&format!(
        "{} pair(s), {} error(s), {} warning(s).\n\n",
        result.pairs.len(),
        result.error_count,
        result.warning_count,
    ));
    for pair in &result.pairs {
        out.push_str(&format!("### {}\n", pair.name));
        out.push_str(&format!(
            "orm: `{}::{}` (line {})  \n",
            pair.orm.file.display(),
            pair.orm.symbol,
            pair.orm.line,
        ));
        out.push_str(&format!(
            "ts:  `{}::{}` (line {})\n\n",
            pair.ts.file.display(),
            pair.ts.symbol,
            pair.ts.line,
        ));
        if pair.violations.is_empty() {
            out.push_str("_No violations._\n\n");
            continue;
        }
        out.push_str("| severity | kind | field | details |\n");
        out.push_str("|---|---|---|---|\n");
        for v in &pair.violations {
            let severity = match v.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let (kind, field, details) = describe_violation(&v.violation);
            out.push_str(&format!("| {severity} | {kind} | {field} | {details} |\n"));
        }
        out.push('\n');
    }
    out
}

fn describe_violation(v: &ContractViolation) -> (&'static str, String, String) {
    match v {
        ContractViolation::ContractFieldMissingOnTs { field, orm_line, .. } => (
            "contract_field_missing_on_ts",
            field.clone(),
            format!("orm line {orm_line}"),
        ),
        ContractViolation::ContractFieldMissingOnOrm { field, ts_line, .. } => (
            "contract_field_missing_on_orm",
            field.clone(),
            format!("ts line {ts_line}"),
        ),
        ContractViolation::ContractTypeMismatch {
            field,
            orm_line,
            ts_line,
            ..
        } => (
            "contract_type_mismatch",
            field.clone(),
            format!("orm line {orm_line}, ts line {ts_line}"),
        ),
        ContractViolation::ContractNullabilityMismatch {
            field,
            orm_nullable,
            ts_nullable,
            orm_line,
            ts_line,
        } => (
            "contract_nullability_mismatch",
            field.clone(),
            format!(
                "orm nullable={orm_nullable}, ts nullable={ts_nullable} (orm {orm_line}, ts {ts_line})"
            ),
        ),
        ContractViolation::ContractRelationMissingOnTs { relation, orm_line } => (
            "contract_relation_missing_on_ts",
            relation.clone(),
            format!("orm line {orm_line}"),
        ),
        ContractViolation::ContractRelationMissingOnOrm { relation, ts_line } => (
            "contract_relation_missing_on_orm",
            relation.clone(),
            format!("ts line {ts_line}"),
        ),
        ContractViolation::ContractCardinalityMismatch {
            relation,
            orm,
            ts,
            orm_line,
            ts_line,
        } => (
            "contract_cardinality_mismatch",
            relation.clone(),
            format!("orm={orm:?}, ts={ts:?} (orm {orm_line}, ts {ts_line})"),
        ),
        ContractViolation::ContractUnmappedOrmType {
            field,
            raw_type,
            orm_line,
        } => (
            "contract_unmapped_orm_type",
            field.clone(),
            format!("raw orm type: `{raw_type}` (orm line {orm_line})"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_json::{
        build_contract_check_result, ContractPairResult, ContractSideInfo, ViolationEntry,
    };
    use graphify_core::contract::{FieldType, PrimitiveType, Severity};
    use std::path::PathBuf;

    #[test]
    fn renders_section_with_violations() {
        let pair = ContractPairResult {
            name: "user".into(),
            orm: ContractSideInfo {
                file: PathBuf::from("schema/user.ts"),
                symbol: "users".into(),
                line: 10,
            },
            ts: ContractSideInfo {
                file: PathBuf::from("api/user.ts"),
                symbol: "UserDto".into(),
                line: 3,
            },
            violations: vec![ViolationEntry {
                severity: Severity::Error,
                violation: ContractViolation::ContractFieldMissingOnTs {
                    field: "phone".into(),
                    orm_type: FieldType::Primitive {
                        value: PrimitiveType::String,
                    },
                    orm_line: 15,
                },
            }],
        };
        let result = build_contract_check_result(vec![pair], Severity::Warning);
        let md = write_contract_markdown_section(&result);
        assert!(md.contains("## Contract Drift"));
        assert!(md.contains("### user"));
        assert!(md.contains("contract_field_missing_on_ts"));
        assert!(md.contains("| error |"));
    }

    #[test]
    fn empty_result_produces_empty_string() {
        let result = build_contract_check_result(Vec::new(), Severity::Warning);
        let md = write_contract_markdown_section(&result);
        assert!(md.is_empty());
    }
}
