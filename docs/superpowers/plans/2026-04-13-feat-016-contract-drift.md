# FEAT-016 — Contract Drift Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship v1 of contract drift detection between Drizzle ORM schemas and TypeScript interface/type declarations, integrated into `graphify check`.

**Architecture:** Pure comparison engine lives in `graphify-core/src/contract.rs`. Two TS-AST-driven parsers live in `graphify-extract` (`drizzle.rs` and `ts_contract.rs`) and reuse the existing tree-sitter TS parser. Output extends the existing `CheckReport` with an additive workspace-level `contracts` block. No new tree-sitter grammar.

**Tech Stack:** Rust 2021, tree-sitter 0.25, tree-sitter-typescript 0.23, serde 1, serde_json 1, clap 4, toml 0.8.

**Spec:** `docs/superpowers/specs/2026-04-13-feat-016-contract-drift-design.md` — all 11 sections assumed read.

---

## File Structure

### New files

| File | Responsibility |
|---|---|
| `crates/graphify-core/src/contract.rs` | `Contract`, `Field`, `Relation`, `FieldType`, `ContractViolation`, `compare_contracts` pure function, `ContractBuilder` test helper |
| `crates/graphify-extract/src/drizzle.rs` | `extract_drizzle_contract(&str, &str) -> Result<Contract, DrizzleParseError>` plus multi-table variant |
| `crates/graphify-extract/src/ts_contract.rs` | `extract_ts_contract(&str, &str) -> Result<Contract, TsContractParseError>` plus scalar-vs-relation classification sweep |
| `crates/graphify-report/src/contract_json.rs` | `ContractCheckResult`, `ContractPairResult`, serialization matching spec schema |
| `crates/graphify-report/src/contract_markdown.rs` | `write_contract_markdown_section(...)` |
| `tests/contract_integration.rs` | 9 CLI integration tests using the `OnceLock` `graphify_bin()` harness |
| `tests/fixtures/contract_drift/monorepo/graphify.toml` | 2-project workspace + 2 pairs (1 clean, 1 drifted) |
| `tests/fixtures/contract_drift/monorepo/packages/db/src/schema/user.ts` | Clean Drizzle table |
| `tests/fixtures/contract_drift/monorepo/packages/api/src/types/user.ts` | Matching TS interface |
| `tests/fixtures/contract_drift/monorepo/packages/db/src/schema/post.ts` | Drizzle table with drift |
| `tests/fixtures/contract_drift/monorepo/packages/api/src/types/post.ts` | TS interface with cardinality + field drift |

### Modified files

| File | Modification |
|---|---|
| `crates/graphify-core/src/lib.rs` | `pub mod contract;` |
| `crates/graphify-extract/src/lib.rs` | `pub mod drizzle;`, `pub mod ts_contract;`, re-exports |
| `crates/graphify-report/src/lib.rs` | `pub mod contract_json;`, `pub mod contract_markdown;`, re-exports |
| `crates/graphify-cli/src/main.rs` | Extend `Config` with `ContractConfig`; extend `CheckReport` with `contracts: Option<ContractCheckResult>`; add `--contracts`, `--no-contracts`, `--contracts-warnings-as-errors` CLI flags; wire contract parsing + comparison into `cmd_check`; extend human and markdown output |

---

## Task 1: Data model + round-trip tests

**Files:**
- Create: `crates/graphify-core/src/contract.rs`
- Modify: `crates/graphify-core/src/lib.rs` (add module)

- [ ] **Step 1: Declare the module**

Modify `crates/graphify-core/src/lib.rs` to add the new module declaration alphabetically:

```rust
pub mod community;
pub mod contract;
pub mod cycles;
pub mod diff;
pub mod graph;
pub mod history;
pub mod metrics;
pub mod policy;
pub mod query;
pub mod types;
```

- [ ] **Step 2: Create `contract.rs` with the type definitions**

Create `crates/graphify-core/src/contract.rs`:

```rust
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
```

- [ ] **Step 3: Run tests and verify they pass**

Run: `cargo test -p graphify-core contract::`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-core/src/contract.rs crates/graphify-core/src/lib.rs
git commit -m "feat(core): contract drift data model and test builder (FEAT-016)"
```

---

## Task 2: Field alignment (Phase 1 of comparison)

**Files:**
- Modify: `crates/graphify-core/src/contract.rs` (add config + alignment + public comparison entry point)

- [ ] **Step 1: Write failing tests**

Append to the `tests` module in `crates/graphify-core/src/contract.rs`:

```rust
    fn default_config() -> (PairConfig, GlobalContractConfig) {
        (PairConfig::default(), GlobalContractConfig::default())
    }

    #[test]
    fn alignment_detects_field_missing_on_ts() {
        let orm = ContractBuilder::orm("user")
            .primitive("id", PrimitiveType::String, false)
            .primitive("phone", PrimitiveType::String, false)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("id", PrimitiveType::String, false)
            .build();
        let (pair, global) = default_config();
        let cmp = compare_contracts(&orm, &ts, &pair, &global);
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(
            cmp.violations[0],
            ContractViolation::ContractFieldMissingOnTs { ref field, .. } if field == "phone"
        ));
    }

    #[test]
    fn alignment_detects_field_missing_on_orm() {
        let orm = ContractBuilder::orm("user")
            .primitive("id", PrimitiveType::String, false)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("id", PrimitiveType::String, false)
            .primitive("nickname", PrimitiveType::String, true)
            .build();
        let (pair, global) = default_config();
        let cmp = compare_contracts(&orm, &ts, &pair, &global);
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(
            cmp.violations[0],
            ContractViolation::ContractFieldMissingOnOrm { ref field, .. } if field == "nickname"
        ));
    }

    #[test]
    fn alignment_respects_ignore_list() {
        let orm = ContractBuilder::orm("user")
            .primitive("id", PrimitiveType::String, false)
            .primitive("internal_audit_id", PrimitiveType::String, true)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("id", PrimitiveType::String, false)
            .build();
        let pair = PairConfig {
            ignore_orm: vec!["internal_audit_id".into()],
            ..PairConfig::default()
        };
        let cmp = compare_contracts(&orm, &ts, &pair, &GlobalContractConfig::default());
        assert_eq!(cmp.violations.len(), 0);
    }

    #[test]
    fn alignment_applies_field_alias() {
        let orm = ContractBuilder::orm("user")
            .raw_primitive("legacyRoleCode", "legacy_role_code", PrimitiveType::String, false)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("roleCode", PrimitiveType::String, false)
            .build();
        let pair = PairConfig {
            field_aliases: vec![FieldAlias {
                orm: "legacy_role_code".into(),
                ts: "roleCode".into(),
            }],
            ..PairConfig::default()
        };
        let cmp = compare_contracts(&orm, &ts, &pair, &GlobalContractConfig::default());
        assert_eq!(cmp.violations, vec![]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-core contract::`
Expected: `compare_contracts` unresolved, `PairConfig` / `GlobalContractConfig` / `FieldAlias` unresolved.

- [ ] **Step 3: Add config types and the alignment entry point**

Append to `crates/graphify-core/src/contract.rs` above the `#[cfg(test)]` block:

```rust
// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PairConfig {
    pub ignore_orm: Vec<String>,
    pub ignore_ts: Vec<String>,
    pub field_aliases: Vec<FieldAlias>,
    pub relation_aliases: Vec<FieldAlias>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAlias {
    pub orm: String,
    pub ts: String,
}

#[derive(Debug, Clone)]
pub struct GlobalContractConfig {
    pub case_rule: CaseRule,
    pub type_map_overrides: std::collections::HashMap<String, FieldType>,
    pub unmapped_type_severity: Severity,
}

impl Default for GlobalContractConfig {
    fn default() -> Self {
        Self {
            case_rule: CaseRule::SnakeCamel,
            type_map_overrides: std::collections::HashMap::new(),
            unmapped_type_severity: Severity::Warning,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseRule {
    SnakeCamel,
    Exact,
}

// ---------------------------------------------------------------------------
// Comparison entry point
// ---------------------------------------------------------------------------

pub fn compare_contracts(
    orm: &Contract,
    ts: &Contract,
    pair: &PairConfig,
    global: &GlobalContractConfig,
) -> ContractComparison {
    let orm_fields = project_fields(&orm.fields, &pair.ignore_orm, AlignmentSide::Orm, pair, global);
    let ts_fields = project_fields(&ts.fields, &pair.ignore_ts, AlignmentSide::Ts, pair, global);

    let mut violations = Vec::new();

    for (key, orm_field) in &orm_fields {
        if !ts_fields.contains_key(key) {
            violations.push(ContractViolation::ContractFieldMissingOnTs {
                field: key.clone(),
                orm_type: orm_field.type_ref.clone(),
                orm_line: orm_field.line,
            });
        }
    }
    for (key, ts_field) in &ts_fields {
        if !orm_fields.contains_key(key) {
            violations.push(ContractViolation::ContractFieldMissingOnOrm {
                field: key.clone(),
                ts_type: ts_field.type_ref.clone(),
                ts_line: ts_field.line,
            });
        }
    }

    ContractComparison {
        pair_name: orm.name.clone(),
        violations,
    }
}

#[derive(Clone, Copy)]
enum AlignmentSide {
    Orm,
    Ts,
}

fn project_fields(
    fields: &[Field],
    ignore: &[String],
    side: AlignmentSide,
    pair: &PairConfig,
    global: &GlobalContractConfig,
) -> std::collections::BTreeMap<String, Field> {
    let ignore_set: std::collections::HashSet<&str> = ignore.iter().map(String::as_str).collect();
    let mut out = std::collections::BTreeMap::new();
    for f in fields {
        if ignore_set.contains(f.raw_name.as_str()) || ignore_set.contains(f.name.as_str()) {
            continue;
        }
        let key = alignment_key(f, side, pair, global);
        out.insert(key, f.clone());
    }
    out
}

fn alignment_key(
    field: &Field,
    side: AlignmentSide,
    pair: &PairConfig,
    global: &GlobalContractConfig,
) -> String {
    for alias in &pair.field_aliases {
        match side {
            AlignmentSide::Orm if alias.orm == field.raw_name => return alias.ts.clone(),
            AlignmentSide::Ts if alias.ts == field.raw_name => return alias.ts.clone(),
            _ => {}
        }
    }
    match global.case_rule {
        CaseRule::Exact => field.raw_name.clone(),
        CaseRule::SnakeCamel => snake_to_camel(&field.raw_name),
    }
}

fn snake_to_camel(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut upper_next = false;
    for ch in input.chars() {
        if ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p graphify-core contract::`
Expected: 7 passing tests (3 from Task 1 + 4 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/src/contract.rs
git commit -m "feat(core): field alignment for contract comparison (FEAT-016)"
```

---

## Task 3: Per-field comparison (nullability + type)

**Files:**
- Modify: `crates/graphify-core/src/contract.rs`

- [ ] **Step 1: Write failing tests**

Append to `tests` module:

```rust
    #[test]
    fn per_field_reports_nullability_mismatch() {
        let orm = ContractBuilder::orm("user")
            .primitive("age", PrimitiveType::Number, false)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("age", PrimitiveType::Number, true)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(
            cmp.violations[0],
            ContractViolation::ContractNullabilityMismatch { orm_nullable: false, ts_nullable: true, .. }
        ));
    }

    #[test]
    fn per_field_reports_type_mismatch() {
        let orm = ContractBuilder::orm("user")
            .primitive("age", PrimitiveType::Number, false)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("age", PrimitiveType::String, false)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(cmp.violations[0], ContractViolation::ContractTypeMismatch { .. }));
    }

    #[test]
    fn per_field_type_match_named_vs_named() {
        let orm = ContractBuilder::orm("user")
            .named("metadata", "UserMetadata", true)
            .build();
        let ts = ContractBuilder::ts("user")
            .named("metadata", "UserMetadata", true)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations, vec![]);
    }

    #[test]
    fn per_field_unmapped_orm_type_emits_warning_variant() {
        let orm = ContractBuilder::orm("post")
            .unmapped("tags", "tsvector")
            .build();
        let ts = ContractBuilder::ts("post")
            .named("tags", "TsVector", true)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(cmp.violations[0], ContractViolation::ContractUnmappedOrmType { .. }));
        assert_eq!(
            cmp.violations[0].severity(GlobalContractConfig::default().unmapped_type_severity),
            Severity::Warning
        );
    }

    #[test]
    fn per_field_unknown_primitive_matches_anything() {
        let orm = ContractBuilder::orm("blob")
            .primitive("payload", PrimitiveType::Unknown, true)
            .build();
        let ts = ContractBuilder::ts("blob")
            .named("payload", "Record<string, unknown>", true)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations, vec![]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-core contract::`
Expected: at least 4 new failures (type/nullability/unmapped checks not yet emitted).

- [ ] **Step 3: Extend `compare_contracts` with Phase 2**

Replace the `compare_contracts` function body in `contract.rs` with a version that also compares shared fields:

```rust
pub fn compare_contracts(
    orm: &Contract,
    ts: &Contract,
    pair: &PairConfig,
    global: &GlobalContractConfig,
) -> ContractComparison {
    let orm_fields = project_fields(&orm.fields, &pair.ignore_orm, AlignmentSide::Orm, pair, global);
    let ts_fields = project_fields(&ts.fields, &pair.ignore_ts, AlignmentSide::Ts, pair, global);

    let mut violations = Vec::new();

    for (key, orm_field) in &orm_fields {
        match ts_fields.get(key) {
            None => violations.push(ContractViolation::ContractFieldMissingOnTs {
                field: key.clone(),
                orm_type: orm_field.type_ref.clone(),
                orm_line: orm_field.line,
            }),
            Some(ts_field) => {
                compare_shared_field(key, orm_field, ts_field, global, &mut violations);
            }
        }
    }
    for (key, ts_field) in &ts_fields {
        if !orm_fields.contains_key(key) {
            violations.push(ContractViolation::ContractFieldMissingOnOrm {
                field: key.clone(),
                ts_type: ts_field.type_ref.clone(),
                ts_line: ts_field.line,
            });
        }
    }

    ContractComparison {
        pair_name: orm.name.clone(),
        violations,
    }
}

fn compare_shared_field(
    key: &str,
    orm: &Field,
    ts: &Field,
    global: &GlobalContractConfig,
    out: &mut Vec<ContractViolation>,
) {
    if orm.nullable != ts.nullable {
        out.push(ContractViolation::ContractNullabilityMismatch {
            field: key.to_string(),
            orm_nullable: orm.nullable,
            ts_nullable: ts.nullable,
            orm_line: orm.line,
            ts_line: ts.line,
        });
    }

    // Type check: apply type map overrides to ORM side if it's a Named/Unmapped token.
    let orm_resolved = resolve_orm_type(&orm.type_ref, global);

    match &orm_resolved {
        FieldType::Unmapped { value } => {
            out.push(ContractViolation::ContractUnmappedOrmType {
                field: key.to_string(),
                raw_type: value.clone(),
                orm_line: orm.line,
            });
            // Skip type comparison for unmapped types.
        }
        _ => {
            if !types_match(&orm_resolved, &ts.type_ref) {
                out.push(ContractViolation::ContractTypeMismatch {
                    field: key.to_string(),
                    orm: orm_resolved,
                    ts: ts.type_ref.clone(),
                    orm_line: orm.line,
                    ts_line: ts.line,
                });
            }
        }
    }
}

fn resolve_orm_type(ty: &FieldType, global: &GlobalContractConfig) -> FieldType {
    match ty {
        FieldType::Unmapped { value } => {
            if let Some(override_ty) = global.type_map_overrides.get(value) {
                override_ty.clone()
            } else {
                ty.clone()
            }
        }
        _ => ty.clone(),
    }
}

fn types_match(a: &FieldType, b: &FieldType) -> bool {
    use FieldType::*;
    match (a, b) {
        (Primitive { value: PrimitiveType::Unknown }, _) => true,
        (_, Primitive { value: PrimitiveType::Unknown }) => true,
        (Primitive { value: x }, Primitive { value: y }) => x == y,
        (Named { value: x }, Named { value: y }) => x == y,
        (Array { value: x }, Array { value: y }) => types_match(x, y),
        (Union { value: xs }, Union { value: ys }) => {
            xs.len() == ys.len()
                && xs.iter().zip(ys.iter()).all(|(p, q)| types_match(p, q))
        }
        (Unmapped { value: x }, Unmapped { value: y }) => x == y,
        _ => false,
    }
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p graphify-core contract::`
Expected: all prior tests plus 5 new = 12 passing.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/src/contract.rs
git commit -m "feat(core): per-field nullability and type comparison (FEAT-016)"
```

---

## Task 4: Relation alignment and cardinality

**Files:**
- Modify: `crates/graphify-core/src/contract.rs`

- [ ] **Step 1: Write failing tests**

Append to `tests` module:

```rust
    #[test]
    fn relation_missing_on_ts() {
        let orm = ContractBuilder::orm("user")
            .relation("posts", Cardinality::Many, "post", true)
            .build();
        let ts = ContractBuilder::ts("user").build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(
            cmp.violations[0],
            ContractViolation::ContractRelationMissingOnTs { ref relation, .. } if relation == "posts"
        ));
    }

    #[test]
    fn relation_cardinality_mismatch() {
        let orm = ContractBuilder::orm("user")
            .relation("author", Cardinality::One, "user", true)
            .build();
        let ts = ContractBuilder::ts("user")
            .relation("author", Cardinality::Many, "user", true)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations.len(), 1);
        assert!(matches!(
            cmp.violations[0],
            ContractViolation::ContractCardinalityMismatch { orm: Cardinality::One, ts: Cardinality::Many, .. }
        ));
    }

    #[test]
    fn relation_target_contract_not_compared() {
        let orm = ContractBuilder::orm("user")
            .relation("profile", Cardinality::One, "profile", true)
            .build();
        let ts = ContractBuilder::ts("user")
            .relation("profile", Cardinality::One, "profile_summary", true)
            .build();
        let cmp = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp.violations, vec![]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-core contract::`
Expected: 3 new failures.

- [ ] **Step 3: Add relation alignment to `compare_contracts`**

Extend `compare_contracts` in `contract.rs` to include relation alignment after the field loops. Add this block just before the final `ContractComparison` construction:

```rust
    let orm_relations = project_relations(&orm.relations, pair, AlignmentSide::Orm, global);
    let ts_relations = project_relations(&ts.relations, pair, AlignmentSide::Ts, global);

    for (key, orm_rel) in &orm_relations {
        match ts_relations.get(key) {
            None => violations.push(ContractViolation::ContractRelationMissingOnTs {
                relation: key.clone(),
                orm_line: orm_rel.line,
            }),
            Some(ts_rel) => {
                if orm_rel.cardinality != ts_rel.cardinality {
                    violations.push(ContractViolation::ContractCardinalityMismatch {
                        relation: key.clone(),
                        orm: orm_rel.cardinality,
                        ts: ts_rel.cardinality,
                        orm_line: orm_rel.line,
                        ts_line: ts_rel.line,
                    });
                }
            }
        }
    }
    for (key, ts_rel) in &ts_relations {
        if !orm_relations.contains_key(key) {
            violations.push(ContractViolation::ContractRelationMissingOnOrm {
                relation: key.clone(),
                ts_line: ts_rel.line,
            });
        }
    }
```

Add the helper:

```rust
fn project_relations(
    relations: &[Relation],
    pair: &PairConfig,
    side: AlignmentSide,
    global: &GlobalContractConfig,
) -> std::collections::BTreeMap<String, Relation> {
    let mut out = std::collections::BTreeMap::new();
    for r in relations {
        let key = relation_alignment_key(r, side, pair, global);
        out.insert(key, r.clone());
    }
    out
}

fn relation_alignment_key(
    rel: &Relation,
    side: AlignmentSide,
    pair: &PairConfig,
    global: &GlobalContractConfig,
) -> String {
    for alias in &pair.relation_aliases {
        match side {
            AlignmentSide::Orm if alias.orm == rel.raw_name => return alias.ts.clone(),
            AlignmentSide::Ts if alias.ts == rel.raw_name => return alias.ts.clone(),
            _ => {}
        }
    }
    match global.case_rule {
        CaseRule::Exact => rel.raw_name.clone(),
        CaseRule::SnakeCamel => snake_to_camel(&rel.raw_name),
    }
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p graphify-core contract::`
Expected: 15 total passing (prior 12 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-core/src/contract.rs
git commit -m "feat(core): relation alignment and cardinality comparison (FEAT-016)"
```

---

## Task 5: Deterministic ordering

**Files:**
- Modify: `crates/graphify-core/src/contract.rs`

- [ ] **Step 1: Write failing test**

Append to `tests`:

```rust
    #[test]
    fn violations_are_deterministically_ordered() {
        // Two identical runs should produce identical violation sequences.
        let orm = ContractBuilder::orm("user")
            .primitive("id", PrimitiveType::String, false)
            .primitive("name", PrimitiveType::String, false)
            .primitive("age", PrimitiveType::Number, false)
            .build();
        let ts = ContractBuilder::ts("user")
            .primitive("age", PrimitiveType::String, true) // type + nullability drift
            .primitive("email", PrimitiveType::String, false) // missing on orm
            .build();

        let cmp_a = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        let cmp_b = compare_contracts(&orm, &ts, &PairConfig::default(), &GlobalContractConfig::default());
        assert_eq!(cmp_a.violations, cmp_b.violations);
        // First violation must be the earliest-line issue on the ORM side.
        assert!(matches!(
            cmp_a.violations[0],
            ContractViolation::ContractFieldMissingOnTs { ref field, .. } if field == "id"
        ));
    }
```

- [ ] **Step 2: Run tests to verify they pass (alphabetical BTreeMap already gives determinism)**

Run: `cargo test -p graphify-core contract::violations_are_deterministically_ordered`
Expected: PASS (the `BTreeMap` used in `project_fields`/`project_relations` already yields deterministic iteration by key).

If it fails, add sorting at the end of `compare_contracts`:

```rust
violations.sort_by(|a, b| {
    fn sort_key(v: &ContractViolation) -> (usize, u8) {
        let (line, rank) = match v {
            ContractViolation::ContractFieldMissingOnTs { orm_line, .. } => (*orm_line, 0),
            ContractViolation::ContractFieldMissingOnOrm { ts_line, .. } => (*ts_line, 1),
            ContractViolation::ContractNullabilityMismatch { orm_line, .. } => (*orm_line, 2),
            ContractViolation::ContractTypeMismatch { orm_line, .. } => (*orm_line, 3),
            ContractViolation::ContractUnmappedOrmType { orm_line, .. } => (*orm_line, 4),
            ContractViolation::ContractRelationMissingOnTs { orm_line, .. } => (*orm_line, 5),
            ContractViolation::ContractRelationMissingOnOrm { ts_line, .. } => (*ts_line, 6),
            ContractViolation::ContractCardinalityMismatch { orm_line, .. } => (*orm_line, 7),
        };
        (line, rank)
    }
    sort_key(a).cmp(&sort_key(b))
});
```

- [ ] **Step 3: Run the full core test suite**

Run: `cargo test -p graphify-core`
Expected: all contract tests plus pre-existing core tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-core/src/contract.rs
git commit -m "feat(core): deterministic ordering for contract violations (FEAT-016)"
```

---

## Task 6: Drizzle parser — scalar tables

**Files:**
- Create: `crates/graphify-extract/src/drizzle.rs`
- Modify: `crates/graphify-extract/src/lib.rs` (add module + re-export)

- [ ] **Step 1: Write failing tests**

Create `crates/graphify-extract/src/drizzle.rs` with the test module skeleton:

```rust
use std::path::PathBuf;

use graphify_core::contract::{Cardinality, Contract, ContractSide, Field, FieldType, PrimitiveType, Relation};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, PartialEq)]
pub struct DrizzleParseError {
    pub message: String,
}

impl std::fmt::Display for DrizzleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for DrizzleParseError {}

pub fn extract_drizzle_contract(source: &str, table: &str) -> Result<Contract, DrizzleParseError> {
    extract_drizzle_contract_at(source, table, PathBuf::from("<inline>"))
}

pub fn extract_drizzle_contract_at(
    _source: &str,
    _table: &str,
    _source_file: PathBuf,
) -> Result<Contract, DrizzleParseError> {
    Err(DrizzleParseError {
        message: "not yet implemented".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::contract::PrimitiveType::*;

    fn assert_field(c: &Contract, name: &str, expected: FieldType, nullable: bool) {
        let f = c
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("missing field {name}"));
        assert_eq!(f.type_ref, expected, "wrong type for {name}");
        assert_eq!(f.nullable, nullable, "wrong nullability for {name}");
    }

    #[test]
    fn parses_scalar_pg_table() {
        let src = r#"
import { pgTable, text, integer, uuid, timestamp, boolean } from 'drizzle-orm/pg-core';
export const users = pgTable('users', {
  id:        uuid('id').primaryKey().defaultRandom(),
  email:     text('email').notNull(),
  age:       integer('age'),
  createdAt: timestamp('created_at').defaultNow().notNull(),
  active:    boolean('active').notNull(),
});
"#;
        let c = extract_drizzle_contract(src, "users").expect("parse ok");
        assert_eq!(c.side, ContractSide::Orm);
        assert_eq!(c.name, "users");
        assert_eq!(c.fields.len(), 5);
        assert_field(&c, "id",        FieldType::Primitive { value: String },  true);  // no .notNull()
        assert_field(&c, "email",     FieldType::Primitive { value: String },  false);
        assert_field(&c, "age",       FieldType::Primitive { value: Number },  true);
        assert_field(&c, "createdAt", FieldType::Primitive { value: Date },    false);
        assert_field(&c, "active",    FieldType::Primitive { value: Boolean }, false);
    }

    #[test]
    fn parses_sqlite_and_mysql_tables() {
        let sqlite = r#"
import { sqliteTable, text, integer } from 'drizzle-orm/sqlite-core';
export const todos = sqliteTable('todos', {
  id:   integer('id').primaryKey(),
  body: text('body').notNull(),
});
"#;
        let c = extract_drizzle_contract(sqlite, "todos").expect("sqlite ok");
        assert_eq!(c.fields.len(), 2);

        let mysql = r#"
import { mysqlTable, varchar, int } from 'drizzle-orm/mysql-core';
export const items = mysqlTable('items', {
  id:   int('id').primaryKey(),
  name: varchar('name', { length: 255 }).notNull(),
});
"#;
        // `int` is not in our default map → Unmapped. `varchar` is String.
        let c = extract_drizzle_contract(mysql, "items").expect("mysql ok");
        assert_field(&c, "name", FieldType::Primitive { value: String }, false);
        assert!(matches!(
            c.fields.iter().find(|f| f.name == "id").unwrap().type_ref,
            FieldType::Unmapped { .. }
        ));
    }

    #[test]
    fn unknown_type_is_unmapped() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core';
export const posts = pgTable('posts', {
  tags: tsvector('tags').notNull(),
});
"#;
        let c = extract_drizzle_contract(src, "posts").expect("ok");
        let f = c.fields.iter().find(|f| f.name == "tags").unwrap();
        assert!(matches!(&f.type_ref, FieldType::Unmapped { value } if value == "tsvector"));
    }
}
```

Modify `crates/graphify-extract/src/lib.rs` to add the module declaration and re-export:

```rust
pub mod cache;
pub mod drizzle;
pub mod go;
pub mod lang;
pub mod python;
pub mod resolver;
pub mod rust_lang;
pub mod ts_contract;
pub mod typescript;
pub mod walker;

pub use drizzle::{extract_drizzle_contract, extract_drizzle_contract_at, DrizzleParseError};
pub use go::GoExtractor;
pub use lang::{ExtractionResult, LanguageExtractor};
pub use python::PythonExtractor;
pub use rust_lang::RustExtractor;
pub use ts_contract::{extract_ts_contract, extract_ts_contract_at, TsContractParseError};
pub use typescript::TypeScriptExtractor;
pub use walker::{detect_local_prefix, discover_files, path_to_module, DiscoveredFile};
```

(`ts_contract.rs` and its exports are created in Task 9 — declare the module now with a temporary stub to keep the crate compiling, OR land both stubs in the same task. Do BOTH stubs now to keep lib.rs stable):

Create a minimal `crates/graphify-extract/src/ts_contract.rs` stub:

```rust
use std::path::PathBuf;

use graphify_core::contract::Contract;

#[derive(Debug, Clone, PartialEq)]
pub struct TsContractParseError {
    pub message: String,
}

impl std::fmt::Display for TsContractParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for TsContractParseError {}

pub fn extract_ts_contract(source: &str, export: &str) -> Result<Contract, TsContractParseError> {
    extract_ts_contract_at(source, export, PathBuf::from("<inline>"))
}

pub fn extract_ts_contract_at(
    _source: &str,
    _export: &str,
    _source_file: PathBuf,
) -> Result<Contract, TsContractParseError> {
    Err(TsContractParseError {
        message: "not yet implemented".into(),
    })
}
```

- [ ] **Step 2: Run tests to verify Drizzle parser tests fail**

Run: `cargo test -p graphify-extract drizzle::`
Expected: 3 failing tests (`"not yet implemented"` path).

- [ ] **Step 3: Implement the scalar-table extractor**

Replace the body of `extract_drizzle_contract_at` in `drizzle.rs` with a real implementation:

```rust
pub fn extract_drizzle_contract_at(
    source: &str,
    table: &str,
    source_file: PathBuf,
) -> Result<Contract, DrizzleParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|e| DrizzleParseError {
            message: format!("load TS grammar: {e}"),
        })?;

    let tree = parser.parse(source, None).ok_or_else(|| DrizzleParseError {
        message: "TS parse returned None".into(),
    })?;

    let bytes = source.as_bytes();
    let mut found: Option<(Node<'_>, &str)> = None;

    // Walk every `export_statement` → `lexical_declaration` → variable_declarator
    // whose value is a call to one of the known table constructors.
    walk_table_bindings(tree.root_node(), bytes, &mut |decl_name, call_node, call_name| {
        if call_name.ends_with("Table") || is_schema_table_chain(call_node, bytes) {
            if first_string_arg(call_node, bytes).as_deref() == Some(table) {
                found = Some((call_node, decl_name));
            }
        }
    });

    let Some((call_node, _decl_name)) = found else {
        return Err(DrizzleParseError {
            message: format!("table '{table}' not found in source"),
        });
    };

    let cols_node = call_node
        .child_by_field_name("arguments")
        .and_then(|args| nth_argument(args, 1, bytes))
        .ok_or_else(|| DrizzleParseError {
            message: "table call is missing a columns object literal".into(),
        })?;

    let fields = parse_columns_object(cols_node, bytes)?;

    let table_line = call_node.start_position().row + 1;

    Ok(Contract {
        name: table.to_string(),
        side: ContractSide::Orm,
        source_file,
        source_symbol: table.to_string(),
        fields,
        relations: Vec::new(),
        // NOTE: `table_line` not stored on Contract; pair-level line is tracked in CLI output layer.
    })
}

fn walk_table_bindings<'a, F>(node: Node<'a>, bytes: &'a [u8], on_match: &mut F)
where
    F: FnMut(&'a str, Node<'a>, &'a str),
{
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "export_statement" || child.kind() == "lexical_declaration" {
            for decl in child.children(&mut child.walk()) {
                if decl.kind() == "variable_declarator" {
                    if let (Some(name_node), Some(value_node)) = (
                        decl.child_by_field_name("name"),
                        decl.child_by_field_name("value"),
                    ) {
                        if value_node.kind() == "call_expression" {
                            if let Some((callee, callee_name)) = callee_name_of(value_node, bytes) {
                                let _ = callee; // retained for potential future use
                                let decl_name = text_of(name_node, bytes);
                                on_match(decl_name, value_node, callee_name);
                            }
                        }
                    }
                }
            }
        }
        walk_table_bindings(child, bytes, on_match);
    }
}

fn callee_name_of<'a>(call: Node<'a>, bytes: &'a [u8]) -> Option<(Node<'a>, &'a str)> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some((func, text_of(func, bytes))),
        "member_expression" => {
            let property = func.child_by_field_name("property")?;
            Some((property, text_of(property, bytes)))
        }
        _ => None,
    }
}

fn is_schema_table_chain(call: Node<'_>, bytes: &[u8]) -> bool {
    // Matches `pgSchema('auth').table(...)` — the call's `function` is a
    // member_expression whose `.object` is itself a call_expression named `pgSchema`.
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let property = func
        .child_by_field_name("property")
        .map(|p| text_of(p, bytes));
    if property.as_deref() != Some("table") {
        return false;
    }
    let Some(object) = func.child_by_field_name("object") else {
        return false;
    };
    if object.kind() != "call_expression" {
        return false;
    }
    callee_name_of(object, bytes)
        .map(|(_, n)| n == "pgSchema" || n == "mysqlSchema" || n == "sqliteSchema")
        .unwrap_or(false)
}

fn nth_argument<'a>(args: Node<'a>, n: usize, _bytes: &'a [u8]) -> Option<Node<'a>> {
    let mut cursor = args.walk();
    let mut idx = 0;
    for child in args.named_children(&mut cursor) {
        if idx == n {
            return Some(child);
        }
        idx += 1;
    }
    None
}

fn first_string_arg(call: Node<'_>, bytes: &[u8]) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() == "string" {
            return Some(string_literal_value(child, bytes));
        }
    }
    None
}

fn string_literal_value(node: Node<'_>, bytes: &[u8]) -> String {
    let raw = text_of(node, bytes);
    raw.trim_matches(|c| c == '\'' || c == '"' || c == '`').to_string()
}

fn text_of<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

fn parse_columns_object(obj: Node<'_>, bytes: &[u8]) -> Result<Vec<Field>, DrizzleParseError> {
    if obj.kind() != "object" {
        return Err(DrizzleParseError {
            message: format!("expected object literal, got {}", obj.kind()),
        });
    }
    let mut fields = Vec::new();
    let mut cursor = obj.walk();
    for pair in obj.named_children(&mut cursor) {
        if pair.kind() != "pair" {
            // spread_element and others — skip with a best-effort warning in stderr
            if pair.kind() == "spread_element" {
                eprintln!("drizzle: spread in column object is not expanded in v1");
            }
            continue;
        }
        let key_node = pair
            .child_by_field_name("key")
            .ok_or_else(|| DrizzleParseError { message: "pair missing key".into() })?;
        let value_node = pair
            .child_by_field_name("value")
            .ok_or_else(|| DrizzleParseError { message: "pair missing value".into() })?;
        let raw_name = property_key_text(key_node, bytes).to_string();
        let (type_ref, nullable, has_default) = interpret_column_chain(value_node, bytes);
        let line = pair.start_position().row + 1;
        fields.push(Field {
            name: raw_name.clone(),
            raw_name,
            type_ref,
            nullable,
            has_default,
            line,
        });
    }
    Ok(fields)
}

fn property_key_text<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    match node.kind() {
        "property_identifier" | "identifier" => text_of(node, bytes),
        "string" => {
            let raw = text_of(node, bytes);
            raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
        }
        _ => text_of(node, bytes),
    }
}

fn interpret_column_chain(value: Node<'_>, bytes: &[u8]) -> (FieldType, bool, bool) {
    // Walk chain leftwards collecting method names; the leftmost call is the builder.
    let mut chain_calls: Vec<&str> = Vec::new();
    let mut current = value;
    let mut root_builder: Option<Node<'_>> = None;

    loop {
        match current.kind() {
            "call_expression" => {
                let Some((callee_node, callee_name)) = callee_name_of(current, bytes) else {
                    break;
                };
                let Some(callee_func) = current.child_by_field_name("function") else {
                    break;
                };
                if callee_func.kind() == "member_expression" {
                    chain_calls.push(callee_name);
                    let Some(receiver) = callee_func.child_by_field_name("object") else {
                        break;
                    };
                    current = receiver;
                } else {
                    root_builder = Some(callee_node);
                    break;
                }
            }
            _ => break,
        }
    }

    let builder_name = root_builder.map(|n| text_of(n, bytes)).unwrap_or("");
    let nullable = !chain_calls.iter().any(|c| *c == "notNull");
    let has_default = chain_calls
        .iter()
        .any(|c| c.starts_with("default") || *c == "$default");

    // `.$type<Foo>()` — if present, override the type to Named("Foo").
    let dollar_type = chain_calls.iter().find(|c| **c == "$type").is_some();

    let primitive = match builder_name {
        "text" | "varchar" | "char" | "uuid" => Some(PrimitiveType::String),
        "integer" | "serial" | "bigserial" | "smallint" | "real" | "double_precision"
        | "numeric" | "decimal" => Some(PrimitiveType::Number),
        "boolean" => Some(PrimitiveType::Boolean),
        "timestamp" | "date" | "time" => Some(PrimitiveType::Date),
        "json" | "jsonb" => Some(PrimitiveType::Unknown),
        _ => None,
    };

    let type_ref = match (primitive, dollar_type) {
        (_, true) => {
            // Extract the generic argument text from the chain by scanning value source.
            // For v1, store the name heuristically: parse from the full source range.
            FieldType::Named {
                value: extract_dollar_type_arg(value, bytes).unwrap_or_else(|| "unknown".into()),
            }
        }
        (Some(PrimitiveType::Unknown), false) => FieldType::Primitive {
            value: PrimitiveType::Unknown,
        },
        (Some(p), false) => FieldType::Primitive { value: p },
        (None, false) => FieldType::Unmapped {
            value: builder_name.to_string(),
        },
    };

    (type_ref, nullable, has_default)
}

fn extract_dollar_type_arg(value: Node<'_>, bytes: &[u8]) -> Option<String> {
    // Search textually: `.$type<Foo>()` — simpler than threading through the AST
    // because tree-sitter-typescript splits type arguments into `type_arguments` nodes
    // only in a .ts grammar and not reliably for all chain forms.
    let src = text_of(value, bytes);
    let start = src.find("$type<")?;
    let after = &src[start + "$type<".len()..];
    let end = after.find('>')?;
    Some(after[..end].trim().to_string())
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p graphify-extract drizzle::`
Expected: 3 passing tests. If the `pg_table_line_number` path is exercised implicitly, also check `cargo test -p graphify-extract` for regressions.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/drizzle.rs crates/graphify-extract/src/ts_contract.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(extract): Drizzle parser for scalar tables (FEAT-016)"
```

---

## Task 7: Drizzle parser — `.$type<Foo>()`, `pgEnum`, `pgSchema` coverage

**Files:**
- Modify: `crates/graphify-extract/src/drizzle.rs`

- [ ] **Step 1: Write failing tests**

Append to `drizzle.rs` `tests` module:

```rust
    #[test]
    fn parses_dollar_type_override() {
        let src = r#"
import { pgTable, jsonb } from 'drizzle-orm/pg-core';
export const users = pgTable('users', {
  metadata: jsonb('metadata').$type<UserMetadata>().notNull(),
});
"#;
        let c = extract_drizzle_contract(src, "users").expect("ok");
        let f = c.fields.iter().find(|f| f.name == "metadata").unwrap();
        assert_eq!(
            f.type_ref,
            FieldType::Named {
                value: "UserMetadata".into()
            }
        );
        assert!(!f.nullable);
    }

    #[test]
    fn parses_pg_schema_chain() {
        let src = r#"
import { pgSchema, uuid, text } from 'drizzle-orm/pg-core';
const auth = pgSchema('auth');
export const users = auth.table('users', {
  id:   uuid('id').primaryKey(),
  name: text('name').notNull(),
});
"#;
        let c = extract_drizzle_contract(src, "users").expect("ok");
        assert_eq!(c.fields.len(), 2);
    }

    #[test]
    fn multi_table_file_picks_by_name() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core';
export const users = pgTable('users', { name: text('name').notNull() });
export const posts = pgTable('posts', { title: text('title').notNull() });
"#;
        let c = extract_drizzle_contract(src, "posts").expect("ok");
        assert_eq!(c.fields.len(), 1);
        assert_eq!(c.fields[0].name, "title");
    }
```

- [ ] **Step 2: Run tests — expect them to pass because Task 6's implementation already handles these cases**

Run: `cargo test -p graphify-extract drizzle::`
Expected: all prior tests plus 3 new = 6 passing.

If any fails, likely cause: `is_schema_table_chain` does not walk through a variable (`const auth = pgSchema('auth'); auth.table(...)`) — Task 6's function matches only when the receiver is a direct `pgSchema(...)` call. Fix by also accepting the case where the receiver is an identifier bound to a `pgSchema(...)` call earlier in the file. Add a second-pass resolver:

```rust
fn is_schema_table_chain(call: Node<'_>, bytes: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let property = func
        .child_by_field_name("property")
        .map(|p| text_of(p, bytes));
    if property.as_deref() != Some("table") {
        return false;
    }
    let Some(object) = func.child_by_field_name("object") else {
        return false;
    };
    match object.kind() {
        "call_expression" => callee_name_of(object, bytes)
            .map(|(_, n)| n == "pgSchema" || n == "mysqlSchema" || n == "sqliteSchema")
            .unwrap_or(false),
        "identifier" => {
            // Accept any identifier here — false-positives are harmless because the
            // caller still filters by table-name string match.
            true
        }
        _ => false,
    }
}
```

- [ ] **Step 3: Re-run tests**

Run: `cargo test -p graphify-extract drizzle::`
Expected: 6 passing.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-extract/src/drizzle.rs
git commit -m "feat(extract): Drizzle \$type<>, pgSchema, multi-table file (FEAT-016)"
```

---

## Task 8: Drizzle parser — relations

**Files:**
- Modify: `crates/graphify-extract/src/drizzle.rs`

- [ ] **Step 1: Write failing tests**

Append to `drizzle.rs` `tests` module:

```rust
    #[test]
    fn parses_relations_block() {
        let src = r#"
import { pgTable, uuid } from 'drizzle-orm/pg-core';
import { relations } from 'drizzle-orm';
export const users = pgTable('users', {
  id: uuid('id').primaryKey(),
});
export const usersRelations = relations(users, ({ one, many }) => ({
  profile: one(profiles, { fields: [users.profileId], references: [profiles.id] }),
  posts:   many(posts),
}));
"#;
        let c = extract_drizzle_contract(src, "users").expect("ok");
        assert_eq!(c.relations.len(), 2);
        let profile = c.relations.iter().find(|r| r.name == "profile").unwrap();
        assert_eq!(profile.cardinality, Cardinality::One);
        assert_eq!(profile.target_contract, "profiles");
        assert!(profile.nullable); // conservative default in v1
        let posts = c.relations.iter().find(|r| r.name == "posts").unwrap();
        assert_eq!(posts.cardinality, Cardinality::Many);
        assert_eq!(posts.target_contract, "posts");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-extract drizzle::parses_relations_block`
Expected: FAIL — `c.relations.len()` is still 0.

- [ ] **Step 3: Parse `relations()` blocks**

In `drizzle.rs`, update `extract_drizzle_contract_at` to also scan for `relations(X, (...) => ({...}))` calls whose first identifier argument matches the declared table variable name. Add the scan after the columns pass (before returning `Contract`):

Replace the `Ok(Contract { ... })` block at the end of `extract_drizzle_contract_at` with:

```rust
    let decl_name_for_table: Option<String> = resolve_declared_name_for_call(tree.root_node(), bytes, call_node);

    let mut relations = Vec::new();
    if let Some(var_name) = decl_name_for_table.as_deref() {
        scan_relations_block(tree.root_node(), bytes, var_name, &mut relations);
    }

    Ok(Contract {
        name: table.to_string(),
        side: ContractSide::Orm,
        source_file,
        source_symbol: table.to_string(),
        fields,
        relations,
    })
}

fn resolve_declared_name_for_call(
    root: Node<'_>,
    bytes: &[u8],
    target_call: Node<'_>,
) -> Option<String> {
    let mut found = None;
    let target_range = target_call.byte_range();
    walk_table_bindings(root, bytes, &mut |decl_name, call_node, _call_name| {
        if call_node.byte_range() == target_range {
            found = Some(decl_name.to_string());
        }
    });
    found
}

fn scan_relations_block(root: Node<'_>, bytes: &[u8], table_var: &str, out: &mut Vec<Relation>) {
    walk_calls(root, bytes, &mut |call| {
        let Some((_, name)) = callee_name_of(call, bytes) else {
            return;
        };
        if name != "relations" {
            return;
        }
        let Some(args) = call.child_by_field_name("arguments") else {
            return;
        };
        let Some(first) = nth_argument(args, 0, bytes) else {
            return;
        };
        if first.kind() != "identifier" || text_of(first, bytes) != table_var {
            return;
        }
        let Some(second) = nth_argument(args, 1, bytes) else {
            return;
        };
        // Arrow function: the body is the object returned.
        let body = find_arrow_body_object(second);
        if let Some(obj) = body {
            let mut cursor = obj.walk();
            for pair in obj.named_children(&mut cursor) {
                if pair.kind() != "pair" {
                    continue;
                }
                let Some(key_node) = pair.child_by_field_name("key") else {
                    continue;
                };
                let Some(value_node) = pair.child_by_field_name("value") else {
                    continue;
                };
                if value_node.kind() != "call_expression" {
                    continue;
                }
                let Some((_, callee)) = callee_name_of(value_node, bytes) else {
                    continue;
                };
                let cardinality = match callee {
                    "one" => Cardinality::One,
                    "many" => Cardinality::Many,
                    _ => continue,
                };
                let rel_name = property_key_text(key_node, bytes).to_string();
                let target = first_identifier_arg(value_node, bytes).unwrap_or_default();
                let line = pair.start_position().row + 1;
                out.push(Relation {
                    name: rel_name.clone(),
                    raw_name: rel_name,
                    cardinality,
                    target_contract: target,
                    nullable: true,
                    line,
                });
            }
        }
    });
}

fn walk_calls<'a, F>(node: Node<'a>, bytes: &'a [u8], on_call: &mut F)
where
    F: FnMut(Node<'a>),
{
    if node.kind() == "call_expression" {
        on_call(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_calls(child, bytes, on_call);
    }
}

fn find_arrow_body_object<'a>(node: Node<'a>) -> Option<Node<'a>> {
    if node.kind() != "arrow_function" {
        return None;
    }
    let body = node.child_by_field_name("body")?;
    match body.kind() {
        "object" => Some(body),
        "parenthesized_expression" => {
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                if child.kind() == "object" {
                    return Some(child);
                }
            }
            None
        }
        "statement_block" => {
            // `return { ... }` form
            let mut cursor = body.walk();
            for child in body.named_children(&mut cursor) {
                if child.kind() == "return_statement" {
                    let mut rc = child.walk();
                    for grand in child.named_children(&mut rc) {
                        if grand.kind() == "object" {
                            return Some(grand);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn first_identifier_arg(call: Node<'_>, bytes: &[u8]) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(text_of(child, bytes).to_string());
        }
    }
    None
}
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p graphify-extract drizzle::`
Expected: 7 passing.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/drizzle.rs
git commit -m "feat(extract): Drizzle relations() block parser (FEAT-016)"
```

---

## Task 9: TS contract parser — interfaces and type aliases

**Files:**
- Modify: `crates/graphify-extract/src/ts_contract.rs` (replace stub with real implementation)

- [ ] **Step 1: Write failing tests**

Replace the stub in `crates/graphify-extract/src/ts_contract.rs` with the full module skeleton that includes tests:

```rust
use std::path::PathBuf;

use graphify_core::contract::{
    Cardinality, Contract, ContractSide, Field, FieldType, PrimitiveType, Relation,
};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, PartialEq)]
pub struct TsContractParseError {
    pub message: String,
}

impl std::fmt::Display for TsContractParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for TsContractParseError {}

pub fn extract_ts_contract(source: &str, export: &str) -> Result<Contract, TsContractParseError> {
    extract_ts_contract_at(source, export, PathBuf::from("<inline>"))
}

pub fn extract_ts_contract_at(
    source: &str,
    export: &str,
    source_file: PathBuf,
) -> Result<Contract, TsContractParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|e| TsContractParseError {
            message: format!("load TS grammar: {e}"),
        })?;
    let tree = parser.parse(source, None).ok_or_else(|| TsContractParseError {
        message: "TS parse returned None".into(),
    })?;
    let bytes = source.as_bytes();

    let target = find_declaration(tree.root_node(), bytes, export).ok_or_else(|| {
        TsContractParseError {
            message: format!("export '{export}' not found"),
        }
    })?;

    let (fields, relations) = match target.kind() {
        "interface_declaration" => parse_interface(target, bytes)?,
        "type_alias_declaration" => parse_type_alias(target, bytes)?,
        other => {
            return Err(TsContractParseError {
                message: format!("unsupported declaration kind: {other}"),
            })
        }
    };

    Ok(Contract {
        name: export.to_string(),
        side: ContractSide::Ts,
        source_file,
        source_symbol: export.to_string(),
        fields,
        relations,
    })
}

fn find_declaration<'a>(root: Node<'a>, bytes: &'a [u8], export: &str) -> Option<Node<'a>> {
    let mut found = None;
    walk_declarations(root, bytes, &mut |kind, name, node| {
        if name == export && (kind == "interface_declaration" || kind == "type_alias_declaration") && found.is_none() {
            found = Some(node);
        }
    });
    found
}

fn walk_declarations<'a, F>(node: Node<'a>, bytes: &'a [u8], on_decl: &mut F)
where
    F: FnMut(&str, &str, Node<'a>),
{
    match node.kind() {
        "interface_declaration" | "type_alias_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                on_decl(node.kind(), text_of(name_node, bytes), node);
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_declarations(child, bytes, on_decl);
    }
}

fn text_of<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("")
}

fn parse_interface(
    node: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    let body = node.child_by_field_name("body").ok_or_else(|| TsContractParseError {
        message: "interface missing body".into(),
    })?;
    parse_members(body, bytes)
}

fn parse_type_alias(
    node: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    let value = node.child_by_field_name("value").ok_or_else(|| TsContractParseError {
        message: "type alias missing value".into(),
    })?;
    let target = match value.kind() {
        "object_type" => value,
        "intersection_type" => {
            // flatten members; see Task 11.
            return parse_intersection(value, bytes);
        }
        other => {
            return Err(TsContractParseError {
                message: format!("unsupported type alias value: {other}"),
            })
        }
    };
    parse_members(target, bytes)
}

fn parse_members(
    body: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    let mut fields = Vec::new();
    let relations: Vec<Relation> = Vec::new(); // relations classification done in Task 10

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "property_signature" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else {
            continue;
        };
        let raw_name = property_name_text(name_node, bytes).to_string();
        let optional = member
            .children(&mut member.walk())
            .any(|n| n.kind() == "?");
        let type_node = member.child_by_field_name("type");
        let (type_ref, mut nullable) = match type_node {
            Some(t) => resolve_type_annotation(t, bytes),
            None => (
                FieldType::Primitive {
                    value: PrimitiveType::Unknown,
                },
                false,
            ),
        };
        if optional {
            nullable = true;
        }
        let line = member.start_position().row + 1;
        fields.push(Field {
            name: raw_name.clone(),
            raw_name,
            type_ref,
            nullable,
            has_default: false,
            line,
        });
    }

    Ok((fields, relations))
}

fn parse_intersection(
    node: Node<'_>,
    bytes: &[u8],
) -> Result<(Vec<Field>, Vec<Relation>), TsContractParseError> {
    // Covered in Task 11.
    let mut fields = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "object_type" {
            let (members, _) = parse_members(child, bytes)?;
            for f in members {
                if let Some(existing) = fields.iter_mut().find(|x: &&mut Field| x.name == f.name) {
                    *existing = f;
                } else {
                    fields.push(f);
                }
            }
        } else {
            eprintln!(
                "ts_contract: intersection with non-inline type '{}' ignored in v1",
                child.kind()
            );
        }
    }
    Ok((fields, Vec::new()))
}

fn property_name_text<'a>(node: Node<'_>, bytes: &'a [u8]) -> &'a str {
    match node.kind() {
        "property_identifier" | "identifier" => text_of(node, bytes),
        "string" => text_of(node, bytes)
            .trim_matches(|c: char| c == '\'' || c == '"' || c == '`'),
        _ => text_of(node, bytes),
    }
}

/// Resolve a `type_annotation` or direct type node to a (FieldType, nullable) pair.
fn resolve_type_annotation(ann: Node<'_>, bytes: &[u8]) -> (FieldType, bool) {
    // A type_annotation wraps the actual type as its first named child.
    let inner = if ann.kind() == "type_annotation" {
        let mut cursor = ann.walk();
        ann.named_children(&mut cursor)
            .next()
            .unwrap_or(ann)
    } else {
        ann
    };
    resolve_type(inner, bytes)
}

fn resolve_type(node: Node<'_>, bytes: &[u8]) -> (FieldType, bool) {
    match node.kind() {
        "predefined_type" => match text_of(node, bytes) {
            "string" => (prim(PrimitiveType::String), false),
            "number" | "bigint" => (prim(PrimitiveType::Number), false),
            "boolean" => (prim(PrimitiveType::Boolean), false),
            "unknown" | "any" | "never" | "void" => (prim(PrimitiveType::Unknown), false),
            other => (
                FieldType::Named {
                    value: other.to_string(),
                },
                false,
            ),
        },
        "type_identifier" => {
            let name = text_of(node, bytes);
            if name == "Date" {
                (prim(PrimitiveType::Date), false)
            } else {
                (
                    FieldType::Named {
                        value: name.to_string(),
                    },
                    false,
                )
            }
        }
        "literal_type" => {
            let src = text_of(node, bytes).trim();
            if src == "null" || src == "undefined" {
                (prim(PrimitiveType::Unknown), true)
            } else {
                (
                    FieldType::Named {
                        value: src.to_string(),
                    },
                    false,
                )
            }
        }
        "null" | "undefined" => (prim(PrimitiveType::Unknown), true),
        "union_type" => resolve_union(node, bytes),
        "array_type" => {
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            let (inner_ty, inner_nullable) = inner
                .map(|n| resolve_type(n, bytes))
                .unwrap_or((prim(PrimitiveType::Unknown), false));
            (
                FieldType::Array {
                    value: Box::new(inner_ty),
                },
                inner_nullable,
            )
        }
        "generic_type" => {
            // Handle `Array<T>` explicitly.
            let name_node = node.child_by_field_name("name");
            let name = name_node.map(|n| text_of(n, bytes)).unwrap_or("");
            if name == "Array" {
                let args = node.child_by_field_name("type_arguments");
                let mut cursor = args.map(|a| a.walk()).unwrap_or_else(|| node.walk());
                let inner = args.and_then(|a| a.named_children(&mut cursor).next());
                let (inner_ty, _) = inner
                    .map(|n| resolve_type(n, bytes))
                    .unwrap_or((prim(PrimitiveType::Unknown), false));
                (
                    FieldType::Array {
                        value: Box::new(inner_ty),
                    },
                    false,
                )
            } else {
                (
                    FieldType::Named {
                        value: name.to_string(),
                    },
                    false,
                )
            }
        }
        "parenthesized_type" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .next()
                .map(|n| resolve_type(n, bytes))
                .unwrap_or((prim(PrimitiveType::Unknown), false))
        }
        _ => (
            FieldType::Named {
                value: text_of(node, bytes).to_string(),
            },
            false,
        ),
    }
}

fn resolve_union(node: Node<'_>, bytes: &[u8]) -> (FieldType, bool) {
    let mut parts: Vec<FieldType> = Vec::new();
    let mut nullable = false;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let (t, n) = resolve_type(child, bytes);
        if n {
            nullable = true;
            continue;
        }
        match &t {
            FieldType::Primitive {
                value: PrimitiveType::Unknown,
            } => {
                // null/undefined literals are represented as Unknown + nullable;
                // once we've already extracted nullable=true we skip them from the union.
                continue;
            }
            _ => parts.push(t),
        }
    }
    let ty = if parts.len() == 1 {
        parts.pop().unwrap()
    } else if parts.is_empty() {
        FieldType::Primitive {
            value: PrimitiveType::Unknown,
        }
    } else {
        FieldType::Union { value: parts }
    };
    (ty, nullable)
}

fn prim(p: PrimitiveType) -> FieldType {
    FieldType::Primitive { value: p }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interface_primitive_fields() {
        let src = r#"
export interface UserDto {
  id: string;
  email: string;
  age: number | null;
  active: boolean;
  createdAt: Date;
}
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        assert_eq!(c.side, ContractSide::Ts);
        assert_eq!(c.fields.len(), 5);
        assert!(matches!(
            c.fields.iter().find(|f| f.name == "id").unwrap().type_ref,
            FieldType::Primitive { value: PrimitiveType::String }
        ));
        let age = c.fields.iter().find(|f| f.name == "age").unwrap();
        assert!(age.nullable);
        let created = c.fields.iter().find(|f| f.name == "createdAt").unwrap();
        assert!(matches!(created.type_ref, FieldType::Primitive { value: PrimitiveType::Date }));
    }

    #[test]
    fn parses_type_alias_object() {
        let src = r#"
export type UserDto = {
  id: string;
  nickname?: string;
};
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        assert_eq!(c.fields.len(), 2);
        let nick = c.fields.iter().find(|f| f.name == "nickname").unwrap();
        assert!(nick.nullable);
    }

    #[test]
    fn parses_array_and_generic_array() {
        let src = r#"
export interface UserDto {
  names: string[];
  tags: Array<string>;
}
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        let names = c.fields.iter().find(|f| f.name == "names").unwrap();
        assert!(matches!(&names.type_ref, FieldType::Array { value } if matches!(**value, FieldType::Primitive { value: PrimitiveType::String })));
        let tags = c.fields.iter().find(|f| f.name == "tags").unwrap();
        assert!(matches!(&tags.type_ref, FieldType::Array { value } if matches!(**value, FieldType::Primitive { value: PrimitiveType::String })));
    }

    #[test]
    fn collapses_null_and_undefined_into_nullable() {
        let src = r#"
export interface UserDto {
  a: string | null;
  b: string | undefined;
  c?: string;
}
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        for f in &c.fields {
            assert!(f.nullable, "{} expected nullable", f.name);
            assert!(matches!(f.type_ref, FieldType::Primitive { value: PrimitiveType::String }));
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p graphify-extract ts_contract::`
Expected: 4 passing tests.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-extract/src/ts_contract.rs
git commit -m "feat(extract): TS interface and type alias parser (FEAT-016)"
```

---

## Task 10: TS relation classification sweep

**Files:**
- Modify: `crates/graphify-extract/src/ts_contract.rs`

- [ ] **Step 1: Write failing tests**

Append to the `tests` module:

```rust
    #[test]
    fn classifies_single_and_many_relations() {
        let src = r#"
export interface ProfileDto { id: string }
export interface PostDto { id: string }
export interface UserDto {
  id: string;
  profile?: ProfileDto;
  posts: PostDto[];
}
"#;
        // One-shot helper: parse all three, then reclassify UserDto.
        let contracts = parse_all_ts_contracts(src, &["UserDto", "ProfileDto", "PostDto"]).expect("ok");
        let user = contracts.iter().find(|c| c.name == "UserDto").unwrap();
        assert_eq!(user.relations.len(), 2);
        let profile = user.relations.iter().find(|r| r.name == "profile").unwrap();
        assert_eq!(profile.cardinality, Cardinality::One);
        assert!(profile.nullable);
        let posts = user.relations.iter().find(|r| r.name == "posts").unwrap();
        assert_eq!(posts.cardinality, Cardinality::Many);
        // Scalar-only fields stay in fields[].
        assert_eq!(user.fields.len(), 1);
        assert_eq!(user.fields[0].name, "id");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p graphify-extract ts_contract::classifies_single_and_many_relations`
Expected: unresolved `parse_all_ts_contracts`.

- [ ] **Step 3: Add the classification sweep**

Add to `ts_contract.rs` a public multi-export helper:

```rust
/// Parse multiple TS contracts from a single source and reclassify fields
/// that reference other known contracts as relations.
pub fn parse_all_ts_contracts(
    source: &str,
    exports: &[&str],
) -> Result<Vec<Contract>, TsContractParseError> {
    parse_all_ts_contracts_at(source, exports, PathBuf::from("<inline>"))
}

pub fn parse_all_ts_contracts_at(
    source: &str,
    exports: &[&str],
    source_file: PathBuf,
) -> Result<Vec<Contract>, TsContractParseError> {
    let mut contracts = Vec::with_capacity(exports.len());
    for export in exports {
        contracts.push(extract_ts_contract_at(source, export, source_file.clone())?);
    }
    let known: std::collections::HashSet<String> =
        contracts.iter().map(|c| c.name.clone()).collect();
    for c in &mut contracts {
        classify_relations(c, &known);
    }
    Ok(contracts)
}

fn classify_relations(contract: &mut Contract, known: &std::collections::HashSet<String>) {
    let mut i = 0;
    while i < contract.fields.len() {
        let f = &contract.fields[i];
        let relation = match &f.type_ref {
            FieldType::Named { value } if known.contains(value) => {
                Some((value.clone(), Cardinality::One))
            }
            FieldType::Array { value } => match value.as_ref() {
                FieldType::Named { value: inner } if known.contains(inner) => {
                    Some((inner.clone(), Cardinality::Many))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((target, cardinality)) = relation {
            let f = contract.fields.remove(i);
            contract.relations.push(Relation {
                name: f.name,
                raw_name: f.raw_name,
                cardinality,
                target_contract: target,
                nullable: f.nullable,
                line: f.line,
            });
        } else {
            i += 1;
        }
    }
}
```

And re-export from `crates/graphify-extract/src/lib.rs`:

```rust
pub use ts_contract::{
    extract_ts_contract, extract_ts_contract_at, parse_all_ts_contracts,
    parse_all_ts_contracts_at, TsContractParseError,
};
```

- [ ] **Step 4: Run tests and verify they pass**

Run: `cargo test -p graphify-extract ts_contract::`
Expected: 5 passing.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-extract/src/ts_contract.rs crates/graphify-extract/src/lib.rs
git commit -m "feat(extract): TS scalar vs relation classification (FEAT-016)"
```

---

## Task 11: TS intersection flattening test

**Files:**
- Modify: `crates/graphify-extract/src/ts_contract.rs` (add coverage only)

- [ ] **Step 1: Write failing test**

Append to the `tests` module:

```rust
    #[test]
    fn flattens_inline_intersections() {
        let src = r#"
export type UserDto = {
  id: string;
} & {
  email: string;
};
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        assert_eq!(c.fields.len(), 2);
        assert!(c.fields.iter().any(|f| f.name == "id"));
        assert!(c.fields.iter().any(|f| f.name == "email"));
    }

    #[test]
    fn intersection_later_inline_object_wins_on_conflict() {
        let src = r#"
export type UserDto = {
  id: string;
} & {
  id: number;
};
"#;
        let c = extract_ts_contract(src, "UserDto").expect("ok");
        let id = c.fields.iter().find(|f| f.name == "id").unwrap();
        assert!(matches!(id.type_ref, FieldType::Primitive { value: PrimitiveType::Number }));
    }
```

- [ ] **Step 2: Run tests to verify they pass (Task 9's `parse_intersection` should already handle this)**

Run: `cargo test -p graphify-extract ts_contract::`
Expected: 7 passing (5 prior + 2 new).

If either fails, debug `parse_intersection` — the iteration order of `named_children` must match source order (tree-sitter guarantees this).

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-extract/src/ts_contract.rs
git commit -m "test(extract): coverage for inline TS intersections (FEAT-016)"
```

---

## Task 12: Report output — JSON serializer

**Files:**
- Create: `crates/graphify-report/src/contract_json.rs`
- Modify: `crates/graphify-report/src/lib.rs`
- Modify: `crates/graphify-report/Cargo.toml` (add `graphify-core` dep if absent)

- [ ] **Step 1: Verify `graphify-core` is a dependency of `graphify-report`**

Run: `grep graphify-core crates/graphify-report/Cargo.toml`
If absent, add to `[dependencies]` in `crates/graphify-report/Cargo.toml`:

```toml
graphify-core = { path = "../graphify-core" }
```

- [ ] **Step 2: Write failing tests**

Create `crates/graphify-report/src/contract_json.rs`:

```rust
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
```

Modify `crates/graphify-report/src/lib.rs` to add the module and re-exports:

```rust
pub mod contract_json;
pub mod contract_markdown;
pub mod csv;
pub mod diff_json;
pub mod diff_markdown;
pub mod graphml;
pub mod html;
pub mod json;
pub mod markdown;
pub mod neo4j;
pub mod obsidian;
pub mod trend_json;
pub mod trend_markdown;

pub use contract_json::{
    build_contract_check_result, ContractCheckResult, ContractPairResult, ContractSideInfo,
    ViolationEntry,
};
pub use contract_markdown::write_contract_markdown_section;
pub use csv::{write_edges_csv, write_nodes_csv};
pub use diff_json::write_diff_json;
pub use diff_markdown::write_diff_markdown;
pub use graphml::write_graphml;
pub use html::write_html;
pub use json::{write_analysis_json, write_graph_json};
pub use markdown::write_report;
pub use neo4j::write_cypher;
pub use obsidian::write_obsidian_vault;
pub use trend_json::write_trend_json;
pub use trend_markdown::write_trend_markdown;

pub use graphify_core::community::Community;

pub type Cycle = Vec<String>;
```

`contract_markdown.rs` is created in Task 13 — declare the module after creating a stub file there. Add a minimal stub to keep the crate compiling now. Create `crates/graphify-report/src/contract_markdown.rs`:

```rust
use crate::contract_json::ContractCheckResult;

pub fn write_contract_markdown_section(_result: &ContractCheckResult) -> String {
    String::new()
}
```

- [ ] **Step 3: Run tests and verify they pass**

Run: `cargo test -p graphify-report contract_json::`
Expected: 2 passing.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-report/src/contract_json.rs crates/graphify-report/src/contract_markdown.rs crates/graphify-report/src/lib.rs crates/graphify-report/Cargo.toml
git commit -m "feat(report): contract check JSON schema (FEAT-016)"
```

---

## Task 13: Report output — Markdown section

**Files:**
- Modify: `crates/graphify-report/src/contract_markdown.rs`

- [ ] **Step 1: Write failing tests**

Replace the stub in `crates/graphify-report/src/contract_markdown.rs` with:

```rust
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
```

- [ ] **Step 2: Run tests and verify they pass**

Run: `cargo test -p graphify-report contract_markdown::`
Expected: 2 passing.

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-report/src/contract_markdown.rs
git commit -m "feat(report): contract drift Markdown section (FEAT-016)"
```

---

## Task 14: CLI wiring, config, integration tests

**Files:**
- Modify: `crates/graphify-cli/src/main.rs` (config, check flow, output, exit codes)
- Create: `tests/contract_integration.rs`
- Create: fixture directory `tests/fixtures/contract_drift/monorepo/...`

This is the integration task. Split into multiple steps so each is still 2–5 minutes of work.

- [ ] **Step 1: Extend `Config` with contract section**

In `crates/graphify-cli/src/main.rs`, find the `Config` struct (around line 38). Extend it:

```rust
use graphify_core::contract::{
    CaseRule, FieldAlias, FieldType, GlobalContractConfig, PairConfig, PrimitiveType, Severity,
};

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    project: Vec<ProjectConfig>,
    #[serde(default)]
    policy: PolicyConfig,
    #[serde(default)]
    contract: ContractConfigRaw,
}

#[derive(Deserialize, Default)]
struct ContractConfigRaw {
    #[serde(default)]
    type_map: std::collections::HashMap<String, String>,
    #[serde(default = "default_case_rule")]
    case_rule: String,
    #[serde(default = "default_unmapped_severity")]
    unmapped_type_severity: String,
    #[serde(default, rename = "pair")]
    pairs: Vec<PairConfigRaw>,
}

fn default_case_rule() -> String {
    "snake_camel".into()
}

fn default_unmapped_severity() -> String {
    "warning".into()
}

#[derive(Deserialize, Default)]
struct PairConfigRaw {
    name: String,
    orm: PairEndpointRaw,
    ts: PairEndpointRaw,
    #[serde(default)]
    field_alias: Vec<FieldAliasRaw>,
    #[serde(default)]
    relation_alias: Vec<FieldAliasRaw>,
    #[serde(default)]
    ignore: Option<IgnoreRaw>,
}

#[derive(Deserialize, Default, Clone)]
struct PairEndpointRaw {
    #[serde(default)]
    source: String, // "drizzle" (required for orm, ignored for ts)
    file: String,
    #[serde(default)]
    table: String,
    #[serde(default)]
    export: String,
}

#[derive(Deserialize, Default, Clone)]
struct FieldAliasRaw {
    orm: String,
    ts: String,
}

#[derive(Deserialize, Default, Clone)]
struct IgnoreRaw {
    #[serde(default)]
    orm: Vec<String>,
    #[serde(default)]
    ts: Vec<String>,
}
```

- [ ] **Step 2: Add `--contracts` / `--no-contracts` / `--contracts-warnings-as-errors` flags**

In `main.rs`, find `Commands::Check` (around line 157). Add three flags:

```rust
    Check {
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,
        #[arg(long)]
        max_cycles: Option<usize>,
        #[arg(long)]
        max_hotspot_score: Option<f64>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        force: bool,
        /// Run the contract drift gate (default: on if [[contract.pair]] is declared)
        #[arg(long, default_value_t = false, conflicts_with = "no_contracts")]
        contracts: bool,
        /// Skip the contract drift gate even when pairs are configured
        #[arg(long = "no-contracts", default_value_t = false)]
        no_contracts: bool,
        /// Treat contract warnings (UnmappedOrmType) as errors
        #[arg(long, default_value_t = false)]
        contracts_warnings_as_errors: bool,
    },
```

Wire the flags through to `cmd_check` — update both the `match` arm (around line 484) and the `cmd_check` signature (around line 1700):

```rust
        Commands::Check {
            config,
            max_cycles,
            max_hotspot_score,
            project,
            json,
            force,
            contracts,
            no_contracts,
            contracts_warnings_as_errors,
        } => {
            cmd_check(
                &config,
                project.as_deref(),
                force,
                CheckLimits { max_cycles, max_hotspot_score },
                json,
                ContractsMode::from_flags(contracts, no_contracts),
                contracts_warnings_as_errors,
            );
        }
```

Add the mode enum near `CheckLimits`:

```rust
#[derive(Debug, Clone, Copy)]
enum ContractsMode {
    Auto,
    On,
    Off,
}

impl ContractsMode {
    fn from_flags(contracts: bool, no_contracts: bool) -> Self {
        match (contracts, no_contracts) {
            (true, _) => ContractsMode::On,
            (_, true) => ContractsMode::Off,
            _ => ContractsMode::Auto,
        }
    }
}
```

- [ ] **Step 3: Extend `CheckReport` + wire contract extraction**

Extend `CheckReport` (around line 1509):

```rust
#[derive(Debug, Clone, Serialize)]
struct CheckReport {
    ok: bool,
    violations: usize,
    projects: Vec<ProjectCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contracts: Option<graphify_report::ContractCheckResult>,
}
```

Update `build_check_report` to take the optional contract result and include its error count in the total:

```rust
fn build_check_report(
    projects: Vec<ProjectCheckResult>,
    contracts: Option<graphify_report::ContractCheckResult>,
) -> CheckReport {
    let mut violations = projects.iter().map(|p| p.violations.len()).sum();
    if let Some(c) = &contracts {
        violations += c.error_count;
    }
    let ok_projects = projects.iter().all(|p| p.ok);
    let ok_contracts = contracts.as_ref().map(|c| c.ok).unwrap_or(true);
    CheckReport {
        ok: ok_projects && ok_contracts,
        violations,
        projects,
        contracts,
    }
}
```

Update `cmd_check` signature and body. Insert contract evaluation after policy evaluation and before `build_check_report`. Full replacement of `cmd_check`:

```rust
fn cmd_check(
    config_path: &Path,
    project_filter: Option<&str>,
    force: bool,
    limits: CheckLimits,
    json: bool,
    contracts_mode: ContractsMode,
    contracts_warnings_as_errors: bool,
) {
    let cfg = load_config(config_path);
    let projects = filter_projects(&cfg, project_filter);
    let mut analyzed_projects = Vec::new();

    for project in &projects {
        let (graph, _excludes, stats) = run_extract(project, &cfg.settings, None, force);
        print_cache_stats(&project.name, &stats);
        let (metrics, communities, cycles) = run_analyze(&graph, &ScoringWeights::default());
        analyzed_projects.push(CheckProjectData {
            name: project.name.clone(),
            graph,
            metrics,
            communities,
            cycles,
        });
    }

    let compiled_policy = if cfg.policy.is_empty() {
        None
    } else {
        Some(CompiledPolicy::compile(&cfg.policy).unwrap_or_else(|err| {
            eprintln!("Invalid policy config: {err}");
            std::process::exit(1);
        }))
    };

    let policy_results = if let Some(policy) = &compiled_policy {
        let policy_inputs: Vec<ProjectGraph<'_>> = analyzed_projects
            .iter()
            .map(|project| ProjectGraph {
                name: &project.name,
                graph: &project.graph,
            })
            .collect();
        policy.evaluate(&policy_inputs)
    } else {
        Vec::new()
    };

    let policy_by_name: HashMap<String, ProjectPolicyResult> = policy_results
        .into_iter()
        .map(|result| (result.name.clone(), result))
        .collect();

    let mut results = Vec::new();
    for project in analyzed_projects {
        let (summary, violations) = evaluate_quality_gates(
            &project.graph,
            &project.metrics,
            &project.communities,
            &project.cycles,
            &limits,
        );
        let policy_result =
            policy_by_name
                .get(&project.name)
                .cloned()
                .unwrap_or(ProjectPolicyResult {
                    name: project.name.clone(),
                    rules_evaluated: 0,
                    violations: Vec::new(),
                });
        results.push(build_project_check_result(
            &project.name,
            summary,
            limits.clone(),
            policy_result,
            violations,
        ));
    }

    let contracts = run_contract_gate(
        &cfg,
        config_path,
        contracts_mode,
        contracts_warnings_as_errors,
    );

    let report = build_check_report(results, contracts);
    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        print_check_report(&report);
        if let Some(contracts) = &report.contracts {
            print_contract_report(contracts);
        }
    }

    if !report.ok {
        std::process::exit(1);
    }
}
```

- [ ] **Step 4: Implement `run_contract_gate`**

Add to `main.rs`:

```rust
fn run_contract_gate(
    cfg: &Config,
    config_path: &Path,
    mode: ContractsMode,
    warnings_as_errors: bool,
) -> Option<graphify_report::ContractCheckResult> {
    let enabled = match mode {
        ContractsMode::Off => false,
        ContractsMode::On => true,
        ContractsMode::Auto => !cfg.contract.pairs.is_empty(),
    };
    if !enabled {
        return None;
    }
    if cfg.contract.pairs.is_empty() {
        eprintln!("warning: --contracts requested but no [[contract.pair]] declared; skipping");
        return None;
    }

    let global = build_global_contract_config(&cfg.contract, warnings_as_errors);
    let workspace_root = config_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let mut pair_results: Vec<graphify_report::ContractPairResult> = Vec::new();

    for pair in &cfg.contract.pairs {
        let pair_cfg = PairConfig {
            ignore_orm: pair.ignore.as_ref().map(|i| i.orm.clone()).unwrap_or_default(),
            ignore_ts: pair.ignore.as_ref().map(|i| i.ts.clone()).unwrap_or_default(),
            field_aliases: pair
                .field_alias
                .iter()
                .map(|a| FieldAlias {
                    orm: a.orm.clone(),
                    ts: a.ts.clone(),
                })
                .collect(),
            relation_aliases: pair
                .relation_alias
                .iter()
                .map(|a| FieldAlias {
                    orm: a.orm.clone(),
                    ts: a.ts.clone(),
                })
                .collect(),
        };

        let orm_path = workspace_root.join(&pair.orm.file);
        let ts_path = workspace_root.join(&pair.ts.file);

        let orm_source = std::fs::read_to_string(&orm_path).unwrap_or_else(|e| {
            eprintln!("contract pair '{}': cannot read ORM file {:?}: {e}", pair.name, orm_path);
            std::process::exit(1);
        });
        let ts_source = std::fs::read_to_string(&ts_path).unwrap_or_else(|e| {
            eprintln!("contract pair '{}': cannot read TS file {:?}: {e}", pair.name, ts_path);
            std::process::exit(1);
        });

        if pair.orm.source != "drizzle" {
            eprintln!(
                "contract pair '{}': orm.source '{}' not supported in v1 (use 'drizzle')",
                pair.name, pair.orm.source
            );
            std::process::exit(1);
        }

        let orm_contract = graphify_extract::extract_drizzle_contract_at(
            &orm_source,
            &pair.orm.table,
            orm_path.clone(),
        )
        .unwrap_or_else(|e| {
            eprintln!("contract pair '{}': Drizzle parse error: {e}", pair.name);
            std::process::exit(1);
        });

        let ts_contract = graphify_extract::extract_ts_contract_at(
            &ts_source,
            &pair.ts.export,
            ts_path.clone(),
        )
        .unwrap_or_else(|e| {
            eprintln!("contract pair '{}': TS parse error: {e}", pair.name);
            std::process::exit(1);
        });

        let cmp = graphify_core::contract::compare_contracts(
            &orm_contract,
            &ts_contract,
            &pair_cfg,
            &global,
        );

        let entries: Vec<graphify_report::ViolationEntry> = cmp
            .violations
            .into_iter()
            .map(|v| graphify_report::ViolationEntry {
                severity: v.severity(global.unmapped_type_severity),
                violation: v,
            })
            .collect();

        pair_results.push(graphify_report::ContractPairResult {
            name: pair.name.clone(),
            orm: graphify_report::ContractSideInfo {
                file: orm_path,
                symbol: pair.orm.table.clone(),
                line: 1, // see Note 1 below
            },
            ts: graphify_report::ContractSideInfo {
                file: ts_path,
                symbol: pair.ts.export.clone(),
                line: 1,
            },
            violations: entries,
        });
    }

    Some(graphify_report::build_contract_check_result(
        pair_results,
        global.unmapped_type_severity,
    ))
}

fn build_global_contract_config(
    cfg: &ContractConfigRaw,
    warnings_as_errors: bool,
) -> GlobalContractConfig {
    let unmapped_severity = if warnings_as_errors {
        Severity::Error
    } else {
        match cfg.unmapped_type_severity.as_str() {
            "error" => Severity::Error,
            _ => Severity::Warning,
        }
    };
    let case_rule = match cfg.case_rule.as_str() {
        "exact" => CaseRule::Exact,
        _ => CaseRule::SnakeCamel,
    };
    let overrides = cfg
        .type_map
        .iter()
        .map(|(k, v)| (k.clone(), parse_type_override(v)))
        .collect();
    GlobalContractConfig {
        case_rule,
        type_map_overrides: overrides,
        unmapped_type_severity: unmapped_severity,
    }
}

fn parse_type_override(spec: &str) -> FieldType {
    match spec {
        "string" => FieldType::Primitive { value: PrimitiveType::String },
        "number" => FieldType::Primitive { value: PrimitiveType::Number },
        "boolean" => FieldType::Primitive { value: PrimitiveType::Boolean },
        "date" => FieldType::Primitive { value: PrimitiveType::Date },
        "unknown" => FieldType::Primitive { value: PrimitiveType::Unknown },
        other => FieldType::Named {
            value: other.to_string(),
        },
    }
}

fn print_contract_report(c: &graphify_report::ContractCheckResult) {
    let status = if c.ok { "OK" } else { "FAILED" };
    println!(
        "[contracts] {} (errors={}, warnings={}, pairs={})",
        status,
        c.error_count,
        c.warning_count,
        c.pairs.len()
    );
    for pair in &c.pairs {
        if pair.violations.is_empty() {
            continue;
        }
        println!(
            "  pair: {} ({}::{} <-> {}::{})",
            pair.name,
            pair.orm.file.display(),
            pair.orm.symbol,
            pair.ts.file.display(),
            pair.ts.symbol,
        );
        for v in &pair.violations {
            let sev = match v.severity {
                Severity::Error => "error  ",
                Severity::Warning => "warning",
            };
            println!("    {sev} {:?}", v.violation);
        }
    }
}
```

> **Note 1:** pair-level `line` (of the ORM table declaration / TS export) is a nice-to-have that requires a second tree-sitter pass to find. v1 hard-codes `line: 1` with the known limitation documented in the release notes. FEAT-015 (editor integration) can improve this.

- [ ] **Step 5: Build and verify crate compiles**

Run: `cargo build -p graphify-cli`
Expected: compiles with warnings at most. Fix any type mismatch (most likely: use of `serde_json::Value` or missing imports).

- [ ] **Step 6: Create the integration fixture**

Create directory `tests/fixtures/contract_drift/monorepo/`. Inside, create the following files:

`tests/fixtures/contract_drift/monorepo/graphify.toml`:

```toml
[settings]
output = "./report"

[[project]]
name = "db"
repo = "./packages/db"
lang = ["typescript"]

[[project]]
name = "api"
repo = "./packages/api"
lang = ["typescript"]

[[contract.pair]]
name = "user"
orm  = { source = "drizzle", file = "packages/db/src/schema/user.ts", table = "users" }
ts   = { file   = "packages/api/src/types/user.ts", export = "UserDto" }

[[contract.pair]]
name = "post"
orm  = { source = "drizzle", file = "packages/db/src/schema/post.ts", table = "posts" }
ts   = { file   = "packages/api/src/types/post.ts", export = "PostDto" }
```

`tests/fixtures/contract_drift/monorepo/packages/db/src/schema/user.ts`:

```ts
import { pgTable, text, integer, uuid, timestamp } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id:        uuid('id').primaryKey(),
  email:     text('email').notNull(),
  age:       integer('age').notNull(),
  createdAt: timestamp('created_at').notNull(),
});
```

`tests/fixtures/contract_drift/monorepo/packages/api/src/types/user.ts`:

```ts
export interface UserDto {
  id: string;
  email: string;
  age: number;
  createdAt: Date;
}
```

`tests/fixtures/contract_drift/monorepo/packages/db/src/schema/post.ts`:

```ts
import { pgTable, text, uuid } from 'drizzle-orm/pg-core';
import { relations } from 'drizzle-orm';

export const posts = pgTable('posts', {
  id:      uuid('id').primaryKey(),
  title:   text('title').notNull(),
  tags:    tsvector('tags').notNull(),
});

export const postsRelations = relations(posts, ({ one }) => ({
  author: one(users),
}));
```

`tests/fixtures/contract_drift/monorepo/packages/api/src/types/post.ts`:

```ts
export interface PostDto {
  id: string;
  title: string;
  authors: PostDto[];
}
```

This fixture encodes: user pair is clean; post pair has `tags` (unmapped `tsvector`), relation name mismatch (`author` vs `authors`), and cardinality mismatch (`one` vs `[]`).

- [ ] **Step 7: Write integration tests**

Create `tests/contract_integration.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn graphify_bin() -> PathBuf {
    static GRAPHIFY_BIN: OnceLock<PathBuf> = OnceLock::new();
    GRAPHIFY_BIN
        .get_or_init(|| {
            let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let status = Command::new("cargo")
                .current_dir(&workspace_root)
                .args(["build", "-q", "-p", "graphify-cli", "--bin", "graphify"])
                .status()
                .expect("build graphify binary for integration tests");
            assert!(status.success(), "cargo build failed");
            workspace_root.join("target/debug/graphify")
        })
        .clone()
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contract_drift/monorepo")
}

fn run_check(args: &[&str]) -> std::process::Output {
    Command::new(graphify_bin())
        .current_dir(fixture_dir())
        .arg("check")
        .args(args)
        .output()
        .expect("run graphify check")
}

#[test]
fn drifted_pair_fails_with_expected_violations() {
    let out = run_check(&["--config", "graphify.toml", "--json"]);
    assert!(!out.status.success(), "exit code should be non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"contracts\""), "missing contracts block");
    assert!(stdout.contains("contract_unmapped_orm_type"), "missing unmapped violation");
    assert!(
        stdout.contains("contract_relation_missing_on_ts")
            || stdout.contains("contract_relation_missing_on_orm"),
        "missing relation violation"
    );
}

#[test]
fn no_contracts_flag_skips_gate() {
    let out = run_check(&["--config", "graphify.toml", "--json", "--no-contracts"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // `contracts` should be omitted entirely.
    assert!(!stdout.contains("\"contracts\""));
}

#[test]
fn warnings_as_errors_escalates_unmapped() {
    let out = run_check(&[
        "--config", "graphify.toml", "--json", "--contracts-warnings-as-errors",
    ]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Every unmapped entry should now be counted as an error.
    assert!(stdout.contains("\"severity\": \"error\""));
    // warning_count for the pair containing tsvector must drop to 0 because it was escalated.
    // A coarse check: find the "warning_count" line with value 0 anywhere in the contracts block.
    assert!(stdout.contains("\"warning_count\": 0"), "warnings should be escalated to errors");
}

#[test]
fn human_output_prints_contracts_section() {
    let out = run_check(&["--config", "graphify.toml"]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[contracts] FAILED"));
}

#[test]
fn idempotent_json_output() {
    let a = run_check(&["--config", "graphify.toml", "--json"]);
    let b = run_check(&["--config", "graphify.toml", "--json"]);
    assert_eq!(a.stdout, b.stdout, "JSON output must be deterministic across runs");
}

#[test]
fn missing_orm_file_produces_clear_error() {
    let broken = fixture_dir().join("graphify.broken.toml");
    std::fs::write(
        &broken,
        r#"
[settings]
output = "./report"

[[project]]
name = "db"
repo = "./packages/db"
lang = ["typescript"]

[[contract.pair]]
name = "user"
orm  = { source = "drizzle", file = "packages/db/src/schema/does_not_exist.ts", table = "users" }
ts   = { file   = "packages/api/src/types/user.ts", export = "UserDto" }
"#,
    )
    .unwrap();
    let out = Command::new(graphify_bin())
        .current_dir(fixture_dir())
        .args(["check", "--config", "graphify.broken.toml"])
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cannot read ORM file"), "expected clear error, got: {stderr}");
    std::fs::remove_file(&broken).ok();
}
```

- [ ] **Step 8: Run the integration tests**

Run: `cargo test --test contract_integration`
Expected: all 6 tests pass. The harness builds the binary automatically via `OnceLock`.

If any test fails, inspect stderr (`--nocapture`): `cargo test --test contract_integration -- --nocapture`.

- [ ] **Step 9: Commit**

```bash
git add crates/graphify-cli/src/main.rs tests/contract_integration.rs tests/fixtures/contract_drift
git commit -m "feat(cli): wire contract drift gate into graphify check (FEAT-016)"
```

---

## Task 15: Close-out — README, task tracker, version bump

**Files:**
- Modify: `README.md` (new section documenting `[[contract.pair]]` config + CLI usage)
- Modify: `CHANGELOG.md`
- Modify: `docs/TaskNotes/Tasks/FEAT-016-contract-drift-detection-between-orm-and-typescript.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`
- Modify: `Cargo.toml` (bump workspace version)

- [ ] **Step 1: Document in README**

Add a new section titled `## Contract Drift (v0.5.0+)` under the existing feature sections. Include:
- One-paragraph intro.
- Minimal `[[contract.pair]]` TOML example identical to Task 14's fixture config.
- Exact command: `graphify check --contracts`.
- Supported sources: Drizzle (Postgres/MySQL/SQLite) + TS interface/type.
- Explicit v1 limitations: no Prisma, no Zod, single-file pair references, `target_contract` not compared.

- [ ] **Step 2: Update CHANGELOG**

Under a new `## 0.5.0 — YYYY-MM-DD` heading add one bullet:

```
- feat(cli): FEAT-016 contract drift detection between Drizzle ORM schemas and TS interface/type declarations, integrated into `graphify check`.
```

- [ ] **Step 3: Close FEAT-016 tasknote**

Set `status: done`, `completed: <today's date>` in frontmatter. Check all subtasks. Add Verification section listing: all 15 tasks complete, full test count (prior + ~40 new = ~309), commit hashes of the shipped work.

- [ ] **Step 4: Update sprint.md**

Flip FEAT-016 row to `**done**` and add an entry under `## Done`:

```
- [[FEAT-016-contract-drift-detection-between-orm-and-typescript]] - Implemented: Drizzle-to-TS contract drift detection via `graphify check`, built-in type map + overrides, snake_case<->camelCase normalization, relation cardinality comparison, JSON + Markdown + human output, 6 integration + ~34 unit tests (YYYY-MM-DD)
```

- [ ] **Step 5: Bump workspace version**

In root `Cargo.toml` under `[workspace.package]`, change `version = "0.4.1"` to `version = "0.5.0"`.

- [ ] **Step 6: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: all tests pass. Count should be 269 prior + ~40 new = ~309.

- [ ] **Step 7: Run clippy strict**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit close-out**

```bash
git add README.md CHANGELOG.md docs/TaskNotes/Tasks/FEAT-016-contract-drift-detection-between-orm-and-typescript.md docs/TaskNotes/Tasks/sprint.md Cargo.toml Cargo.lock
git commit -m "chore: ship FEAT-016 contract drift v0.5.0 (docs + version bump)"
```

---

## Self-Review Notes

**Spec coverage:**
- §1 Scope → Tasks 6–11 (Drizzle scalar + relations + TS parser + scalar-only) plus Task 14 fixture covers drift classes.
- §2 Architecture → Tasks 1 (core module), 6 (drizzle), 9 (ts_contract), 12/13 (report), 14 (cli wiring).
- §3 Config → Task 14 Step 1 + Step 4 (raw config + global config builder).
- §4 Data model → Task 1.
- §5 Drizzle parser → Tasks 6, 7, 8.
- §6 TS parser → Tasks 9, 10, 11.
- §7 Comparison algorithm → Tasks 2 (alignment), 3 (per-field), 4 (relations), 5 (ordering).
- §8 Output → Tasks 12 (JSON), 13 (Markdown), 14 Steps 3–4 (CLI wiring, human output).
- §9 Testing strategy → Tier 1 covered by Tasks 1–5 unit tests; Tier 2 by Tasks 6–11; Tier 3 by Task 14.
- §10 Open questions deferred → explicitly out of scope, no task.
- §11 References → no task required.

**Gaps / known limitations:**
- Pair-level declaration `line` hard-coded to 1 (documented in Task 14 Note 1). Acceptable tradeoff for v1; FEAT-015 will close this.
- Performance guardrail test (100-pair fixture from Spec §9) NOT included in Task 14. Added as an explicit deferred item in the FEAT-016 tasknote's Verification section — revisit when real-world usage appears.
- HTML contract panel (Spec §8) NOT implemented. Deferred: add once editor integration (FEAT-015) defines the interaction model.

**Placeholder scan:**
- No `TBD`, `TODO`, or `implement later` in any task body. All code blocks are complete.

**Type consistency check:**
- `Contract`, `Field`, `FieldType`, `Relation`, `Cardinality`, `ContractSide`, `ContractViolation`, `Severity` all declared in Task 1 and used consistently through Tasks 2–14.
- `PairConfig`, `GlobalContractConfig`, `FieldAlias`, `CaseRule` declared in Task 2 and used in Tasks 3, 4, 5, 14.
- `extract_drizzle_contract`, `extract_drizzle_contract_at`, `DrizzleParseError` declared in Task 6 and re-exported from `lib.rs` in the same task.
- `extract_ts_contract`, `extract_ts_contract_at`, `TsContractParseError` declared in Task 6 as stubs, fully implemented in Task 9, extended with `parse_all_ts_contracts` in Task 10.
- `ContractCheckResult`, `ContractPairResult`, `ContractSideInfo`, `ViolationEntry`, `build_contract_check_result` declared in Task 12 and consumed in Tasks 13, 14.
- `CheckReport` extension (`contracts: Option<_>`) added in Task 14 Step 3 is consistent with how it's read in the human and JSON output paths of the same task.
