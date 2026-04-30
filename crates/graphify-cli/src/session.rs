//! `graphify session brief` / `graphify session scope` (FEAT-042).
//!
//! Native replacement for two project-local bash scripts
//! (`gf-context-brief.sh` and `gf-context-scope.sh`) maintained by every
//! consumer to drive Claude Code session context. The schema is
//! graphify-owned; consumers (cursos, nymos, ordo, …) read it via
//! `/session-start` skills and `tn-session-dispatcher` subagents.
//!
//! Schema bump from the bash version (`schema_version: 1`) to `2`:
//!
//! - `frozen[]` is dropped — that list is consumer-specific (mirrors each
//!   project's `CLAUDE.md` frozen-modules section), not graphify concern.
//!   Consumers append it post-hoc via `jq` if they want.
//! - `scope_explains[].explain` is now a structured JSON object (the full
//!   `explain` report from the query engine) instead of a 40-line text blob.
//!   Richer for tooling; consumers that want plain text can still call
//!   `graphify explain <node>` directly.
//! - `scope` no longer auto-resolves the active `tn` task. The caller passes
//!   `--files <a,b,c>` explicitly so graphify never depends on tasknotes-cli.
//!
//! See `apps/tasknotes-cli/docs/TaskNotes/Tasks/FEAT-042-*.md` (originating
//! task) for the full design discussion.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA_VERSION: u32 = 2;

// ---------------------------------------------------------------------------
// Brief — schema
// ---------------------------------------------------------------------------

/// Top-N hotspot record. Mirrors the bash output, minus the `frozen` flag
/// (consumer concern).
#[derive(Debug, Serialize)]
struct Hotspot {
    project: String,
    id: String,
    score: f64,
    in_degree: u64,
    out_degree: u64,
    in_cycle: bool,
    hotspot_type: Value,
}

#[derive(Debug, Serialize)]
struct CycleEntry {
    project: String,
    cycle: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Brief {
    schema_version: u32,
    generated_at: String,
    graphify_version: String,
    /// `null` when no `report/baseline/` directory exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_age_days: Option<i64>,
    /// `true` when the baseline is older than `--stale-days`. Always present
    /// (defaults to `false`) so consumers can branch without null-checks.
    stale: bool,
    projects: Vec<String>,
    hotspots: Vec<Value>,
    cycles: Vec<Value>,
    /// Filled by `graphify session scope`; empty here.
    scope_files: Vec<String>,
    /// Filled by `graphify session scope`; empty here.
    scope_explains: Vec<Value>,
    /// Optional, populated only when `scope` was run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope_task: Option<String>,
}

// ---------------------------------------------------------------------------
// Brief — entry points
// ---------------------------------------------------------------------------

/// Options for `graphify session brief`. Mirrors the bash flags one-to-one.
pub struct BriefOpts {
    pub project_names: Vec<String>,
    pub output_root: PathBuf,
    pub out_path: PathBuf,
    pub top: usize,
    pub stale_days: i64,
    pub force: bool,
    pub check: bool,
}

/// Top-level handler for `graphify session brief`. Returns the brief on
/// success (caller prints / writes / exits per opts).
///
/// # Errors
/// - `output_root` missing or empty.
/// - Failure to write the output file (when not in `--check` mode).
pub fn run_brief(opts: &BriefOpts) -> Result<i32> {
    if opts.project_names.is_empty() {
        bail!("graphify.toml has no [[project]] entries");
    }

    // Cache check: if every analysis.json predates the existing brief and
    // --force is off, skip regeneration.
    let needs_regen = brief_needs_regen(opts);

    if opts.check {
        // --check mode: 0 = fresh, 2 = stale (matches bash semantics so
        // CI / pre-flight scripts can branch on exit code).
        return Ok(if needs_regen { 2 } else { 0 });
    }

    if !needs_regen && !opts.force {
        eprintln!(
            "[graphify session brief] cache fresh: {} (use --force to regenerate)",
            opts.out_path.display()
        );
        return Ok(0);
    }

    let brief = build_brief(opts)?;
    write_brief(&brief, &opts.out_path)
        .with_context(|| format!("failed to write {}", opts.out_path.display()))?;
    eprintln!(
        "[graphify session brief] wrote {} (projects={}, hotspots={}, cycles={}, stale={})",
        opts.out_path.display(),
        brief.projects.len(),
        brief.hotspots.len(),
        brief.cycles.len(),
        brief.stale
    );
    Ok(0)
}

fn brief_needs_regen(opts: &BriefOpts) -> bool {
    if !opts.out_path.exists() {
        return true;
    }
    let Ok(out_mtime) = fs_mtime(&opts.out_path) else {
        return true;
    };
    for name in &opts.project_names {
        let aj = analysis_path(&opts.output_root, name);
        if let Ok(aj_mtime) = fs_mtime(&aj) {
            if aj_mtime > out_mtime {
                return true;
            }
        }
    }
    false
}

fn build_brief(opts: &BriefOpts) -> Result<Brief> {
    let baseline_age_days = baseline_age_days(&opts.output_root);
    let stale = baseline_age_days.is_some_and(|d| d > opts.stale_days);
    if stale {
        eprintln!(
            "[graphify session brief] WARN: baseline is {}d old (>{}d). Run `graphify analyze`.",
            baseline_age_days.unwrap_or(0),
            opts.stale_days
        );
    }

    let mut hotspots: Vec<Hotspot> = Vec::new();
    let mut cycles: Vec<CycleEntry> = Vec::new();

    for name in &opts.project_names {
        let aj = analysis_path(&opts.output_root, name);
        if !aj.is_file() {
            eprintln!(
                "[graphify session brief] skip {} (no {})",
                name,
                aj.display()
            );
            continue;
        }
        let raw = std::fs::read_to_string(&aj)
            .with_context(|| format!("failed to read {}", aj.display()))?;
        let parsed: Value = serde_json::from_str(&raw)
            .with_context(|| format!("invalid JSON in {}", aj.display()))?;

        if let Some(arr) = parsed.get("nodes").and_then(Value::as_array) {
            for node in arr {
                if let Some(h) = parse_hotspot(node, name) {
                    hotspots.push(h);
                }
            }
        }
        if let Some(arr) = parsed.get("cycles").and_then(Value::as_array) {
            for c in arr {
                if let Some(cycle_vec) = parse_cycle(c) {
                    cycles.push(CycleEntry {
                        project: name.clone(),
                        cycle: cycle_vec,
                    });
                }
            }
        }
    }

    // Sort hotspots by score desc, trim to top.
    hotspots.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hotspots.truncate(opts.top);

    let hotspots_json: Vec<Value> = hotspots
        .into_iter()
        .map(|h| serde_json::to_value(h).expect("Hotspot serializable"))
        .collect();
    let cycles_json: Vec<Value> = cycles
        .into_iter()
        .map(|c| serde_json::to_value(c).expect("CycleEntry serializable"))
        .collect();

    Ok(Brief {
        schema_version: SCHEMA_VERSION,
        generated_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        graphify_version: env!("CARGO_PKG_VERSION").to_string(),
        baseline_age_days,
        stale,
        projects: opts.project_names.clone(),
        hotspots: hotspots_json,
        cycles: cycles_json,
        scope_files: Vec::new(),
        scope_explains: Vec::new(),
        scope_task: None,
    })
}

fn parse_hotspot(node: &Value, project: &str) -> Option<Hotspot> {
    let id = node.get("id")?.as_str()?.to_string();
    let score = node.get("score")?.as_f64()?;
    let in_degree = node.get("in_degree")?.as_u64()?;
    let out_degree = node.get("out_degree")?.as_u64()?;
    let in_cycle = node.get("in_cycle")?.as_bool()?;
    // hotspot_type may be a string or an object — pass through as-is.
    let hotspot_type = node.get("hotspot_type").cloned().unwrap_or(Value::Null);
    Some(Hotspot {
        project: project.to_string(),
        id,
        score,
        in_degree,
        out_degree,
        in_cycle,
        hotspot_type,
    })
}

fn parse_cycle(c: &Value) -> Option<Vec<String>> {
    // analysis.json `cycles[]` items are arrays of node ids in the bash
    // version, but the typed `Cycle` struct in graphify-report adds an
    // `edges` field. Accept either shape: pure array, or `{nodes: [...]}`.
    if let Some(arr) = c.as_array() {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        );
    }
    if let Some(arr) = c.get("nodes").and_then(Value::as_array) {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        );
    }
    None
}

fn analysis_path(output_root: &Path, project_name: &str) -> PathBuf {
    output_root.join(project_name).join("analysis.json")
}

fn fs_mtime(p: &Path) -> Result<SystemTime> {
    Ok(std::fs::metadata(p)?.modified()?)
}

/// Age in days of the freshest `analysis.json` under `report/baseline/`.
///
/// Returns `None` when no `analysis.json` is found (no baseline ever
/// promoted) — that's the no-warn path.
///
/// **Why not the directory mtime?** POSIX directory mtimes only update on
/// entry add/remove. Overwriting an existing file in place leaves the dir
/// mtime untouched, so the dir-mtime approach (used pre-0.13.7) reported
/// the age of the dir's *first creation*, not the freshness of its
/// contents — even after a fresh `cp report/<proj>/analysis.json
/// report/baseline/analysis.json`. See GH issue #15 / BUG-028.
///
/// **Order of preference, per file:**
/// 1. Top-level `generated_at` field (ISO 8601 UTC, written by 0.13.7+).
/// 2. The file's own mtime (legacy compat for snapshots written by 0.13.6
///    or earlier).
///
/// **Multi-baseline:** scans `baseline/` and `baseline/*/` (depth ≤ 1) so
/// both single-project (`baseline/analysis.json`) and multi-project
/// (`baseline/<proj>/analysis.json`) layouts work. Returns the smallest
/// age across all found files — the youngest baseline wins, matching
/// the operator's mental model after a batch promotion.
fn baseline_age_days(output_root: &Path) -> Option<i64> {
    let baseline_root = output_root.join("baseline");
    if !baseline_root.is_dir() {
        return None;
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    let direct = baseline_root.join("analysis.json");
    if direct.is_file() {
        candidates.push(direct);
    }
    if let Ok(entries) = std::fs::read_dir(&baseline_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let nested = path.join("analysis.json");
                if nested.is_file() {
                    candidates.push(nested);
                }
            }
        }
    }

    let now = SystemTime::now();
    candidates
        .iter()
        .filter_map(|path| analysis_file_age_days(path, now))
        .min()
}

/// Age in days of a single `analysis.json` artifact, preferring the
/// embedded `generated_at` field over the file's mtime.
fn analysis_file_age_days(path: &Path, now: SystemTime) -> Option<i64> {
    if let Ok(raw) = std::fs::read_to_string(path) {
        if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
            if let Some(ts) = parsed.get("generated_at").and_then(Value::as_str) {
                if let Some(age) = parse_iso8601_age_days(ts, now) {
                    return Some(age);
                }
            }
        }
    }
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let elapsed = now.duration_since(mtime).ok()?;
    Some((elapsed.as_secs() / 86_400) as i64)
}

/// Parse an ISO 8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) into an age in
/// days relative to `now`. Returns `None` on parse failure or future
/// timestamps (which would be negative — caller treats as "absent" so the
/// fallback can take over).
fn parse_iso8601_age_days(ts: &str, now: SystemTime) -> Option<i64> {
    use chrono::{DateTime, Utc};
    let parsed: DateTime<Utc> = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    let parsed_st: SystemTime = parsed.into();
    let elapsed = now.duration_since(parsed_st).ok()?;
    Some((elapsed.as_secs() / 86_400) as i64)
}

fn write_brief(brief: &Brief, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(brief)?;
    std::fs::write(path, json)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Scope — entry point
// ---------------------------------------------------------------------------

/// Options for `graphify session scope`. The caller passes file paths
/// explicitly via `--files`; graphify never reaches into a `tn` task body
/// (that coupling stays out of the binary — see open question #2 in the
/// originating FEAT-042 task body).
pub struct ScopeOpts {
    pub files: Vec<String>,
    pub max: usize,
    pub in_path: PathBuf,
    pub task: Option<String>,
}

/// Merge `scope_files` + `scope_explains` into the brief at `opts.in_path`.
///
/// `explain` is computed by the caller (`graphify-cli` owns the query
/// engine, so this module accepts a closure rather than reaching into the
/// engine itself — keeps the module unit-testable without a graph fixture).
///
/// # Errors
/// - The brief at `opts.in_path` is missing or malformed.
pub fn run_scope<F>(opts: &ScopeOpts, mut explain: F) -> Result<i32>
where
    F: FnMut(&str) -> Option<Value>,
{
    let raw = std::fs::read_to_string(&opts.in_path).with_context(|| {
        format!(
            "{} missing — run `graphify session brief` first",
            opts.in_path.display()
        )
    })?;
    let mut brief: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid JSON in {}", opts.in_path.display()))?;

    let files: Vec<String> = opts
        .files
        .iter()
        .filter(|f| !f.is_empty())
        .take(opts.max)
        .cloned()
        .collect();

    if files.is_empty() {
        eprintln!("[graphify session scope] no files supplied — leaving scope empty");
        return Ok(0);
    }

    let mut explains: Vec<Value> = Vec::with_capacity(files.len());
    for f in &files {
        let report = explain(f);
        let entry = serde_json::json!({
            "file": f,
            "explain": report.unwrap_or_else(|| Value::String("(no explain)".into())),
        });
        explains.push(entry);
    }

    let obj = brief
        .as_object_mut()
        .ok_or_else(|| anyhow!("brief is not a JSON object"))?;
    obj.insert("scope_files".into(), serde_json::to_value(&files)?);
    obj.insert("scope_explains".into(), serde_json::to_value(&explains)?);
    if let Some(task) = &opts.task {
        obj.insert("scope_task".into(), Value::String(task.clone()));
    }

    let json = serde_json::to_string_pretty(&brief)?;
    std::fs::write(&opts.in_path, json)?;
    eprintln!(
        "[graphify session scope] merged scope (files={}{})",
        files.len(),
        opts.task
            .as_deref()
            .map_or(String::new(), |t| format!(", task={t}"))
    );
    Ok(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn write_analysis(root: &Path, name: &str, body: Value) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("analysis.json"),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();
    }

    fn analysis_with_nodes(scores: &[(&str, f64)]) -> Value {
        let nodes: Vec<Value> = scores
            .iter()
            .map(|(id, score)| {
                json!({
                    "id": id,
                    "betweenness": 0.0,
                    "pagerank": 0.0,
                    "in_degree": 1,
                    "out_degree": 0,
                    "in_cycle": false,
                    "score": score,
                    "community_id": 0,
                    "hotspot_type": "Hub"
                })
            })
            .collect();
        json!({
            "nodes": nodes,
            "edges": [],
            "communities": [],
            "cycles": [["a", "b", "a"]],
            "summary": {
                "total_nodes": 0,
                "total_edges": 0,
                "total_communities": 0,
                "total_cycles": 1,
                "top_hotspots": []
            },
            "confidence_summary": {
                "extracted_count": 0,
                "extracted_pct": 0.0,
                "inferred_count": 0,
                "inferred_pct": 0.0,
                "ambiguous_count": 0,
                "ambiguous_pct": 0.0,
                "expected_external_count": 0,
                "expected_external_pct": 0.0,
                "mean_confidence": 1.0
            }
        })
    }

    #[test]
    fn brief_collects_top_hotspots_across_projects() {
        let root = TempDir::new().unwrap();
        let report = root.path().join("report");
        write_analysis(
            &report,
            "web",
            analysis_with_nodes(&[("web.a", 0.9), ("web.b", 0.4)]),
        );
        write_analysis(
            &report,
            "api",
            analysis_with_nodes(&[("api.a", 0.7), ("api.b", 0.95)]),
        );

        let opts = BriefOpts {
            project_names: vec!["web".into(), "api".into()],
            output_root: report,
            out_path: root.path().join("brief.json"),
            top: 3,
            stale_days: 7,
            force: true,
            check: false,
        };
        let rc = run_brief(&opts).unwrap();
        assert_eq!(rc, 0);

        let raw = std::fs::read_to_string(&opts.out_path).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        let hs = parsed["hotspots"].as_array().unwrap();
        assert_eq!(hs.len(), 3);
        // Score-sorted desc across projects.
        let ids: Vec<&str> = hs.iter().map(|n| n["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["api.b", "web.a", "api.a"]);
        // schema bump
        assert_eq!(parsed["schema_version"].as_u64().unwrap(), 2);
        // frozen[] is gone — ownership belongs to consumers.
        assert!(parsed.get("frozen").is_none());
    }

    #[test]
    fn brief_check_returns_2_when_stale() {
        let root = TempDir::new().unwrap();
        let report = root.path().join("report");
        write_analysis(&report, "web", analysis_with_nodes(&[("web.a", 0.5)]));
        let opts = BriefOpts {
            project_names: vec!["web".into()],
            output_root: report,
            out_path: root.path().join("brief.json"), // does not exist
            top: 5,
            stale_days: 7,
            force: false,
            check: true,
        };
        assert_eq!(run_brief(&opts).unwrap(), 2);
    }

    #[test]
    fn brief_skips_missing_projects_silently() {
        let root = TempDir::new().unwrap();
        let report = root.path().join("report");
        write_analysis(&report, "web", analysis_with_nodes(&[("web.a", 0.5)]));
        let opts = BriefOpts {
            project_names: vec!["web".into(), "ghost".into()],
            output_root: report,
            out_path: root.path().join("brief.json"),
            top: 5,
            stale_days: 7,
            force: true,
            check: false,
        };
        assert_eq!(run_brief(&opts).unwrap(), 0);
        let parsed: Value =
            serde_json::from_reader(std::fs::File::open(&opts.out_path).unwrap()).unwrap();
        assert_eq!(parsed["hotspots"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn brief_empty_projects_errors() {
        let root = TempDir::new().unwrap();
        let opts = BriefOpts {
            project_names: vec![],
            output_root: root.path().to_path_buf(),
            out_path: root.path().join("brief.json"),
            top: 5,
            stale_days: 7,
            force: true,
            check: false,
        };
        assert!(run_brief(&opts).is_err());
    }

    #[test]
    fn scope_merges_files_and_explains() {
        let tmp = TempDir::new().unwrap();
        let brief_path = tmp.path().join("brief.json");
        let initial = json!({
            "schema_version": 2,
            "generated_at": "2026-04-26T00:00:00Z",
            "graphify_version": "0.0.0",
            "stale": false,
            "projects": ["web"],
            "hotspots": [],
            "cycles": [],
            "scope_files": [],
            "scope_explains": []
        });
        std::fs::write(&brief_path, serde_json::to_string(&initial).unwrap()).unwrap();

        let opts = ScopeOpts {
            files: vec!["a.ts".into(), "b.ts".into()],
            max: 5,
            in_path: brief_path.clone(),
            task: Some("FEAT-001".into()),
        };
        let rc = run_scope(&opts, |f| {
            Some(json!({
                "node": f,
                "in_degree": 1
            }))
        })
        .unwrap();
        assert_eq!(rc, 0);

        let parsed: Value =
            serde_json::from_str(&std::fs::read_to_string(&brief_path).unwrap()).unwrap();
        assert_eq!(parsed["scope_files"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["scope_task"].as_str(), Some("FEAT-001"));
        let explains = parsed["scope_explains"].as_array().unwrap();
        assert_eq!(explains[0]["file"].as_str(), Some("a.ts"));
        assert_eq!(explains[0]["explain"]["node"].as_str(), Some("a.ts"));
    }

    #[test]
    fn scope_caps_files_at_max() {
        let tmp = TempDir::new().unwrap();
        let brief_path = tmp.path().join("brief.json");
        std::fs::write(
            &brief_path,
            serde_json::to_string(&json!({"scope_files":[],"scope_explains":[]})).unwrap(),
        )
        .unwrap();

        let opts = ScopeOpts {
            files: (0..10).map(|i| format!("f{i}.ts")).collect(),
            max: 3,
            in_path: brief_path.clone(),
            task: None,
        };
        run_scope(&opts, |_| None).unwrap();

        let parsed: Value =
            serde_json::from_str(&std::fs::read_to_string(&brief_path).unwrap()).unwrap();
        assert_eq!(parsed["scope_files"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn scope_unknown_file_renders_no_explain() {
        let tmp = TempDir::new().unwrap();
        let brief_path = tmp.path().join("brief.json");
        std::fs::write(
            &brief_path,
            serde_json::to_string(&json!({"scope_files":[],"scope_explains":[]})).unwrap(),
        )
        .unwrap();
        let opts = ScopeOpts {
            files: vec!["ghost.ts".into()],
            max: 5,
            in_path: brief_path.clone(),
            task: None,
        };
        run_scope(&opts, |_| None).unwrap();

        let parsed: Value =
            serde_json::from_str(&std::fs::read_to_string(&brief_path).unwrap()).unwrap();
        assert_eq!(
            parsed["scope_explains"][0]["explain"].as_str(),
            Some("(no explain)")
        );
    }

    #[test]
    fn scope_missing_brief_errors() {
        let tmp = TempDir::new().unwrap();
        let opts = ScopeOpts {
            files: vec!["a.ts".into()],
            max: 5,
            in_path: tmp.path().join("nope.json"),
            task: None,
        };
        assert!(run_scope(&opts, |_| None).is_err());
    }

    #[test]
    fn cycle_parse_handles_array_and_object() {
        assert_eq!(
            parse_cycle(&json!(["a", "b"])),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(
            parse_cycle(&json!({"nodes": ["x", "y"]})),
            Some(vec!["x".into(), "y".into()])
        );
        assert!(parse_cycle(&json!(42)).is_none());
    }

    // -----------------------------------------------------------------------
    // BUG-028: baseline_age_days reads `generated_at` from analysis.json,
    // not the directory mtime.
    // -----------------------------------------------------------------------

    #[test]
    fn baseline_age_returns_none_when_no_baseline_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(baseline_age_days(tmp.path()).is_none());
    }

    #[test]
    fn baseline_age_returns_none_when_baseline_dir_empty() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("baseline")).unwrap();
        assert!(baseline_age_days(tmp.path()).is_none());
    }

    #[test]
    fn baseline_age_uses_generated_at_field_when_present() {
        // Repro of GH issue #15: dir mtime can be wildly old, but the
        // `generated_at` field in the file should win.
        let tmp = TempDir::new().unwrap();
        let baseline = tmp.path().join("baseline");
        std::fs::create_dir_all(&baseline).unwrap();
        let body = json!({
            "generated_at": Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "nodes": [],
            "edges": [],
            "communities": [],
            "cycles": [],
            "summary": {
                "total_nodes": 0, "total_edges": 0,
                "total_communities": 0, "total_cycles": 0,
                "top_hotspots": []
            }
        });
        std::fs::write(
            baseline.join("analysis.json"),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();

        let age = baseline_age_days(tmp.path()).expect("expected an age");
        assert_eq!(age, 0, "expected 0 days for a just-stamped baseline");
    }

    #[test]
    fn baseline_age_falls_back_to_file_mtime_when_field_absent() {
        // Legacy snapshot (no `generated_at`) — must fall back to file
        // mtime, NOT directory mtime. The file mtime is fresh because we
        // just wrote it; the directory mtime is also fresh on first
        // creation but the test shape would still pass with the old
        // (broken) implementation. This test guards the fallback path.
        let tmp = TempDir::new().unwrap();
        let baseline = tmp.path().join("baseline");
        std::fs::create_dir_all(&baseline).unwrap();
        let body = json!({
            "nodes": [], "edges": [], "communities": [], "cycles": [],
            "summary": {
                "total_nodes": 0, "total_edges": 0,
                "total_communities": 0, "total_cycles": 0,
                "top_hotspots": []
            }
        });
        std::fs::write(
            baseline.join("analysis.json"),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();

        let age = baseline_age_days(tmp.path()).expect("expected an age");
        assert_eq!(age, 0, "fresh file should report 0d via mtime fallback");
    }

    #[test]
    fn baseline_age_walks_nested_per_project_layout() {
        // Multi-project layout: report/baseline/<project>/analysis.json.
        let tmp = TempDir::new().unwrap();
        let proj = tmp.path().join("baseline").join("my-svc");
        std::fs::create_dir_all(&proj).unwrap();
        let body = json!({
            "generated_at": Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "nodes": [], "edges": [], "communities": [], "cycles": [],
            "summary": {
                "total_nodes": 0, "total_edges": 0,
                "total_communities": 0, "total_cycles": 0,
                "top_hotspots": []
            }
        });
        std::fs::write(
            proj.join("analysis.json"),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();

        let age = baseline_age_days(tmp.path()).expect("expected an age");
        assert_eq!(age, 0);
    }

    #[test]
    fn baseline_age_picks_youngest_across_multiple_baselines() {
        // Two project baselines, one stamped 30 days ago, one stamped now.
        // The function must report the youngest age — operators promote in
        // batch and care about freshness, not staleness of stragglers.
        let tmp = TempDir::new().unwrap();
        let baseline = tmp.path().join("baseline");
        let old = baseline.join("old-svc");
        let fresh = baseline.join("fresh-svc");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&fresh).unwrap();

        let mk_body = |ts: String| {
            json!({
                "generated_at": ts,
                "nodes": [], "edges": [], "communities": [], "cycles": [],
                "summary": {
                    "total_nodes": 0, "total_edges": 0,
                    "total_communities": 0, "total_cycles": 0,
                    "top_hotspots": []
                }
            })
        };

        let thirty_days_ago = Utc::now() - chrono::Duration::days(30);
        std::fs::write(
            old.join("analysis.json"),
            serde_json::to_string_pretty(&mk_body(
                thirty_days_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            ))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            fresh.join("analysis.json"),
            serde_json::to_string_pretty(&mk_body(
                Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            ))
            .unwrap(),
        )
        .unwrap();

        let age = baseline_age_days(tmp.path()).expect("expected an age");
        assert_eq!(age, 0, "youngest baseline should win");
    }

    #[test]
    fn parse_iso8601_age_handles_known_value() {
        // 7 days ago should yield 7 (give-or-take rounding to whole days).
        let now = SystemTime::now();
        let seven_days_ago = Utc::now() - chrono::Duration::days(7);
        let ts = seven_days_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let age = parse_iso8601_age_days(&ts, now).expect("parse failed");
        assert!((6..=7).contains(&age), "expected 6 or 7 days, got {}", age);
    }

    #[test]
    fn parse_iso8601_age_returns_none_on_garbage() {
        let now = SystemTime::now();
        assert!(parse_iso8601_age_days("not-a-date", now).is_none());
        assert!(parse_iso8601_age_days("", now).is_none());
    }
}
