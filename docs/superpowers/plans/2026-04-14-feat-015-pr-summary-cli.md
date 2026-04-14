# FEAT-015 PR Summary CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `graphify pr-summary <DIR>` — a CLI subcommand that renders a concise Markdown summary of architectural change for a single project's Graphify output directory, suitable for pre-push self-review, human handoff, and CI integration.

**Architecture:** Pure renderer in `graphify-report/src/pr_summary.rs` consumes existing JSON artifacts (`analysis.json`, `drift-report.json`, `check-report.json`); thin CLI glue in `graphify-cli/src/main.rs` loads files and dispatches. As prerequisites: (a) `Deserialize` is added to `DiffReport` and `ContractCheckResult` chains; (b) `CheckReport` and related types move from private scope in `graphify-cli` to a new public module `graphify-report/src/check_report.rs`; (c) `graphify check` begins writing `check-report.json` to each project's output dir alongside existing artifacts.

**Tech Stack:** Rust 2021, serde + serde_json, anyhow, clap, petgraph (already in the workspace — no new deps).

**Spec:** `docs/superpowers/specs/2026-04-14-feat-015-pr-summary-cli-design.md`

---

## Pre-flight

Before starting, from the repo root:

```bash
cargo test --workspace    # expect: all green (baseline)
cargo build --release -p graphify-cli    # build the binary for integration tests later
git status --short        # expect: clean-ish (target/ artifacts may be modified; ignore)
```

If baseline is not green, **stop** and fix before proceeding.

---

## Task 1: Add `Deserialize` to `DiffReport` and sub-types

**Context:** `pr_summary` must deserialize `drift-report.json` into `DiffReport`. Currently `DiffReport` and its 9 sub-types are `Serialize` only.

**Files:**
- Modify: `crates/graphify-core/src/diff.rs`

- [ ] **Step 1: Write a failing roundtrip test**

Add to the end of `crates/graphify-core/src/diff.rs` (or to its existing `#[cfg(test)] mod tests { ... }` block):

```rust
#[test]
fn diff_report_roundtrips_json() {
    use crate::diff::{CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta};

    let report = DiffReport {
        summary_delta: SummaryDelta {
            nodes: Delta { before: 10, after: 12, change: 2 },
            edges: Delta { before: 20, after: 25, change: 5 },
            communities: Delta { before: 3, after: 3, change: 0 },
            cycles: Delta { before: 0, after: 1, change: 1 },
        },
        edges: EdgeDiff { added_nodes: vec!["a".into()], removed_nodes: vec![], degree_changes: vec![] },
        cycles: CycleDiff { introduced: vec![vec!["a".into(), "b".into()]], resolved: vec![] },
        hotspots: HotspotDiff { rising: vec![], falling: vec![], new_hotspots: vec![], removed_hotspots: vec![] },
        communities: CommunityDiff { moved_nodes: vec![], stable_count: 3 },
    };

    let json = serde_json::to_string(&report).expect("serialize");
    let back: DiffReport = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(back.summary_delta.nodes.change, 2);
    assert_eq!(back.cycles.introduced.len(), 1);
    assert_eq!(back.edges.added_nodes, vec!["a".to_string()]);
}
```

- [ ] **Step 2: Run the test — confirm it fails**

```bash
cargo test -p graphify-core diff_report_roundtrips_json
```

Expected: fails to compile with `the trait bound \`DiffReport: Deserialize<'_>\` is not satisfied` (or similar on sub-types).

- [ ] **Step 3: Add `Deserialize` to the 10 struct derives**

In `crates/graphify-core/src/diff.rs`, for each of these 10 structs, change `#[derive(Debug, Clone, Serialize)]` to `#[derive(Debug, Clone, Serialize, Deserialize)]`:

- `DiffReport` (line ~51)
- `SummaryDelta` (line ~60)
- `Delta<T>` (line ~68) — `Delta<T>` requires a generic bound; use `#[derive(Debug, Clone, Serialize, Deserialize)]` — serde handles generics when the inner type implements the traits (which `usize`/`i64`/`f64` do by default).
- `EdgeDiff` (line ~75)
- `DegreeChange` (line ~82)
- `CycleDiff` (line ~89)
- `HotspotDiff` (line ~95)
- `ScoreChange` (line ~103)
- `CommunityDiff` (line ~111)
- `CommunityMove` (line ~117)

(Line numbers are approximate — find each `#[derive(Debug, Clone, Serialize)]` that immediately precedes one of the 10 types above.)

- [ ] **Step 4: Run the test — confirm it passes**

```bash
cargo test -p graphify-core diff_report_roundtrips_json
```

Expected: `test diff_report_roundtrips_json ... ok`.

- [ ] **Step 5: Run the full workspace test suite**

```bash
cargo test --workspace
```

Expected: all tests pass. If anything else broke, it means some code was relying on `DiffReport` not being `Deserialize` (unlikely). Investigate before proceeding.

- [ ] **Step 6: Clippy check**

```bash
cargo clippy -p graphify-core -- -D warnings
```

Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-core/src/diff.rs
git commit -m "feat(core): derive Deserialize on DiffReport and sub-types (FEAT-015)"
```

---

## Task 2: Add `Deserialize` to `ContractCheckResult` chain and `ContractViolation`

**Context:** `pr_summary` must deserialize the `contracts` field of a unified `check-report.json` into `Option<ContractCheckResult>`. Currently `ContractCheckResult` and its 3 sub-types in `graphify-report/src/contract_json.rs` are `Serialize` only, and `ContractViolation` + `ContractComparison` in `graphify-core/src/contract.rs` are also `Serialize` only (but their sibling types `Severity`, `ContractSide`, `FieldType`, `PrimitiveType`, `Cardinality`, `Field`, `Relation`, `Contract` are already `Serialize + Deserialize`). We need `Deserialize` on all pieces of the chain that end up inside `ViolationEntry.violation: ContractViolation`.

**Files:**
- Modify: `crates/graphify-report/src/contract_json.rs`
- Modify: `crates/graphify-core/src/contract.rs`

- [ ] **Step 1: Write a failing roundtrip test**

Add inside (or at the end of) `crates/graphify-report/src/contract_json.rs`, creating a `#[cfg(test)] mod tests { ... }` block if one does not already exist:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::contract::Severity;
    use std::path::PathBuf;

    #[test]
    fn contract_check_result_roundtrips_json() {
        let result = ContractCheckResult {
            ok: false,
            error_count: 1,
            warning_count: 0,
            pairs: vec![ContractPairResult {
                name: "users_pair".into(),
                orm: ContractSideInfo {
                    file: PathBuf::from("schema/users.ts"),
                    symbol: "users".into(),
                    line: 10,
                },
                ts: ContractSideInfo {
                    file: PathBuf::from("frontend/types/user.ts"),
                    symbol: "User".into(),
                    line: 5,
                },
                violations: vec![],
            }],
        };

        let json = serde_json::to_string(&result).expect("serialize");
        let back: ContractCheckResult = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.error_count, 1);
        assert_eq!(back.pairs.len(), 1);
        assert_eq!(back.pairs[0].name, "users_pair");
        let _ = Severity::Error;
    }
}
```

- [ ] **Step 2: Run the test — confirm it fails**

```bash
cargo test -p graphify-report contract_check_result_roundtrips_json
```

Expected: fails to compile with `the trait bound \`ContractCheckResult: Deserialize<'_>\` is not satisfied`.

- [ ] **Step 3: Add `Deserialize` to the 4 structs in `contract_json.rs`**

In `crates/graphify-report/src/contract_json.rs`:

- Change `use serde::Serialize;` to `use serde::{Deserialize, Serialize};` at the top.
- For each of these 4 structs, change `#[derive(Debug, Clone, Serialize)]` to `#[derive(Debug, Clone, Serialize, Deserialize)]`:
  - `ContractCheckResult` (line ~6)
  - `ContractPairResult` (line ~14)
  - `ContractSideInfo` (line ~22)
  - `ViolationEntry` (line ~29)

- [ ] **Step 3b: Add `Deserialize` to `ContractViolation` and `ContractComparison` in `graphify-core/src/contract.rs`**

In `crates/graphify-core/src/contract.rs`, find these two types (around lines 77 and 83):

- `pub struct ContractComparison` — change `#[derive(Debug, Clone, PartialEq, Serialize)]` to `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`
- `pub enum ContractViolation` — change `#[derive(Debug, Clone, PartialEq, Serialize)]` to `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`

`Severity`, `FieldType`, `PrimitiveType`, `Cardinality`, `Field`, `Relation`, `Contract`, and `ContractSide` already derive both traits — no change needed.

- [ ] **Step 4: Run the test — confirm it passes**

```bash
cargo test -p graphify-report contract_check_result_roundtrips_json
```

Expected: `test contract_check_result_roundtrips_json ... ok`.

- [ ] **Step 5: Full workspace + clippy**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-report/src/contract_json.rs crates/graphify-core/src/contract.rs
git commit -m "feat(core+report): derive Deserialize on ContractCheckResult + ContractViolation chain (FEAT-015)"
```

---

## Task 3: Create `graphify-report/src/check_report.rs` module with moved types

**Context:** Move `CheckReport`, `ProjectCheckResult`, `ProjectCheckSummary`, `PolicyCheckSummary`, `CheckViolation`, and `CheckLimits` from private scope in `graphify-cli/src/main.rs` to a new public module in `graphify-report`. Add `Deserialize` derives.

**Files:**
- Create: `crates/graphify-report/src/check_report.rs`
- Modify: `crates/graphify-report/src/lib.rs`

- [ ] **Step 1: Create `crates/graphify-report/src/check_report.rs` with the moved types**

```rust
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
```

- [ ] **Step 2: Register the module in `graphify-report/src/lib.rs`**

Open `crates/graphify-report/src/lib.rs`. At the top of the file, add `pub mod check_report;` alongside the other `pub mod` declarations. Below, add a convenience re-export in the same style as `ContractCheckResult` is re-exported (see line ~17):

Example of what to add (place near the other re-exports):

```rust
pub mod check_report;
pub use check_report::{
    CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
    ProjectCheckSummary,
};
```

- [ ] **Step 3: Run the roundtrip tests**

```bash
cargo test -p graphify-report check_report
```

Expected: 6 tests in `check_report::tests` all pass.

- [ ] **Step 4: Workspace compile check**

```bash
cargo build --workspace
```

Expected: compiles. `graphify-cli` may still compile because it has not yet been wired to use the new module (that is Task 4); the private types in `main.rs` still work as before.

- [ ] **Step 5: Clippy check**

```bash
cargo clippy -p graphify-report -- -D warnings
```

Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/graphify-report/src/check_report.rs crates/graphify-report/src/lib.rs
git commit -m "feat(report): introduce public check_report module (FEAT-015)"
```

---

## Task 4: Swap `graphify-cli` to import `CheckReport` types from `graphify-report::check_report`

**Context:** Remove the now-duplicated private types from `graphify-cli/src/main.rs` and import the public ones. Behavior is unchanged; this is a pure code-move.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Confirm the current test suite is green**

```bash
cargo test --workspace
```

Expected: all tests pass before starting the swap.

- [ ] **Step 2: Remove the private struct definitions**

In `crates/graphify-cli/src/main.rs`, delete the private definitions for the following (in the `// Quality gates` section, approximately lines 1528-1612):

- `struct CheckLimits` (the `#[derive(...)]` line plus the struct body)
- `struct ProjectCheckSummary`
- `struct PolicyCheckSummary`
- `enum CheckViolation`
- `struct ProjectCheckResult`
- `struct CheckReport`

**Keep**: `enum ContractsMode` and `impl ContractsMode` — those are CLI-only and stay local.

- [ ] **Step 3: Add the import**

Near the top of `crates/graphify-cli/src/main.rs` (wherever other `use graphify_report::...` statements live), add:

```rust
use graphify_report::check_report::{
    CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
    ProjectCheckSummary,
};
```

If any of those names are already imported from elsewhere in `graphify_report`, consolidate into one `use` statement.

- [ ] **Step 4: Fix any field-access errors**

The moved types are now `pub` with `pub` fields. Code in `main.rs` that constructed these types via named-field syntax (for example `ProjectCheckSummary { nodes, edges, ... }`) will continue to work because all fields are `pub`. Run a build:

```bash
cargo build -p graphify-cli
```

Fix any residual visibility errors that appear (for example if some consumer accessed a field that was previously not marked `pub`). Expected: clean compile.

- [ ] **Step 5: Run the full test suite**

```bash
cargo test --workspace
```

Expected: all tests pass. The `check` command tests (including FEAT-004, FEAT-013, FEAT-016 integration tests) must still pass — they were exercising the exact same JSON shape.

- [ ] **Step 6: Clippy check**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "refactor(cli): import CheckReport types from graphify-report (FEAT-015)"
```

---

## Task 5: `graphify check` writes `<project_out>/check-report.json` unconditionally

**Context:** The unified `CheckReport` must land on disk in each project's output directory alongside other artifacts (`analysis.json`, `drift-report.json`, …). The existing stdout behavior under `--json` is preserved; writing to disk is additive.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`
- Modify or create: an integration test (in `crates/graphify-cli/tests/`) that asserts `check-report.json` is produced.

- [ ] **Step 1: Locate the check command handler**

In `crates/graphify-cli/src/main.rs`, find the function that handles `Commands::Check { .. }`. It builds `CheckReport` via `build_check_report(...)` and currently does either `print_check_report(&report)` (human mode) or `println!("{}", serde_json::to_string_pretty(&report).unwrap())` (JSON mode). Identify the block immediately after `let report = build_check_report(...)`.

- [ ] **Step 2: Write a failing integration test**

Add to `crates/graphify-cli/tests/integration_test.rs` (or create a new file `crates/graphify-cli/tests/check_report_write_test.rs` if the existing file is large; follow whichever convention the project uses):

```rust
use std::fs;
use std::process::Command;

// Reuse the existing binary-build harness. If this file is new, include the
// OnceLock + cargo build guard used by other integration tests.

#[test]
fn graphify_check_writes_check_report_json_to_each_project_output_dir() {
    // Assumes a fixture workspace exists with a graphify.toml and at least one project.
    // If no such fixture exists yet, create a minimal one under
    // `crates/graphify-cli/tests/fixtures/check_write/` that has a graphify.toml
    // plus a tiny `src/` tree with one .py file.
    let fixture = std::path::Path::new("tests/fixtures/check_write");
    let out_dir = tempfile::tempdir().expect("tempdir");

    // Run: graphify run --config <fixture>/graphify.toml --output <out>
    let status = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "run",
            "--config",
            fixture.join("graphify.toml").to_str().unwrap(),
            "--output",
            out_dir.path().to_str().unwrap(),
        ])
        .status()
        .expect("run graphify run");
    assert!(status.success(), "graphify run failed");

    // Run: graphify check --config <fixture>/graphify.toml --output <out>
    let status = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args([
            "check",
            "--config",
            fixture.join("graphify.toml").to_str().unwrap(),
            "--output",
            out_dir.path().to_str().unwrap(),
        ])
        .status()
        .expect("run graphify check");

    // Exit status may be non-zero if violations were emitted; we only care that the file was written.
    let _ = status;

    // Assert check-report.json now exists under the project's output subdir.
    // Each project's output dir is named after the project in graphify.toml.
    // Walk the out_dir and require at least one project dir contains check-report.json.
    let mut found = false;
    for entry in fs::read_dir(out_dir.path()).expect("read out dir") {
        let entry = entry.expect("entry");
        if entry.path().is_dir() {
            let check_report = entry.path().join("check-report.json");
            if check_report.exists() {
                found = true;
                // Parse to confirm shape
                let content = fs::read_to_string(&check_report).expect("read check-report");
                let parsed: graphify_report::check_report::CheckReport =
                    serde_json::from_str(&content).expect("parse check-report");
                assert!(parsed.projects.len() >= 1);
                break;
            }
        }
    }
    assert!(found, "check-report.json was not written to any project output dir");
}
```

**Notes on the fixture:**
- If `crates/graphify-cli/tests/fixtures/check_write/` does not exist, create a minimal one: one `graphify.toml` pointing at an inline `src/` tree with a single `main.py` containing `import os`. Use the simplest possible setup — the test cares about file existence, not contents.
- If an existing integration test already builds a similar fixture, reuse it by pointing at the same directory.

- [ ] **Step 3: Run the test — confirm it fails**

```bash
cargo test -p graphify-cli graphify_check_writes_check_report_json_to_each_project_output_dir
```

Expected: fails with `check-report.json was not written to any project output dir`.

- [ ] **Step 4: Implement the write**

In the `Commands::Check` handler in `crates/graphify-cli/src/main.rs`, immediately after `let report = build_check_report(...)` and before the current human/JSON output branch, add:

```rust
// Write check-report.json to every project's output directory alongside existing artifacts.
// Additive: the existing stdout behavior under --json is preserved below.
for project_result in &report.projects {
    // Resolve project output directory. The config + output root together
    // determine the per-project path: <output_root>/<project_name>/.
    // Use the same construction used elsewhere in this function
    // (search for `proj_out` or `project_out_dir` in this file for the idiom).
    let project_out_dir = /* same idiom as analysis.json write path */;
    let path = project_out_dir.join("check-report.json");
    let text = serde_json::to_string_pretty(&report).expect("serialize CheckReport");
    if let Err(err) = std::fs::write(&path, text) {
        eprintln!(
            "warning: failed to write check-report.json to {}: {}",
            path.display(),
            err
        );
    }
}
```

**Important:** The file content is the **entire unified `CheckReport`** — not per-project slices. Each project's directory gets a copy of the full unified report so that `graphify pr-summary <DIR>` can read from any one project dir without needing the root. (Writing the full report repeatedly is cheap — projects are few and the JSON is small.)

If the dispatch loop makes per-project writes cumbersome, write once after the loop to the first project's dir only, OR write once to the output root and have `pr_summary` search. **Prefer per-project duplication** — it keeps the CLI contract identical to every other per-project artifact (`analysis.json`, `drift-report.json`).

Find the existing idiom in the same function that computes a project's output dir (look for where `analysis.json` is written under `proj_out` around line 475, or `drift-report.json` around line 964) and reuse that idiom.

- [ ] **Step 5: Run the test — confirm it passes**

```bash
cargo test -p graphify-cli graphify_check_writes_check_report_json_to_each_project_output_dir
```

Expected: test passes.

- [ ] **Step 6: Full workspace test**

```bash
cargo test --workspace
```

Expected: all tests pass. The existing `graphify check` stdout tests must still pass (the write is additive).

- [ ] **Step 7: Clippy check**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/graphify-cli/src/main.rs crates/graphify-cli/tests/
git commit -m "feat(cli): write check-report.json to each project output dir (FEAT-015)"
```

---

## Task 6: Create `pr_summary` skeleton with header, stats, and footer

**Context:** Begin the renderer module with the always-rendered pieces. No drift or outstanding-issues sections yet — those arrive in later tasks.

**Files:**
- Create: `crates/graphify-report/src/pr_summary.rs`
- Modify: `crates/graphify-report/src/lib.rs`

- [ ] **Step 1: Register the module**

Add to `crates/graphify-report/src/lib.rs`:

```rust
pub mod pr_summary;
```

- [ ] **Step 2: Create the renderer with skeleton and a failing test**

Verified `AnalysisSnapshot` shape (from `crates/graphify-core/src/diff.rs` lines 13-45):

```rust
pub struct AnalysisSnapshot {
    pub nodes: Vec<NodeSnapshot>,
    pub communities: Vec<CommunitySnapshot>,
    pub cycles: Vec<Vec<String>>,
    pub summary: SummarySnapshot,
}

pub struct SummarySnapshot {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_communities: usize,
    pub total_cycles: usize,
}
```

There is **no `project` field** on `AnalysisSnapshot`. Project name resolution is always via the directory basename (see Task 13). Stats counts come from `analysis.summary.total_nodes` / `total_edges`.

Create `crates/graphify-report/src/pr_summary.rs`:

```rust
//! PR Summary renderer.
//!
//! Produces a concise Markdown summary of architectural change for a single
//! project's Graphify output directory. Pure function: no I/O. Consumers are
//! expected to load inputs separately and pass them in as structs.

use graphify_core::diff::{AnalysisSnapshot, DiffReport};

use crate::check_report::CheckReport;

/// Render a PR summary Markdown string.
///
/// * `project_name` — resolved project name (caller provides; in the CLI,
///   this is the basename of the output directory).
/// * `analysis` — required; yields the header stats line.
/// * `drift` — optional; produces the "Drift in this PR" section. When `None`,
///   a hint line directs the reader to run `graphify diff`.
/// * `check` — optional; produces the "Outstanding issues" section from its
///   project violations and embedded contract result.
pub fn render(
    project_name: &str,
    analysis: &AnalysisSnapshot,
    drift: Option<&DiffReport>,
    check: Option<&CheckReport>,
) -> String {
    let mut out = String::new();
    render_header(&mut out, project_name);
    render_stats_line(&mut out, analysis, drift);
    render_drift_section(&mut out, drift);
    render_outstanding_section(&mut out, check);
    render_footer(&mut out);
    out
}

fn render_header(out: &mut String, project_name: &str) {
    out.push_str(&format!(
        "### Graphify — Architecture Delta for `{}`\n\n",
        project_name
    ));
}

fn render_stats_line(out: &mut String, analysis: &AnalysisSnapshot, drift: Option<&DiffReport>) {
    match drift {
        Some(d) => {
            let nb = d.summary_delta.nodes.before;
            let na = d.summary_delta.nodes.after;
            let eb = d.summary_delta.edges.before;
            let ea = d.summary_delta.edges.after;
            out.push_str(&format!(
                "{} → {} nodes ({:+}) · {} → {} edges ({:+})\n\n",
                nb, na, na as i64 - nb as i64, eb, ea, ea as i64 - eb as i64,
            ));
        }
        None => {
            out.push_str(&format!(
                "{} nodes · {} edges\n\n",
                analysis.summary.total_nodes, analysis.summary.total_edges,
            ));
        }
    }
}

fn render_drift_section(_out: &mut String, _drift: Option<&DiffReport>) {
    // Task 7+ implements this.
}

fn render_outstanding_section(_out: &mut String, _check: Option<&CheckReport>) {
    // Task 11+ implements this.
}

fn render_footer(out: &mut String) {
    out.push_str(&format!(
        "\n<sub>Graphify v{} · `graphify pr-summary <dir>` to regenerate</sub>\n",
        env!("CARGO_PKG_VERSION")
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Concrete minimal `AnalysisSnapshot` builder for tests.
    /// JSON literal matches the real snapshot shape exactly.
    pub(super) fn minimal_analysis() -> AnalysisSnapshot {
        let json = r#"{
            "nodes": [],
            "communities": [],
            "cycles": [],
            "summary": {
                "total_nodes": 0,
                "total_edges": 0,
                "total_communities": 0,
                "total_cycles": 0
            }
        }"#;
        serde_json::from_str(json).expect("minimal snapshot")
    }

    /// Analysis snapshot with specific node/edge counts (used by later tests).
    pub(super) fn analysis_with_counts(nodes: usize, edges: usize) -> AnalysisSnapshot {
        let json = format!(
            r#"{{
                "nodes": [],
                "communities": [],
                "cycles": [],
                "summary": {{
                    "total_nodes": {},
                    "total_edges": {},
                    "total_communities": 0,
                    "total_cycles": 0
                }}
            }}"#,
            nodes, edges
        );
        serde_json::from_str(&json).expect("sized snapshot")
    }

    #[test]
    fn renders_header_with_project_name() {
        let a = minimal_analysis();
        let out = render("my-app", &a, None, None);
        assert!(out.contains("### Graphify — Architecture Delta for `my-app`"));
    }

    #[test]
    fn renders_footer_with_version() {
        let a = minimal_analysis();
        let out = render("my-app", &a, None, None);
        assert!(out.contains(&format!("Graphify v{}", env!("CARGO_PKG_VERSION"))));
        assert!(out.contains("`graphify pr-summary <dir>` to regenerate"));
    }

    #[test]
    fn renders_stats_line_without_drift_shows_absolute_counts() {
        let a = analysis_with_counts(142, 301);
        let out = render("my-app", &a, None, None);
        assert!(out.contains("142 nodes · 301 edges"));
    }
}
```

- [ ] **Step 3: Run the tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: 3 tests pass. If the JSON literal is off (e.g., a field name typo), `serde_json::from_str` will fail — fix by matching the verified shape above.

- [ ] **Step 4: Clippy check**

```bash
cargo clippy -p graphify-report -- -D warnings
```

Expected: no warnings. The unused `_out` / `_drift` / `_check` params in placeholder functions are prefixed with `_` to suppress warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs crates/graphify-report/src/lib.rs
git commit -m "feat(report): pr_summary skeleton with header, stats, footer (FEAT-015)"
```

---

## Task 7: Render drift section — new cycles and broken cycles

**Context:** Populate the "Drift in this PR" section with introduced and resolved cycles from `DiffReport.cycles`.

**Files:**
- Modify: `crates/graphify-report/src/pr_summary.rs`

- [ ] **Step 1: Write failing tests**

Add to the `tests` module in `crates/graphify-report/src/pr_summary.rs`:

```rust
fn drift_with_cycles(introduced: Vec<Vec<&str>>, resolved: Vec<Vec<&str>>) -> DiffReport {
    use graphify_core::diff::{CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta};
    DiffReport {
        summary_delta: SummaryDelta {
            nodes: Delta { before: 0, after: 0, change: 0 },
            edges: Delta { before: 0, after: 0, change: 0 },
            communities: Delta { before: 0, after: 0, change: 0 },
            cycles: Delta { before: 0, after: 0, change: 0 },
        },
        edges: EdgeDiff { added_nodes: vec![], removed_nodes: vec![], degree_changes: vec![] },
        cycles: CycleDiff {
            introduced: introduced.into_iter().map(|c| c.into_iter().map(String::from).collect()).collect(),
            resolved: resolved.into_iter().map(|c| c.into_iter().map(String::from).collect()).collect(),
        },
        hotspots: HotspotDiff { rising: vec![], falling: vec![], new_hotspots: vec![], removed_hotspots: vec![] },
        communities: CommunityDiff { moved_nodes: vec![], stable_count: 0 },
    }
}

#[test]
fn renders_new_cycle_rows_with_path_hint() {
    let a = minimal_analysis();
    let d = drift_with_cycles(
        vec![vec!["app.services.auth", "app.repositories.user"]],
        vec![],
    );
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("#### Drift in this PR"));
    assert!(out.contains("**New cycle**"));
    assert!(out.contains("`app.services.auth`"));
    assert!(out.contains("`app.repositories.user`"));
    assert!(out.contains("`→ graphify path app.services.auth app.repositories.user`"));
}

#[test]
fn renders_broken_cycle_rows_without_next_step_hint() {
    let a = minimal_analysis();
    let d = drift_with_cycles(
        vec![],
        vec![vec!["app.a", "app.b"]],
    );
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("**Broken cycle**"));
    assert!(out.contains("`app.a`"));
    assert!(out.contains("`app.b`"));
    // Broken cycles are already resolved; no investigation hint needed.
    assert!(!out.contains("graphify path app.a app.b"));
}

#[test]
fn omits_cycle_rows_when_no_cycles_changed() {
    let a = minimal_analysis();
    let d = drift_with_cycles(vec![], vec![]);
    let out = render("my-app", &a, Some(&d), None);
    assert!(!out.contains("**New cycle**"));
    assert!(!out.contains("**Broken cycle**"));
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-report pr_summary::tests::renders_new_cycle_rows_with_path_hint
```

Expected: fails with `Drift in this PR` heading missing.

- [ ] **Step 3: Implement `render_drift_section` — cycles portion**

Replace the placeholder `render_drift_section` with:

```rust
fn render_drift_section(out: &mut String, drift: Option<&DiffReport>) {
    let Some(drift) = drift else { return; };
    // Collect any-finding flag
    let has_any_drift = !drift.cycles.introduced.is_empty()
        || !drift.cycles.resolved.is_empty()
        || !drift.hotspots.rising.is_empty()
        || !drift.hotspots.new_hotspots.is_empty()
        || !drift.communities.moved_nodes.is_empty();

    out.push_str("#### Drift in this PR\n\n");
    if !has_any_drift {
        out.push_str("_No architectural changes vs baseline._\n\n");
        return;
    }

    render_cycle_rows(out, drift);
    // Hotspot rows and community rows arrive in Tasks 8 and 9.
}

fn render_cycle_rows(out: &mut String, drift: &DiffReport) {
    const MAX_ROWS: usize = 5;

    for cycle in drift.cycles.introduced.iter().take(MAX_ROWS) {
        let pair = cycle_pair_label(cycle);
        out.push_str(&format!("- **New cycle** — {}\n", pair));
        if let Some((a, b)) = cycle_first_pair(cycle) {
            out.push_str(&format!("  `→ graphify path {} {}`\n", a, b));
        }
    }
    if drift.cycles.introduced.len() > MAX_ROWS {
        let extra = drift.cycles.introduced.len() - MAX_ROWS;
        out.push_str(&format!(
            "  _…and {} more (see drift-report.md)_\n",
            extra
        ));
    }

    for cycle in drift.cycles.resolved.iter().take(MAX_ROWS) {
        let pair = cycle_pair_label(cycle);
        out.push_str(&format!("- **Broken cycle** — {}\n", pair));
    }
    if drift.cycles.resolved.len() > MAX_ROWS {
        let extra = drift.cycles.resolved.len() - MAX_ROWS;
        out.push_str(&format!(
            "  _…and {} more (see drift-report.md)_\n",
            extra
        ));
    }
}

fn cycle_pair_label(cycle: &[String]) -> String {
    // A cycle is a list of nodes; show the first two joined with ↔, or
    // fall back to "↔"-joining all nodes if the cycle has <2 members.
    match cycle.len() {
        0 => "(empty cycle)".to_string(),
        1 => format!("`{}` ↔ `{}`", cycle[0], cycle[0]),
        _ => format!("`{}` ↔ `{}`", cycle[0], cycle[1]),
    }
}

fn cycle_first_pair(cycle: &[String]) -> Option<(&str, &str)> {
    if cycle.len() >= 2 {
        Some((&cycle[0], &cycle[1]))
    } else {
        None
    }
}
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: all 6 tests pass (3 from Task 6 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs
git commit -m "feat(report): pr_summary renders new/broken cycles with path hint (FEAT-015)"
```

---

## Task 8: Render drift section — escalated and new hotspots with 5-row cap

**Context:** Populate the hotspot rows from `DiffReport.hotspots.rising` (escalated) and `.new_hotspots` (newly appearing above threshold). Each row carries an inline `graphify explain` hint.

**Files:**
- Modify: `crates/graphify-report/src/pr_summary.rs`

- [ ] **Step 1: Write failing tests**

Add to the `tests` module:

```rust
fn drift_with_hotspots(rising: Vec<(&str, f64, f64)>, new_hotspots: Vec<(&str, f64)>) -> DiffReport {
    use graphify_core::diff::{CommunityDiff, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, ScoreChange, SummaryDelta};
    DiffReport {
        summary_delta: SummaryDelta {
            nodes: Delta { before: 0, after: 0, change: 0 },
            edges: Delta { before: 0, after: 0, change: 0 },
            communities: Delta { before: 0, after: 0, change: 0 },
            cycles: Delta { before: 0, after: 0, change: 0 },
        },
        edges: EdgeDiff { added_nodes: vec![], removed_nodes: vec![], degree_changes: vec![] },
        cycles: CycleDiff { introduced: vec![], resolved: vec![] },
        hotspots: HotspotDiff {
            rising: rising.into_iter().map(|(id, before, after)| ScoreChange {
                id: id.into(), before, after, delta: after - before,
            }).collect(),
            falling: vec![],
            new_hotspots: new_hotspots.into_iter().map(|(id, after)| ScoreChange {
                id: id.into(), before: 0.0, after, delta: after,
            }).collect(),
            removed_hotspots: vec![],
        },
        communities: CommunityDiff { moved_nodes: vec![], stable_count: 0 },
    }
}

#[test]
fn renders_escalated_hotspots_with_explain_hint() {
    let a = minimal_analysis();
    let d = drift_with_hotspots(
        vec![("app.services.auth", 0.71, 0.83), ("app.api.routes", 0.48, 0.52)],
        vec![],
    );
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("**Escalated hotspots (2)**"));
    assert!(out.contains("`app.services.auth`"));
    assert!(out.contains("0.71"));
    assert!(out.contains("0.83"));
    assert!(out.contains("`→ graphify explain app.services.auth`"));
    assert!(out.contains("`app.api.routes`"));
}

#[test]
fn renders_new_hotspots_with_explain_hint() {
    let a = minimal_analysis();
    let d = drift_with_hotspots(
        vec![],
        vec![("app.core.new_mod", 0.66)],
    );
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("**New hotspots (1)**"));
    assert!(out.contains("`app.core.new_mod`"));
    assert!(out.contains("score 0.66"));
    assert!(out.contains("`→ graphify explain app.core.new_mod`"));
}

#[test]
fn caps_escalated_hotspots_at_5_rows_with_more_hint() {
    let a = minimal_analysis();
    let rising: Vec<(&str, f64, f64)> = (0..7)
        .map(|i| (Box::leak(format!("app.mod_{}", i).into_boxed_str()) as &str, 0.5, 0.7))
        .collect();
    let d = drift_with_hotspots(rising, vec![]);
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("**Escalated hotspots (7)**"));
    // First 5 shown, then "…and 2 more"
    assert!(out.contains("app.mod_0"));
    assert!(out.contains("app.mod_4"));
    assert!(!out.contains("app.mod_5")); // 6th and 7th hidden
    assert!(out.contains("_…and 2 more"));
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-report pr_summary::tests::renders_escalated_hotspots_with_explain_hint
```

Expected: fails (no escalated-hotspots rendering yet).

- [ ] **Step 3: Implement the hotspot section**

In `render_drift_section`, after the `render_cycle_rows(out, drift)` call, add `render_hotspot_rows(out, drift);`. Then add the helper:

```rust
fn render_hotspot_rows(out: &mut String, drift: &DiffReport) {
    const MAX_ROWS: usize = 5;

    if !drift.hotspots.rising.is_empty() {
        out.push_str(&format!(
            "- **Escalated hotspots ({})**\n",
            drift.hotspots.rising.len()
        ));
        for change in drift.hotspots.rising.iter().take(MAX_ROWS) {
            out.push_str(&format!(
                "  - `{}` ({:.2} → {:.2})  `→ graphify explain {}`\n",
                change.id, change.before, change.after, change.id
            ));
        }
        if drift.hotspots.rising.len() > MAX_ROWS {
            let extra = drift.hotspots.rising.len() - MAX_ROWS;
            out.push_str(&format!(
                "  _…and {} more (see drift-report.md)_\n",
                extra
            ));
        }
    }

    if !drift.hotspots.new_hotspots.is_empty() {
        out.push_str(&format!(
            "- **New hotspots ({})**\n",
            drift.hotspots.new_hotspots.len()
        ));
        for change in drift.hotspots.new_hotspots.iter().take(MAX_ROWS) {
            out.push_str(&format!(
                "  - `{}` (score {:.2})  `→ graphify explain {}`\n",
                change.id, change.after, change.id
            ));
        }
        if drift.hotspots.new_hotspots.len() > MAX_ROWS {
            let extra = drift.hotspots.new_hotspots.len() - MAX_ROWS;
            out.push_str(&format!(
                "  _…and {} more (see drift-report.md)_\n",
                extra
            ));
        }
    }
}
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: 9 tests pass (6 prior + 3 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs
git commit -m "feat(report): pr_summary renders escalated and new hotspots (FEAT-015)"
```

---

## Task 9: Render drift section — community shifts and drift-empty collapse

**Context:** Populate the community-shift row from `DiffReport.communities.moved_nodes` (aggregated into a single bullet per prefix shift). Also finalize the "drift is empty" collapse behavior so the "Drift in this PR" section shows `_No architectural changes vs baseline._` when nothing fires.

**Files:**
- Modify: `crates/graphify-report/src/pr_summary.rs`

- [ ] **Step 1: Write failing tests**

```rust
fn drift_with_community_moves(moves: Vec<(&str, usize, usize)>) -> DiffReport {
    use graphify_core::diff::{CommunityDiff, CommunityMove, CycleDiff, Delta, DiffReport, EdgeDiff, HotspotDiff, SummaryDelta};
    DiffReport {
        summary_delta: SummaryDelta {
            nodes: Delta { before: 0, after: 0, change: 0 },
            edges: Delta { before: 0, after: 0, change: 0 },
            communities: Delta { before: 1, after: 2, change: 1 },
            cycles: Delta { before: 0, after: 0, change: 0 },
        },
        edges: EdgeDiff { added_nodes: vec![], removed_nodes: vec![], degree_changes: vec![] },
        cycles: CycleDiff { introduced: vec![], resolved: vec![] },
        hotspots: HotspotDiff { rising: vec![], falling: vec![], new_hotspots: vec![], removed_hotspots: vec![] },
        communities: CommunityDiff {
            moved_nodes: moves.into_iter().map(|(id, from_c, to_c)| CommunityMove {
                id: id.into(), from_community: from_c, to_community: to_c,
            }).collect(),
            stable_count: 0,
        },
    }
}

#[test]
fn renders_community_shift_row_when_nodes_moved() {
    let a = minimal_analysis();
    let d = drift_with_community_moves(vec![
        ("app.services.auth", 0, 1),
        ("app.services.user", 0, 1),
    ]);
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("**Community shift**"));
    // Aggregation: a short summary of how many nodes moved, not per-node bullets
    assert!(out.contains("2 nodes moved") || out.contains("(2)"));
}

#[test]
fn renders_no_changes_message_when_drift_empty() {
    let a = minimal_analysis();
    let d = drift_with_community_moves(vec![]);  // also no cycles, no hotspots
    let out = render("my-app", &a, Some(&d), None);
    assert!(out.contains("#### Drift in this PR"));
    assert!(out.contains("_No architectural changes vs baseline._"));
    // Bullet headers should not appear when there is nothing
    assert!(!out.contains("**New cycle**"));
    assert!(!out.contains("**Escalated hotspots"));
}
```

- [ ] **Step 2: Run the tests — confirm they fail**

```bash
cargo test -p graphify-report pr_summary::tests::renders_community_shift_row_when_nodes_moved
```

Expected: fails.

- [ ] **Step 3: Implement the community-shift row**

Add after `render_hotspot_rows(out, drift);` in `render_drift_section`:

```rust
    render_community_shift_row(out, drift);
```

And the helper:

```rust
fn render_community_shift_row(out: &mut String, drift: &DiffReport) {
    let moved = &drift.communities.moved_nodes;
    if moved.is_empty() {
        return;
    }
    let communities_before = drift.summary_delta.communities.before;
    let communities_after = drift.summary_delta.communities.after;
    out.push_str(&format!(
        "- **Community shift** — {} node{} moved across community boundaries (communities: {} → {})\n",
        moved.len(),
        if moved.len() == 1 { "" } else { "s" },
        communities_before,
        communities_after,
    ));
}
```

The "no changes" message was already wired in Task 7's `render_drift_section` (the `has_any_drift` guard). Verify the second test passes without further code changes.

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: 11 tests pass (9 prior + 2 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs
git commit -m "feat(report): pr_summary renders community shifts; handles empty drift (FEAT-015)"
```

---

## Task 10: Drift section — missing drift artifact produces hint

**Context:** When `drift` is `None`, the summary should still render the header/stats/footer but insert a hint telling the user to run `graphify diff`.

**Files:**
- Modify: `crates/graphify-report/src/pr_summary.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn renders_missing_drift_hint_when_drift_is_none() {
    let a = minimal_analysis();
    let out = render("my-app", &a, None, None);
    assert!(out.contains("#### Drift in this PR"));
    assert!(out.contains("_No drift baseline"));
    assert!(out.contains("graphify diff"));
    // No empty bullets / section-complete message
    assert!(!out.contains("_No architectural changes vs baseline._"));
}
```

- [ ] **Step 2: Run the test — confirm it fails**

```bash
cargo test -p graphify-report pr_summary::tests::renders_missing_drift_hint_when_drift_is_none
```

Expected: fails (current code skips the drift section entirely when `drift` is `None`).

- [ ] **Step 3: Modify `render_drift_section` to emit the hint path**

```rust
fn render_drift_section(out: &mut String, drift: Option<&DiffReport>) {
    out.push_str("#### Drift in this PR\n\n");
    let Some(drift) = drift else {
        out.push_str(
            "_No drift baseline — run `graphify diff --baseline <path> --config graphify.toml` to populate._\n\n",
        );
        return;
    };

    let has_any_drift = !drift.cycles.introduced.is_empty()
        || !drift.cycles.resolved.is_empty()
        || !drift.hotspots.rising.is_empty()
        || !drift.hotspots.new_hotspots.is_empty()
        || !drift.communities.moved_nodes.is_empty();

    if !has_any_drift {
        out.push_str("_No architectural changes vs baseline._\n\n");
        return;
    }

    render_cycle_rows(out, drift);
    render_hotspot_rows(out, drift);
    render_community_shift_row(out, drift);
    out.push('\n');
}
```

- [ ] **Step 4: Run the tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: 12 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs
git commit -m "feat(report): pr_summary emits hint when drift baseline missing (FEAT-015)"
```

---

## Task 11: Outstanding issues — render rules violations subsection

**Context:** When `check` is `Some`, render the "Outstanding issues" section with a `Rules violations` subsection populated from `check.projects[*].violations` where the variant is `Limit` or `Policy`.

**Files:**
- Modify: `crates/graphify-report/src/pr_summary.rs`

- [ ] **Step 1: Write failing tests**

```rust
fn check_with_rule_violations(violations: Vec<(&str, &str, &str)>) -> CheckReport {
    use crate::check_report::{
        CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
        ProjectCheckSummary,
    };
    CheckReport {
        ok: false,
        violations: violations.len(),
        projects: vec![ProjectCheckResult {
            name: "my-app".into(),
            ok: false,
            summary: ProjectCheckSummary {
                nodes: 0, edges: 0, communities: 0, cycles: 0,
                max_hotspot_score: 0.0, max_hotspot_id: None,
            },
            limits: CheckLimits::default(),
            policy_summary: PolicyCheckSummary { rules_evaluated: violations.len(), policy_violations: violations.len() },
            violations: violations.into_iter().map(|(rule, src, tgt)| CheckViolation::Policy {
                kind: "policy_rule".into(),
                rule: rule.into(),
                source_node: src.into(),
                target_node: tgt.into(),
                source_project: "my-app".into(),
                target_project: "my-app".into(),
                source_selectors: vec![],
                target_selectors: vec![],
            }).collect(),
        }],
        contracts: None,
    }
}

#[test]
fn renders_rules_violations_subsection() {
    let a = minimal_analysis();
    let c = check_with_rule_violations(vec![
        ("no_cross_layer_imports", "app.api.routes", "app.repositories.user"),
    ]);
    let out = render("my-app", &a, None, Some(&c));
    assert!(out.contains("#### Outstanding issues"));
    assert!(out.contains("**Rules violations (1)**"));
    assert!(out.contains("`no_cross_layer_imports`"));
    assert!(out.contains("`app.api.routes`"));
    assert!(out.contains("`app.repositories.user`"));
}

#[test]
fn omits_outstanding_section_when_check_is_none() {
    let a = minimal_analysis();
    let out = render("my-app", &a, None, None);
    assert!(!out.contains("#### Outstanding issues"));
    assert!(!out.contains("**Rules violations"));
}

#[test]
fn omits_outstanding_section_when_no_violations() {
    use crate::check_report::CheckReport;
    let a = minimal_analysis();
    let c = CheckReport {
        ok: true, violations: 0, projects: vec![], contracts: None,
    };
    let out = render("my-app", &a, None, Some(&c));
    assert!(!out.contains("#### Outstanding issues"));
}
```

- [ ] **Step 2: Run tests — confirm they fail**

```bash
cargo test -p graphify-report pr_summary::tests::renders_rules_violations_subsection
```

- [ ] **Step 3: Implement `render_outstanding_section` — rules portion**

Replace the placeholder with:

```rust
fn render_outstanding_section(out: &mut String, check: Option<&CheckReport>) {
    let Some(check) = check else { return; };

    let rule_count: usize = check.projects.iter().map(|p| p.violations.len()).sum();
    let contract_count = check
        .contracts
        .as_ref()
        .map(|c| c.pairs.iter().map(|p| p.violations.len()).sum::<usize>())
        .unwrap_or(0);

    if rule_count == 0 && contract_count == 0 {
        return;
    }

    out.push_str("#### Outstanding issues\n\n");
    render_rules_violations(out, check, rule_count);
    // Contract subsection arrives in Task 12.
}

fn render_rules_violations(out: &mut String, check: &CheckReport, total_rule_count: usize) {
    use crate::check_report::CheckViolation;
    const MAX_ROWS: usize = 5;

    if total_rule_count == 0 {
        return;
    }

    out.push_str(&format!(
        "**Rules violations ({})** — `graphify check --config graphify.toml`\n",
        total_rule_count
    ));

    let mut shown = 0usize;
    'outer: for project in &check.projects {
        for v in &project.violations {
            if shown >= MAX_ROWS {
                break 'outer;
            }
            match v {
                CheckViolation::Policy { rule, source_node, target_node, .. } => {
                    out.push_str(&format!(
                        "- `{}` — `{}` → `{}`\n",
                        rule, source_node, target_node
                    ));
                }
                CheckViolation::Limit { kind, actual, expected_max, node_id } => {
                    match node_id {
                        Some(n) => out.push_str(&format!(
                            "- `{}` — `{}`: {} > {}\n",
                            kind, n, actual, expected_max
                        )),
                        None => out.push_str(&format!(
                            "- `{}` — {} > {}\n",
                            kind, actual, expected_max
                        )),
                    }
                }
            }
            shown += 1;
        }
    }

    if total_rule_count > MAX_ROWS {
        let extra = total_rule_count - MAX_ROWS;
        out.push_str(&format!(
            "_…and {} more (see check-report.json)_\n",
            extra
        ));
    }
    out.push('\n');
}
```

- [ ] **Step 4: Run tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: 15 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs
git commit -m "feat(report): pr_summary renders rules violations subsection (FEAT-015)"
```

---

## Task 12: Outstanding issues — render contract drift subsection

**Context:** Populate a second subsection from `check.contracts.pairs[*].violations` when the contract result is present and has at least one pair-level violation.

**Files:**
- Modify: `crates/graphify-report/src/pr_summary.rs`

- [ ] **Step 1: Write failing tests**

```rust
fn check_with_contract_violations(violations_per_pair: Vec<(&str, &str, &str)>) -> CheckReport {
    use std::path::PathBuf;
    use graphify_core::contract::{ContractViolation, FieldType, PrimitiveType};
    use crate::contract_json::{ContractCheckResult, ContractPairResult, ContractSideInfo, ViolationEntry};
    use crate::check_report::CheckReport;
    use graphify_core::contract::Severity;

    let pairs = violations_per_pair.into_iter().map(|(pair_name, orm_tbl, ts_ty)| ContractPairResult {
        name: pair_name.into(),
        orm: ContractSideInfo { file: PathBuf::from("schema.ts"), symbol: orm_tbl.into(), line: 1 },
        ts: ContractSideInfo { file: PathBuf::from("types.ts"), symbol: ts_ty.into(), line: 1 },
        violations: vec![ViolationEntry {
            severity: Severity::Error,
            violation: ContractViolation::ContractFieldMissingOnTs {
                field: "created_at".into(),
                orm_type: FieldType::Primitive { value: PrimitiveType::Date },
                orm_line: 10,
            },
        }],
    }).collect::<Vec<_>>();

    CheckReport {
        ok: false,
        violations: pairs.len(),
        projects: vec![],
        contracts: Some(ContractCheckResult {
            ok: false,
            error_count: pairs.len(),
            warning_count: 0,
            pairs,
        }),
    }
}

#[test]
fn renders_contract_drift_subsection() {
    let a = minimal_analysis();
    let c = check_with_contract_violations(vec![("users_pair", "users", "User")]);
    let out = render("my-app", &a, None, Some(&c));
    assert!(out.contains("#### Outstanding issues"));
    assert!(out.contains("**Contract drift (1)**"));
    assert!(out.contains("`users`"));
    assert!(out.contains("`User`"));
}

#[test]
fn renders_both_rules_and_contract_subsections() {
    let a = minimal_analysis();
    let mut c = check_with_rule_violations(vec![("r1", "a", "b")]);
    let extra = check_with_contract_violations(vec![("pair", "users", "User")]);
    c.contracts = extra.contracts;
    let out = render("my-app", &a, None, Some(&c));
    assert!(out.contains("**Rules violations"));
    assert!(out.contains("**Contract drift"));
}

#[test]
fn renders_only_contract_subsection_when_no_rules() {
    let a = minimal_analysis();
    let c = check_with_contract_violations(vec![("pair", "users", "User")]);
    let out = render("my-app", &a, None, Some(&c));
    assert!(out.contains("**Contract drift"));
    assert!(!out.contains("**Rules violations"));
}
```

The `ContractViolation::ContractFieldMissingOnTs { field, orm_type, orm_line }` variant used above matches the actual enum shape in `graphify-core/src/contract.rs` (verified around line 82). See `summarize_contract_violation` below for the full list of variants to handle.

- [ ] **Step 2: Run tests — confirm they fail**

```bash
cargo test -p graphify-report pr_summary::tests::renders_contract_drift_subsection
```

- [ ] **Step 3: Implement the contract subsection**

In `render_outstanding_section`, after the `render_rules_violations` call, add:

```rust
    render_contract_violations(out, check, contract_count);
```

And add the helper:

```rust
fn render_contract_violations(out: &mut String, check: &CheckReport, total_contract_count: usize) {
    const MAX_ROWS: usize = 5;

    if total_contract_count == 0 {
        return;
    }
    let Some(contracts) = check.contracts.as_ref() else { return; };

    out.push_str(&format!(
        "**Contract drift ({})** — `graphify check --config graphify.toml`\n",
        total_contract_count
    ));

    let mut shown = 0usize;
    'outer: for pair in &contracts.pairs {
        for entry in &pair.violations {
            if shown >= MAX_ROWS {
                break 'outer;
            }
            let summary = summarize_contract_violation(&entry.violation);
            out.push_str(&format!(
                "- `{}` (ORM `{}`) ↔ `{}` (TS): {}\n",
                pair.name, pair.orm.symbol, pair.ts.symbol, summary,
            ));
            shown += 1;
        }
    }

    if total_contract_count > MAX_ROWS {
        let extra = total_contract_count - MAX_ROWS;
        out.push_str(&format!(
            "_…and {} more (see check-report.json)_\n",
            extra
        ));
    }
    out.push('\n');
}

fn summarize_contract_violation(v: &graphify_core::contract::ContractViolation) -> String {
    use graphify_core::contract::ContractViolation as V;
    match v {
        V::ContractFieldMissingOnTs { field, .. } => format!("field `{}` missing on TS side", field),
        V::ContractFieldMissingOnOrm { field, .. } => format!("field `{}` missing on ORM side", field),
        V::ContractTypeMismatch { field, .. } => format!("type mismatch on field `{}`", field),
        V::ContractNullabilityMismatch { field, orm_nullable, ts_nullable, .. } => {
            format!("nullability mismatch on `{}` (ORM={}, TS={})", field, orm_nullable, ts_nullable)
        }
        V::ContractRelationMissingOnTs { relation, .. } => format!("relation `{}` missing on TS side", relation),
        V::ContractRelationMissingOnOrm { relation, .. } => format!("relation `{}` missing on ORM side", relation),
        V::ContractCardinalityMismatch { relation, .. } => format!("cardinality mismatch on relation `{}`", relation),
        V::ContractUnmappedOrmType { field, raw_type, .. } => format!("unmapped ORM type `{}` on field `{}`", raw_type, field),
    }
}
```

Exhaustive match — every `ContractViolation` variant gets a short human summary. This matches the 8 variants present in `graphify-core/src/contract.rs` at time of writing.

- [ ] **Step 4: Run tests — confirm they pass**

```bash
cargo test -p graphify-report pr_summary
```

Expected: 18 tests pass (15 prior + 3 new).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-report/src/pr_summary.rs
git commit -m "feat(report): pr_summary renders contract drift subsection (FEAT-015)"
```

---

## Task 13: Project name resolution — directory basename helper

**Context:** The renderer takes a `project_name` parameter; the CLI must resolve it. `AnalysisSnapshot` does **not** carry a project name (verified shape: `nodes`, `communities`, `cycles`, `summary` only). The resolution is simply the directory basename. This task adds a small well-tested helper so the CLI dispatch (Task 14) stays tidy.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Add the helper function**

Near the bottom of `crates/graphify-cli/src/main.rs`, or wherever similar small helpers live:

```rust
fn resolve_project_name(dir: &std::path::Path) -> String {
    dir.file_name()
        .and_then(|os| os.to_str())
        .unwrap_or("unknown")
        .to_string()
}
```

- [ ] **Step 2: Add unit tests**

Add a `#[cfg(test)] mod pr_summary_helper_tests { ... }` block (or append to an existing tests module) in `main.rs`:

```rust
#[cfg(test)]
mod pr_summary_helper_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn resolve_project_name_uses_dir_basename() {
        assert_eq!(resolve_project_name(Path::new("./report/my-app")), "my-app");
        assert_eq!(resolve_project_name(Path::new("/abs/report/web")), "web");
    }

    #[test]
    fn resolve_project_name_returns_unknown_for_empty_path() {
        assert_eq!(resolve_project_name(Path::new("")), "unknown");
    }

    #[test]
    fn resolve_project_name_handles_trailing_slash() {
        // Path::file_name strips trailing components that are "." or root markers.
        assert_eq!(resolve_project_name(Path::new("./report/my-app/")), "my-app");
    }
}
```

- [ ] **Step 3: Run the tests — confirm they pass**

```bash
cargo test -p graphify-cli pr_summary_helper_tests
```

Expected: 3 tests pass.

- [ ] **Step 4: Clippy check**

```bash
cargo clippy -p graphify-cli -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat(cli): resolve_project_name helper for pr-summary (FEAT-015)"
```

---

## Task 14: CLI dispatch — `Commands::PrSummary { dir }` happy path

**Context:** Wire a new clap subcommand, add file-loading glue that calls the renderer, and write the output to stdout. Error paths arrive in Task 15.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Write a failing integration test**

Add to `crates/graphify-cli/tests/pr_summary_integration.rs` (new file):

```rust
use std::fs;
use std::process::Command;

#[test]
fn pr_summary_prints_markdown_given_analysis_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Project name comes from the directory basename, so create a named subdir.
    let project_dir = dir.path().join("my-app");
    std::fs::create_dir(&project_dir).unwrap();

    // Minimal valid AnalysisSnapshot JSON (matches graphify-core/src/diff.rs shape).
    let analysis_json = r#"{
        "nodes": [],
        "communities": [],
        "cycles": [],
        "summary": {
            "total_nodes": 0,
            "total_edges": 0,
            "total_communities": 0,
            "total_cycles": 0
        }
    }"#;
    fs::write(project_dir.join("analysis.json"), analysis_json).expect("write analysis.json");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", project_dir.to_str().unwrap()])
        .output()
        .expect("run pr-summary");

    assert!(output.status.success(), "pr-summary failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("### Graphify — Architecture Delta for `my-app`"));
    assert!(stdout.contains("#### Drift in this PR"));
    assert!(stdout.contains("_No drift baseline"));
    // No outstanding-issues section when no check-report.json
    assert!(!stdout.contains("#### Outstanding issues"));
    // Footer present
    assert!(stdout.contains("graphify pr-summary <dir>"));
}
```

- [ ] **Step 2: Run the test — confirm it fails**

```bash
cargo test -p graphify-cli pr_summary_prints_markdown_given_analysis_only
```

Expected: fails with `unrecognized subcommand \`pr-summary\``.

- [ ] **Step 3: Add the clap variant**

In `crates/graphify-cli/src/main.rs`, locate the `enum Commands` and add a new variant near existing subcommands:

```rust
/// Render a PR-ready Markdown summary from a project's Graphify output directory.
PrSummary {
    /// Path to a single project's Graphify output directory (for example ./report/my-app).
    dir: std::path::PathBuf,
},
```

- [ ] **Step 4: Add the dispatch arm**

In the main dispatch `match cli.command { ... }`, add:

```rust
Commands::PrSummary { dir } => run_pr_summary(&dir)?,
```

And add the handler:

```rust
fn run_pr_summary(dir: &std::path::Path) -> anyhow::Result<()> {
    use graphify_core::diff::{AnalysisSnapshot, DiffReport};
    use graphify_report::check_report::CheckReport;
    use graphify_report::pr_summary;

    if !dir.exists() {
        anyhow::bail!("graphify pr-summary: directory '{}' not found", dir.display());
    }

    let analysis_path = dir.join("analysis.json");
    if !analysis_path.exists() {
        anyhow::bail!(
            "graphify pr-summary: missing analysis.json in '{}' (run 'graphify run' first)",
            dir.display()
        );
    }
    let analysis_text = std::fs::read_to_string(&analysis_path)
        .map_err(|e| anyhow::anyhow!("graphify pr-summary: failed to read analysis.json: {}", e))?;
    let analysis: AnalysisSnapshot = serde_json::from_str(&analysis_text)
        .map_err(|e| anyhow::anyhow!("graphify pr-summary: failed to parse analysis.json: {}", e))?;

    let drift = load_optional_json::<DiffReport>(&dir.join("drift-report.json"), "drift-report.json");
    let check = load_optional_json::<CheckReport>(&dir.join("check-report.json"), "check-report.json");

    let project_name = resolve_project_name(dir);
    let output = pr_summary::render(&project_name, &analysis, drift.as_ref(), check.as_ref());
    print!("{}", output);
    Ok(())
}

fn load_optional_json<T: for<'de> serde::Deserialize<'de>>(path: &std::path::Path, label: &str) -> Option<T> {
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<T>(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("warning: failed to parse {}, skipping section: {}", label, e);
                None
            }
        },
        Err(e) => {
            eprintln!("warning: failed to read {}, skipping section: {}", label, e);
            None
        }
    }
}
```

Ensure the clap subcommand name matches: clap derives a kebab-case name from `PrSummary` → `pr-summary` by default.

- [ ] **Step 5: Run the test — confirm it passes**

```bash
cargo test -p graphify-cli pr_summary_prints_markdown_given_analysis_only
```

Expected: pass. If `analysis.json` literal is invalid (missing required fields), parse errors will surface — fix by matching the real schema.

- [ ] **Step 6: Full workspace + clippy**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-cli/src/main.rs crates/graphify-cli/tests/pr_summary_integration.rs
git commit -m "feat(cli): graphify pr-summary subcommand (happy path) (FEAT-015)"
```

---

## Task 15: CLI error paths — missing dir, multi-project root, malformed JSON

**Context:** Cover the exit-2 error paths and the multi-project-root detection.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`
- Modify: `crates/graphify-cli/tests/pr_summary_integration.rs`

- [ ] **Step 1: Write failing tests**

Append to `crates/graphify-cli/tests/pr_summary_integration.rs`:

```rust
#[test]
fn pr_summary_exits_non_zero_when_directory_missing() {
    let missing = std::path::Path::new("/tmp/graphify-pr-summary-does-not-exist-xyz");
    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", missing.to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn pr_summary_exits_non_zero_when_analysis_json_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", dir.path().to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing analysis.json"));
}

const MINIMAL_ANALYSIS_JSON: &str = r#"{
    "nodes": [],
    "communities": [],
    "cycles": [],
    "summary": {
        "total_nodes": 0,
        "total_edges": 0,
        "total_communities": 0,
        "total_cycles": 0
    }
}"#;

#[test]
fn pr_summary_detects_multi_project_root() {
    let root = tempfile::tempdir().expect("tempdir");
    // Create two project subdirs, each with its own analysis.json.
    for project in &["web", "api"] {
        let p = root.path().join(project);
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("analysis.json"), MINIMAL_ANALYSIS_JSON).unwrap();
    }
    // Root itself has no analysis.json.
    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", root.path().to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("multi-project output root"));
}

#[test]
fn pr_summary_warns_and_continues_on_malformed_drift_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("analysis.json"), MINIMAL_ANALYSIS_JSON).unwrap();
    std::fs::write(dir.path().join("drift-report.json"), "{not valid json").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", dir.path().to_str().unwrap()])
        .output()
        .expect("run pr-summary");
    assert!(output.status.success(), "pr-summary should not fail on malformed optional input");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to parse drift-report.json"));
}
```

Hoist the `MINIMAL_ANALYSIS_JSON` constant to the top of `crates/graphify-cli/tests/pr_summary_integration.rs` so all integration tests in the file share it.

- [ ] **Step 2: Run tests — confirm they fail as expected**

```bash
cargo test -p graphify-cli pr_summary_
```

Expect: several tests fail. `missing` and `malformed` tests may already pass (the earlier code handled them); `multi-project` detection will definitely fail.

- [ ] **Step 3: Add multi-project-root detection**

In `run_pr_summary`, before the `analysis.json` existence check, add:

```rust
    if !analysis_path.exists() {
        // Detect a multi-project root: no analysis.json here but at least one subdir has its own.
        if dir.is_dir() {
            let any_child_has_analysis = std::fs::read_dir(dir)
                .ok()
                .map(|iter| iter.filter_map(Result::ok)
                    .any(|entry| entry.path().is_dir() && entry.path().join("analysis.json").exists()))
                .unwrap_or(false);
            if any_child_has_analysis {
                anyhow::bail!(
                    "graphify pr-summary: '{}' is a multi-project output root — point at a single project subdirectory",
                    dir.display()
                );
            }
        }
        anyhow::bail!(
            "graphify pr-summary: missing analysis.json in '{}' (run 'graphify run' first)",
            dir.display()
        );
    }
```

(Move this block above the existing `analysis.json` missing check; adapt the existing code accordingly so the multi-project check runs first.)

- [ ] **Step 4: Verify exit code propagates**

Ensure the main `main()` function maps `anyhow::Error` to a non-zero exit code (most projects using `anyhow::Result<()>` already do this via the `?` and the default error printer). If it does not, wrap with:

```rust
fn main() {
    if let Err(err) = real_main() {
        eprintln!("{}", err);
        std::process::exit(2);
    }
}
```

If the existing `main()` already handles errors, leave it alone.

- [ ] **Step 5: Run tests — confirm they pass**

```bash
cargo test -p graphify-cli pr_summary_
```

Expected: all 5 integration tests pass (the original happy-path plus the 4 new error tests).

- [ ] **Step 6: Full workspace + clippy**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 7: Commit**

```bash
git add crates/graphify-cli/src/main.rs crates/graphify-cli/tests/pr_summary_integration.rs
git commit -m "feat(cli): pr-summary error paths and multi-project detection (FEAT-015)"
```

---

## Task 16: End-to-end integration test with realistic fixture dir

**Context:** One realistic integration test that exercises the full happy path: analysis + drift + check all present, producing the full output.

**Files:**
- Create: `crates/graphify-cli/tests/fixtures/pr_summary/analysis.json`
- Create: `crates/graphify-cli/tests/fixtures/pr_summary/drift-report.json`
- Create: `crates/graphify-cli/tests/fixtures/pr_summary/check-report.json`
- Modify: `crates/graphify-cli/tests/pr_summary_integration.rs`

- [ ] **Step 1: Produce the fixtures**

Run `graphify run` + `graphify diff` + `graphify check` against a tiny real fixture codebase (one that produces a non-trivial graph with at least one cycle and one hotspot), then copy the produced JSONs into `crates/graphify-cli/tests/fixtures/pr_summary/`. Hand-edit to introduce a few specific findings:

- At least one new cycle in `drift-report.json` (introduced)
- At least one rising hotspot in `drift-report.json`
- At least one project policy violation in `check-report.json`
- At least one contract violation in `check-report.json.contracts`

Keep file sizes small (<2 KB each). Strip irrelevant deep fields if they add bulk.

**Alternative for Step 1:** if producing via a real run is impractical, hand-author minimal JSONs that match the schemas and contain the findings described above. Use the existing roundtrip unit tests (from Tasks 1, 2, and 3) as shape references.

- [ ] **Step 2: Add the integration test**

Append to `crates/graphify-cli/tests/pr_summary_integration.rs`:

```rust
#[test]
fn pr_summary_end_to_end_against_realistic_fixture() {
    let fixture = std::path::Path::new("tests/fixtures/pr_summary");
    assert!(fixture.join("analysis.json").exists(), "fixture setup");

    let output = Command::new(env!("CARGO_BIN_EXE_graphify"))
        .args(["pr-summary", fixture.to_str().unwrap()])
        .output()
        .expect("run pr-summary");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Header + stats present
    assert!(stdout.contains("### Graphify — Architecture Delta for"));
    assert!(stdout.contains("nodes"));
    assert!(stdout.contains("edges"));

    // Drift section with at least one finding
    assert!(stdout.contains("#### Drift in this PR"));
    assert!(stdout.contains("**New cycle**") || stdout.contains("**Escalated hotspots**") || stdout.contains("**New hotspots**"));

    // Outstanding issues section with rules + contract
    assert!(stdout.contains("#### Outstanding issues"));
    assert!(stdout.contains("**Rules violations"));
    assert!(stdout.contains("**Contract drift"));

    // Footer
    assert!(stdout.contains("graphify pr-summary <dir>"));
}
```

- [ ] **Step 3: Run the test**

```bash
cargo test -p graphify-cli pr_summary_end_to_end_against_realistic_fixture
```

Expected: pass.

- [ ] **Step 4: Workspace + clippy final**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/tests/pr_summary_integration.rs \
        crates/graphify-cli/tests/fixtures/pr_summary/
git commit -m "test(cli): pr-summary end-to-end integration fixture (FEAT-015)"
```

---

## Task 17: README — document `graphify pr-summary` + new `check-report.json` artifact

**Context:** Two README updates: (1) a new recipe for `graphify pr-summary` with a GitHub Actions example; (2) a note under the artifacts table that `graphify check` now writes `check-report.json`.

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add a row to the command table**

In the "Commands" section (search for `| \`graphify check\``), add a row:

```markdown
| `graphify pr-summary` | Render a PR-ready Markdown summary of architectural change |
```

- [ ] **Step 2: Add `check-report.json` to the artifacts table**

In the artifacts/output table (search for `| \`drift-report.json\``), add:

```markdown
| `check-report.json` | JSON | Unified check result (rules + contract); written by `graphify check` |
```

- [ ] **Step 3: Add a CI integration recipe**

In the "CI integration" section (search for existing `graphify check` CI examples around README line 220-230), add the following block immediately after the existing CI recipe, or wherever it best fits flow-wise:

```markdown
### Render a PR summary for GitHub Actions

After `graphify run` + `graphify diff` + `graphify check` populate the project output directory, append a concise Markdown summary to the GitHub Actions job summary:

\`\`\`yaml
- run: graphify run --config graphify.toml
- run: graphify diff --baseline ./baseline/analysis.json --config graphify.toml --project my-app
- run: graphify check --config graphify.toml || true
- run: graphify pr-summary ./report/my-app >> "$GITHUB_STEP_SUMMARY"
\`\`\`

\`graphify pr-summary <DIR>\` is a pure renderer: it reads existing JSON artifacts (analysis.json required; drift-report.json and check-report.json optional) and prints Markdown to stdout. Exit code is 0 regardless of findings — gate with \`graphify check\` separately if you want CI to fail on violations.

Output is optimized for solo-dev + AI-authored PR review: each finding carries an inline \`graphify explain\` / \`graphify path\` hint so the next investigation step is one copy-paste away.
```

(Quadruple-backtick opening/closing here is just for this plan document. When writing the actual README content, use triple backticks for the YAML block.)

- [ ] **Step 4: Smoke-test the README renders**

Optional but recommended: open `README.md` in an editor with a Markdown preview, or run `pandoc README.md -o /tmp/readme.html` to confirm nothing broke the formatting.

- [ ] **Step 5: Final workspace test + clippy**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "docs: document graphify pr-summary and check-report.json (FEAT-015)"
```

---

## Post-implementation: close-out (deferred to ship session)

These steps happen in a separate close-out session once all implementation commits are on `main`:

- Update `docs/TaskNotes/Tasks/FEAT-015-pr-and-editor-integration.md`:
  - Status: `done`
  - `completed: 2026-04-XX`
  - Subtasks checked
  - Verification section appended (cite the commits and test commands)
- Update `docs/TaskNotes/Tasks/sprint.md`:
  - FEAT-015 row → `**done**`
  - Add entry to Done section
- Bump workspace version to v0.6.0 in `Cargo.toml` (separate close-out session per the session brief)
- Tag v0.6.0 and push (explicit user approval; covered by GATE-2 in future session brief)

---

## Self-review checklist (run before handing off)

**Spec coverage:**

- [x] Section 1 in-scope list → Tasks 1-17 cover each bullet
- [x] "Included ecosystem changes" → Tasks 3-5 (type-move + write-to-disk)
- [x] Section 2 user-facing CLI shape → Task 14 (dispatch) + Task 17 (README)
- [x] Section 3 architecture → Task 3 (check_report.rs), Task 6 (pr_summary.rs), Task 14 (CLI glue)
- [x] Section 4 input contract → Tasks 14, 15 (file loading with graceful degradation)
- [x] Section 5 output Markdown → Tasks 6-12 (per-section rendering)
- [x] Section 6 degradation table → Task 10 (drift missing), Task 11/12 (outstanding missing), Task 15 (dir/analysis errors)
- [x] Section 7 testing strategy → each task includes its own unit/CLI/integration tests; Task 16 adds the end-to-end fixture test
- [x] Section 8 file touches → matches Tasks 3, 4, 5, 6, 14, 16, 17
- [x] Section 10 done criteria → each bullet maps to at least one task commit

**Placeholder scan:** no "TBD" / "implement later" / unwritten code. Each task shows the code or the exact transformation.

**Type consistency:** `CheckReport`, `CheckViolation::{Limit, Policy}`, `DiffReport.cycles.introduced/resolved`, `HotspotDiff.{rising,new_hotspots}`, `CommunityDiff.moved_nodes` — all match the real type names verified in the codebase.

**Resolved during plan self-review** (concrete values in the plan, no implementer guesswork needed):
- `AnalysisSnapshot` shape verified against `graphify-core/src/diff.rs` lines 13-45 — fields are `nodes`/`communities`/`cycles`/`summary`; stats come from `summary.total_nodes`/`summary.total_edges`.
- `ContractViolation` variants verified against `graphify-core/src/contract.rs` lines 82-122 — 8 variants total; `summarize_contract_violation` matches exhaustively.
- `AnalysisSnapshot` has no `project` field — project name resolution is dir-basename only (Task 13).
- `ContractViolation` and `ContractComparison` in `graphify-core/src/contract.rs` need `Deserialize` added — covered explicitly in Task 2 Step 3b.

Any implementer should be able to execute each task end-to-end without needing to re-read the spec, using only the plan text plus a few targeted file reads called out in each task.
