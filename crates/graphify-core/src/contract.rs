use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data model — mirrors spec Section 4
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    pub name: String,
    pub side: ContractSide,
    pub source_file: PathBuf,
    pub source_symbol: String,
    pub fields: Vec<Field>,
    pub relations: Vec<Relation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractSide {
    Orm,
    Ts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub raw_name: String,
    pub type_ref: FieldType,
    pub nullable: bool,
    #[serde(default)]
    pub has_default: bool,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldType {
    Primitive { value: PrimitiveType },
    Named { value: String },
    Union { value: Vec<FieldType> },
    Array { value: Box<FieldType> },
    Unmapped { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveType {
    String,
    Number,
    Boolean,
    Date,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub name: String,
    pub raw_name: String,
    pub cardinality: Cardinality,
    pub target_contract: String,
    pub nullable: bool,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    One,
    Many,
}

// ---------------------------------------------------------------------------
// Violations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContractComparison {
    pub pair_name: String,
    pub violations: Vec<ContractViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContractViolation {
    ContractFieldMissingOnTs {
        field: String,
        orm_type: FieldType,
        orm_line: usize,
    },
    ContractFieldMissingOnOrm {
        field: String,
        ts_type: FieldType,
        ts_line: usize,
    },
    ContractTypeMismatch {
        field: String,
        orm: FieldType,
        ts: FieldType,
        orm_line: usize,
        ts_line: usize,
    },
    ContractNullabilityMismatch {
        field: String,
        orm_nullable: bool,
        ts_nullable: bool,
        orm_line: usize,
        ts_line: usize,
    },
    ContractRelationMissingOnTs {
        relation: String,
        orm_line: usize,
    },
    ContractRelationMissingOnOrm {
        relation: String,
        ts_line: usize,
    },
    ContractCardinalityMismatch {
        relation: String,
        orm: Cardinality,
        ts: Cardinality,
        orm_line: usize,
        ts_line: usize,
    },
    ContractUnmappedOrmType {
        field: String,
        raw_type: String,
        orm_line: usize,
    },
}

impl ContractViolation {
    pub fn severity(&self, unmapped_severity: Severity) -> Severity {
        match self {
            ContractViolation::ContractUnmappedOrmType { .. } => unmapped_severity,
            _ => Severity::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) struct ContractBuilder {
    inner: Contract,
}

#[cfg(test)]
#[allow(dead_code)]
impl ContractBuilder {
    pub fn orm(name: &str) -> Self {
        Self::new(name, ContractSide::Orm)
    }

    pub fn ts(name: &str) -> Self {
        Self::new(name, ContractSide::Ts)
    }

    fn new(name: &str, side: ContractSide) -> Self {
        Self {
            inner: Contract {
                name: name.to_string(),
                side,
                source_file: PathBuf::from(format!("<{name}>")),
                source_symbol: name.to_string(),
                fields: Vec::new(),
                relations: Vec::new(),
            },
        }
    }

    pub fn primitive(mut self, name: &str, prim: PrimitiveType, nullable: bool) -> Self {
        let line = self.inner.fields.len() + 1;
        self.inner.fields.push(Field {
            name: name.to_string(),
            raw_name: name.to_string(),
            type_ref: FieldType::Primitive { value: prim },
            nullable,
            has_default: false,
            line,
        });
        self
    }

    pub fn raw_primitive(
        mut self,
        normalized: &str,
        raw: &str,
        prim: PrimitiveType,
        nullable: bool,
    ) -> Self {
        let line = self.inner.fields.len() + 1;
        self.inner.fields.push(Field {
            name: normalized.to_string(),
            raw_name: raw.to_string(),
            type_ref: FieldType::Primitive { value: prim },
            nullable,
            has_default: false,
            line,
        });
        self
    }

    pub fn named(mut self, name: &str, named_ref: &str, nullable: bool) -> Self {
        let line = self.inner.fields.len() + 1;
        self.inner.fields.push(Field {
            name: name.to_string(),
            raw_name: name.to_string(),
            type_ref: FieldType::Named {
                value: named_ref.to_string(),
            },
            nullable,
            has_default: false,
            line,
        });
        self
    }

    pub fn unmapped(mut self, name: &str, raw: &str) -> Self {
        let line = self.inner.fields.len() + 1;
        self.inner.fields.push(Field {
            name: name.to_string(),
            raw_name: name.to_string(),
            type_ref: FieldType::Unmapped {
                value: raw.to_string(),
            },
            nullable: true,
            has_default: false,
            line,
        });
        self
    }

    pub fn relation(
        mut self,
        name: &str,
        cardinality: Cardinality,
        target: &str,
        nullable: bool,
    ) -> Self {
        let line = self.inner.relations.len() + 1;
        self.inner.relations.push(Relation {
            name: name.to_string(),
            raw_name: name.to_string(),
            cardinality,
            target_contract: target.to_string(),
            nullable,
            line,
        });
        self
    }

    pub fn build(self) -> Contract {
        self.inner
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_orm_contract() {
        let c = ContractBuilder::orm("users")
            .primitive("id", PrimitiveType::String, false)
            .primitive("email", PrimitiveType::String, false)
            .primitive("age", PrimitiveType::Number, true)
            .relation("posts", Cardinality::Many, "post", true)
            .build();

        assert_eq!(c.name, "users");
        assert_eq!(c.side, ContractSide::Orm);
        assert_eq!(c.fields.len(), 3);
        assert_eq!(c.relations.len(), 1);
        assert_eq!(c.fields[0].name, "id");
        assert!(!c.fields[0].nullable);
        assert!(c.fields[2].nullable);
        assert_eq!(c.relations[0].cardinality, Cardinality::Many);
    }

    #[test]
    fn violation_severity_defaults() {
        let v = ContractViolation::ContractFieldMissingOnTs {
            field: "phone".into(),
            orm_type: FieldType::Primitive {
                value: PrimitiveType::String,
            },
            orm_line: 10,
        };
        assert_eq!(v.severity(Severity::Warning), Severity::Error);

        let w = ContractViolation::ContractUnmappedOrmType {
            field: "tags".into(),
            raw_type: "tsvector".into(),
            orm_line: 31,
        };
        assert_eq!(w.severity(Severity::Warning), Severity::Warning);
        assert_eq!(w.severity(Severity::Error), Severity::Error);
    }

    #[test]
    fn contract_round_trips_through_json() {
        let original = ContractBuilder::ts("user")
            .primitive("id", PrimitiveType::String, false)
            .named("metadata", "UserMetadata", true)
            .relation("profile", Cardinality::One, "profile", true)
            .build();
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Contract = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }
}
