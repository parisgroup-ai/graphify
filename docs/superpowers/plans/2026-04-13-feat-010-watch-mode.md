# FEAT-010: Watch Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `graphify watch` subcommand that monitors source files and auto-rebuilds the analysis pipeline on changes, using the FEAT-005 extraction cache for fast incremental rebuilds.

**Architecture:** New `Commands::Watch` variant in the CLI dispatches to a `cmd_watch` function that runs the pipeline once, then starts a `notify` file watcher with 300ms debounce. On change events, it determines which project(s) are affected and re-runs the full pipeline (extract + cache → analyze → report) for those projects only.

**Tech Stack:** `notify 7` (cross-platform file watcher), `notify-debouncer-mini 0.5` (debounce wrapper), existing `run_extract`/`run_analyze`/`write_all_outputs` pipeline functions.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `crates/graphify-cli/Cargo.toml` | Modify | Add `notify` and `notify-debouncer-mini` dependencies |
| `crates/graphify-cli/src/main.rs` | Modify | Add `Commands::Watch` variant, `cmd_watch` function, watch event loop |
| `crates/graphify-cli/src/watch.rs` | Create | `WatchFilter` struct (extension matching, exclude filtering, output dir exclusion) |

---

### Task 1: Add `notify` dependencies

**Files:**
- Modify: `crates/graphify-cli/Cargo.toml`

- [ ] **Step 1: Add notify and notify-debouncer-mini to Cargo.toml**

Add to the `[dependencies]` section of `crates/graphify-cli/Cargo.toml`:

```toml
notify = "7"
notify-debouncer-mini = "0.5"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p graphify-cli`
Expected: Compiles with no errors (dependencies download and resolve).

- [ ] **Step 3: Commit**

```bash
git add crates/graphify-cli/Cargo.toml Cargo.lock
git commit -m "feat(cli): add notify + notify-debouncer-mini deps (FEAT-010)"
```

---

### Task 2: Create `WatchFilter` with tests

**Files:**
- Create: `crates/graphify-cli/src/watch.rs`
- Modify: `crates/graphify-cli/src/main.rs` (add `mod watch;`)

- [ ] **Step 1: Write failing tests for WatchFilter**

Create `crates/graphify-cli/src/watch.rs` with the test module first:

```rust
use std::path::{Path, PathBuf};

/// Filters file-system events to only relevant source file changes.
///
/// Checks:
/// 1. File extension matches configured languages (.py, .ts, .tsx)
/// 2. Path is not inside an excluded directory
/// 3. Path is not inside the output directory
pub struct WatchFilter {
    extensions: Vec<String>,
    exclude_dirs: Vec<String>,
    output_dir: PathBuf,
}

impl WatchFilter {
    pub fn new(
        languages: &[String],
        exclude_dirs: &[String],
        output_dir: &Path,
    ) -> Self {
        let extensions: Vec<String> = languages
            .iter()
            .flat_map(|lang| match lang.to_lowercase().as_str() {
                "python" => vec!["py".to_string()],
                "typescript" => vec!["ts".to_string(), "tsx".to_string()],
                _ => vec![],
            })
            .collect();

        Self {
            extensions,
            exclude_dirs: exclude_dirs.to_vec(),
            output_dir: output_dir.to_path_buf(),
        }
    }

    /// Returns `true` if the path represents a relevant source file change.
    pub fn should_rebuild(&self, path: &Path) -> bool {
        // Check extension
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => return false,
        };
        if !self.extensions.iter().any(|allowed| allowed == ext) {
            return false;
        }

        // Check excluded directories
        let path_str = path.to_string_lossy();
        for exclude in &self.exclude_dirs {
            if path_str.contains(&format!("/{exclude}/")) || path_str.contains(&format!("\\{exclude}\\")) {
                return false;
            }
        }

        // Check output directory
        if path.starts_with(&self.output_dir) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter() -> WatchFilter {
        WatchFilter::new(
            &["python".to_string(), "typescript".to_string()],
            &["node_modules".to_string(), "__pycache__".to_string(), ".git".to_string()],
            Path::new("/project/report"),
        )
    }

    #[test]
    fn accepts_python_files() {
        let filter = make_filter();
        assert!(filter.should_rebuild(Path::new("/project/app/main.py")));
    }

    #[test]
    fn accepts_typescript_files() {
        let filter = make_filter();
        assert!(filter.should_rebuild(Path::new("/project/src/index.ts")));
        assert!(filter.should_rebuild(Path::new("/project/src/App.tsx")));
    }

    #[test]
    fn rejects_non_source_files() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/README.md")));
        assert!(!filter.should_rebuild(Path::new("/project/Cargo.toml")));
        assert!(!filter.should_rebuild(Path::new("/project/image.png")));
    }

    #[test]
    fn rejects_files_without_extension() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/Makefile")));
    }

    #[test]
    fn rejects_excluded_directories() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/node_modules/dep/index.ts")));
        assert!(!filter.should_rebuild(Path::new("/project/app/__pycache__/module.py")));
        assert!(!filter.should_rebuild(Path::new("/project/.git/hooks/pre-commit.py")));
    }

    #[test]
    fn rejects_output_directory() {
        let filter = make_filter();
        assert!(!filter.should_rebuild(Path::new("/project/report/proj/graph.json")));
        assert!(!filter.should_rebuild(Path::new("/project/report/anything.py")));
    }

    #[test]
    fn empty_languages_rejects_all() {
        let filter = WatchFilter::new(&[], &[], Path::new("/out"));
        assert!(!filter.should_rebuild(Path::new("/project/main.py")));
    }
}
```

- [ ] **Step 2: Add `mod watch;` to main.rs**

Add `mod watch;` near the top of `crates/graphify-cli/src/main.rs`, after the `use` imports (around line 25):

```rust
mod watch;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p graphify-cli -- watch`
Expected: All 7 WatchFilter tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/graphify-cli/src/watch.rs crates/graphify-cli/src/main.rs
git commit -m "feat(cli): WatchFilter with extension + exclude + output dir filtering (FEAT-010)"
```

---

### Task 3: Add `Commands::Watch` variant and CLI parsing

**Files:**
- Modify: `crates/graphify-cli/src/main.rs:69-232` (Commands enum)

- [ ] **Step 1: Add Watch variant to Commands enum**

In `crates/graphify-cli/src/main.rs`, add the `Watch` variant to the `Commands` enum, after the `Shell` variant (around line 231):

```rust
    /// Watch source files and auto-rebuild on changes
    Watch {
        /// Path to graphify.toml config
        #[arg(long, default_value = "graphify.toml")]
        config: PathBuf,

        /// Output directory (overrides config setting)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Force full rebuild on first run, ignoring extraction cache
        #[arg(long)]
        force: bool,

        /// Output formats: json,csv,md,html,neo4j,graphml,obsidian (comma-separated)
        #[arg(long)]
        format: Option<String>,
    },
```

- [ ] **Step 2: Add placeholder match arm**

In the `match cli.command` block in `main()`, add a placeholder arm for `Watch`:

```rust
        Commands::Watch { config, output, force, format } => {
            cmd_watch(&config, output.as_deref(), force, format.as_deref());
        }
```

- [ ] **Step 3: Add empty cmd_watch stub**

Add at the bottom of `main.rs`, before the helper functions:

```rust
fn cmd_watch(config_path: &Path, output_override: Option<&Path>, force: bool, format_override: Option<&str>) {
    let _cfg = load_config(config_path);
    let _out_dir = resolve_output(&_cfg, output_override);
    let _formats = resolve_formats(&_cfg, format_override);
    eprintln!("Watch mode not yet implemented");
}
```

- [ ] **Step 4: Verify it compiles and --help shows Watch**

Run: `cargo run -p graphify-cli -- watch --help`
Expected: Shows help text for the watch subcommand with --config, --output, --force, --format flags.

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "feat(cli): add Commands::Watch variant with CLI flags (FEAT-010)"
```

---

### Task 4: Extract `run_pipeline_for_project` helper

The `Commands::Run` and `Commands::Report` blocks have duplicated pipeline logic (extract → analyze → assign communities → write outputs). Extract this into a reusable function that `cmd_watch` can call per rebuild cycle.

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Create `run_pipeline_for_project` function**

Add this function after `run_analyze` (around line 960):

```rust
/// Runs the full pipeline for a single project: extract → analyze → write outputs.
///
/// Returns a `ProjectData` struct for use in cross-project summaries.
fn run_pipeline_for_project(
    project: &ProjectConfig,
    settings: &Settings,
    proj_out: &Path,
    weights: &ScoringWeights,
    formats: &[String],
    force: bool,
) -> ProjectData {
    let (graph, _, stats) = run_extract(project, settings, Some(proj_out), force);
    print_cache_stats(&project.name, &stats);
    let (mut metrics, communities, cycles_simple) = run_analyze(&graph, weights);
    assign_community_ids(&mut metrics, &communities);
    let cycles_for_report: Vec<Cycle> = cycles_simple;
    write_all_outputs(
        &project.name,
        &graph,
        &metrics,
        &communities,
        &cycles_for_report,
        proj_out,
        formats,
    );
    ProjectData {
        name: project.name.clone(),
        graph,
        metrics,
        cycles: cycles_for_report,
    }
}
```

- [ ] **Step 2: Refactor Commands::Run to use it**

Replace the body of the `Commands::Run` match arm with:

```rust
        Commands::Run { config, output, force } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, None);
            let formats = resolve_formats(&cfg, None);
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let pd = run_pipeline_for_project(project, &cfg.settings, &proj_out, &w, &formats, force);
                println!(
                    "[{}] Pipeline complete → {}",
                    project.name,
                    proj_out.display()
                );
                project_data.push(pd);
            }
            if project_data.len() > 1 {
                write_summary(&project_data, &out_dir);
            }
        }
```

- [ ] **Step 3: Refactor Commands::Report to use it**

Replace the body of the `Commands::Report` match arm with:

```rust
        Commands::Report {
            config,
            output,
            weights,
            format,
            force,
        } => {
            let cfg = load_config(&config);
            let out_dir = resolve_output(&cfg, output.as_deref());
            let w = resolve_weights(&cfg, weights.as_deref());
            let formats = resolve_formats(&cfg, format.as_deref());
            let mut project_data: Vec<ProjectData> = Vec::new();
            for project in &cfg.project {
                let proj_out = out_dir.join(&project.name);
                std::fs::create_dir_all(&proj_out).expect("create output directory");
                let pd = run_pipeline_for_project(project, &cfg.settings, &proj_out, &w, &formats, force);
                println!(
                    "[{}] Report written to {}",
                    project.name,
                    proj_out.display()
                );
                project_data.push(pd);
            }
            if project_data.len() > 1 {
                write_summary(&project_data, &out_dir);
            }
        }
```

- [ ] **Step 4: Run full test suite to verify refactor is safe**

Run: `cargo test -p graphify-cli`
Expected: All existing tests pass (no behavior change).

- [ ] **Step 5: Commit**

```bash
git add crates/graphify-cli/src/main.rs
git commit -m "refactor(cli): extract run_pipeline_for_project helper (FEAT-010)"
```

---

### Task 5: Implement `cmd_watch` — initial run + watch loop

**Files:**
- Modify: `crates/graphify-cli/src/main.rs` (replace `cmd_watch` stub)
- Modify: `crates/graphify-cli/src/watch.rs` (add `determine_affected_projects` helper)

- [ ] **Step 1: Add `determine_affected_projects` to watch.rs**

Add this function to `crates/graphify-cli/src/watch.rs`:

```rust
/// Given a set of changed file paths and a list of project repo directories,
/// returns indices of projects whose repo directory contains at least one changed file.
pub fn determine_affected_projects(
    changed_paths: &[PathBuf],
    project_repos: &[PathBuf],
) -> Vec<usize> {
    let mut affected = Vec::new();
    for (i, repo) in project_repos.iter().enumerate() {
        let canonical_repo = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.clone());
        if changed_paths.iter().any(|p| {
            let canonical_p = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            canonical_p.starts_with(&canonical_repo)
        }) {
            affected.push(i);
        }
    }
    affected
}

#[cfg(test)]
mod tests {
    // ... existing tests ...

    #[test]
    fn determine_affected_projects_matches_by_prefix() {
        use super::determine_affected_projects;
        use std::path::PathBuf;

        let repos = vec![
            PathBuf::from("/project/apps/api"),
            PathBuf::from("/project/apps/web"),
        ];
        let changed = vec![PathBuf::from("/project/apps/api/src/main.py")];
        let affected = determine_affected_projects(&changed, &repos);
        assert_eq!(affected, vec![0]);
    }

    #[test]
    fn determine_affected_projects_multiple() {
        use super::determine_affected_projects;
        use std::path::PathBuf;

        let repos = vec![
            PathBuf::from("/project/apps/api"),
            PathBuf::from("/project/apps/web"),
        ];
        let changed = vec![
            PathBuf::from("/project/apps/api/src/main.py"),
            PathBuf::from("/project/apps/web/src/index.ts"),
        ];
        let affected = determine_affected_projects(&changed, &repos);
        assert_eq!(affected, vec![0, 1]);
    }

    #[test]
    fn determine_affected_projects_none() {
        use super::determine_affected_projects;
        use std::path::PathBuf;

        let repos = vec![PathBuf::from("/project/apps/api")];
        let changed = vec![PathBuf::from("/other/path/file.py")];
        let affected = determine_affected_projects(&changed, &repos);
        assert!(affected.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p graphify-cli -- watch`
Expected: All WatchFilter tests + 3 new `determine_affected_projects` tests pass.

- [ ] **Step 3: Implement full `cmd_watch`**

Replace the `cmd_watch` stub in `main.rs` with the full implementation:

```rust
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::sync::mpsc;
use std::time::Duration;
use watch::{WatchFilter, determine_affected_projects};

fn cmd_watch(config_path: &Path, output_override: Option<&Path>, force: bool, format_override: Option<&str>) {
    let cfg = load_config(config_path);
    let out_dir = resolve_output(&cfg, output_override);
    let weights = resolve_weights(&cfg, None);
    let formats = resolve_formats(&cfg, format_override);

    if cfg.project.is_empty() {
        eprintln!("Error: no projects configured in config file.");
        std::process::exit(1);
    }

    // Collect all language strings and excludes for the watch filter.
    let all_langs: Vec<String> = cfg.project
        .iter()
        .flat_map(|p| p.lang.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let exclude_dirs = cfg.settings.exclude.clone().unwrap_or_default();

    let canonical_out = std::fs::canonicalize(&out_dir).unwrap_or_else(|_| out_dir.clone());
    let filter = WatchFilter::new(&all_langs, &exclude_dirs, &canonical_out);

    // Collect project repo paths for affected-project detection.
    let project_repos: Vec<PathBuf> = cfg.project
        .iter()
        .map(|p| {
            let repo = PathBuf::from(&p.repo);
            std::fs::canonicalize(&repo).unwrap_or(repo)
        })
        .collect();

    // Run initial pipeline.
    eprintln!("=== Initial build ===");
    for project in &cfg.project {
        let proj_out = out_dir.join(&project.name);
        std::fs::create_dir_all(&proj_out).expect("create output directory");
        let _ = run_pipeline_for_project(project, &cfg.settings, &proj_out, &weights, &formats, force);
        eprintln!("[{}] Ready.", project.name);
    }

    // Setup file watcher.
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(300), tx)
        .expect("create file watcher");

    for repo in &project_repos {
        debouncer.watcher().watch(repo, notify::RecursiveMode::Recursive)
            .unwrap_or_else(|e| {
                eprintln!("Error: cannot watch {:?}: {e}", repo);
                std::process::exit(1);
            });
    }

    eprintln!();
    eprintln!("Watching {} project(s). Press Ctrl+C to stop.", cfg.project.len());
    for (i, repo) in project_repos.iter().enumerate() {
        eprintln!("  [{}] {}", cfg.project[i].name, repo.display());
    }
    eprintln!();

    // Event loop.
    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let changed_paths: Vec<PathBuf> = events
                    .iter()
                    .filter(|e| e.kind == DebouncedEventKind::Any)
                    .map(|e| e.path.clone())
                    .filter(|p| filter.should_rebuild(p))
                    .collect();

                if changed_paths.is_empty() {
                    continue;
                }

                let affected = determine_affected_projects(&changed_paths, &project_repos);
                if affected.is_empty() {
                    continue;
                }

                let start = std::time::Instant::now();
                eprintln!("--- Rebuild triggered ({} file(s) changed) ---", changed_paths.len());

                for &idx in &affected {
                    let project = &cfg.project[idx];
                    let proj_out = out_dir.join(&project.name);
                    std::fs::create_dir_all(&proj_out).expect("create output directory");
                    let _ = run_pipeline_for_project(
                        project, &cfg.settings, &proj_out, &weights, &formats, false,
                    );
                    eprintln!("[{}] Rebuilt.", project.name);
                }

                let elapsed = start.elapsed();
                eprintln!("--- Done in {:.1}s ---\n", elapsed.as_secs_f64());
            }
            Ok(Err(errors)) => {
                for e in errors {
                    eprintln!("Watch error: {e:?}");
                }
            }
            Err(e) => {
                eprintln!("Channel error: {e}");
                break;
            }
        }
    }
}
```

- [ ] **Step 4: Add the necessary imports at the top of main.rs**

Add to the `use` imports section (around lines 1-25):

```rust
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
```

And add `use watch::{WatchFilter, determine_affected_projects};` (or inline the use inside `cmd_watch` since these are only used there).

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p graphify-cli`
Expected: Compiles with no errors.

- [ ] **Step 6: Run clippy**

Run: `cargo clippy -p graphify-cli -- -D warnings`
Expected: No warnings.

- [ ] **Step 7: Run full test suite**

Run: `cargo test -p graphify-cli`
Expected: All tests pass (including WatchFilter and determine_affected_projects tests).

- [ ] **Step 8: Commit**

```bash
git add crates/graphify-cli/src/main.rs crates/graphify-cli/src/watch.rs
git commit -m "feat(cli): implement graphify watch — file watcher with debounced rebuild (FEAT-010)"
```

---

### Task 6: Manual verification + docs update

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`

- [ ] **Step 1: Manual test with a real project**

If a `graphify.toml` exists in the workspace or a test project:

Run: `cargo run -p graphify-cli -- watch --config graphify.toml`

Then in another terminal, modify a `.py` or `.ts` file and verify:
- The watch detects the change
- Pipeline re-runs (cache stats printed)
- Output files are regenerated

If no test project is available, verify with: `cargo run -p graphify-cli -- watch --help`

- [ ] **Step 2: Update CLAUDE.md**

Add `graphify watch` to the CLI commands section. Update the test count. Add watch.rs to the key modules table.

- [ ] **Step 3: Update sprint.md**

Mark FEAT-010 as `**done**` and add to the Done section.

- [ ] **Step 4: Run full workspace tests + clippy**

Run: `cargo test -p graphify-core -p graphify-extract -p graphify-report -p graphify-cli -p graphify-mcp && cargo clippy --workspace -- -D warnings`
Expected: All tests pass, zero clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md docs/TaskNotes/Tasks/sprint.md
git commit -m "docs: update CLAUDE.md and sprint board for FEAT-010 watch mode"
```

---

## Summary

| Task | Description | Estimated Steps |
|---|---|---|
| 1 | Add notify dependencies | 3 |
| 2 | WatchFilter with tests | 4 |
| 3 | Commands::Watch CLI variant | 5 |
| 4 | Extract run_pipeline_for_project helper | 5 |
| 5 | Implement cmd_watch event loop | 8 |
| 6 | Manual verification + docs | 5 |
| **Total** | | **30 steps** |
