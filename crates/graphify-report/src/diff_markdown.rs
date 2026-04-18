use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::diff::{DiffReport, ScoreChange};

/// Writes the diff report as a human-readable Markdown file to `path`.
///
/// # Panics
/// Panics if file I/O fails.
pub fn write_diff_markdown(report: &DiffReport, path: &Path) {
    let buf = render_diff_markdown(report);
    std::fs::write(path, buf).expect("write diff markdown");
}

/// Renders the DiffReport as a Markdown string.
fn render_diff_markdown(report: &DiffReport) -> String {
    let mut buf = String::new();

    // Title
    writeln!(buf, "# Architectural Drift Report").unwrap();
    writeln!(buf).unwrap();

    // Summary table
    writeln!(buf, "## Summary").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Metric | Before | After | Change |").unwrap();
    writeln!(buf, "|--------|--------|-------|--------|").unwrap();
    write_summary_row(&mut buf, "Nodes", &report.summary_delta.nodes);
    write_summary_row(&mut buf, "Edges", &report.summary_delta.edges);
    write_summary_row(&mut buf, "Communities", &report.summary_delta.communities);
    write_summary_row(&mut buf, "Cycles", &report.summary_delta.cycles);
    writeln!(buf).unwrap();

    // New / Removed nodes
    write_node_list(&mut buf, "New Nodes", &report.edges.added_nodes);
    write_node_list(&mut buf, "Removed Nodes", &report.edges.removed_nodes);

    // Degree changes
    if !report.edges.degree_changes.is_empty() {
        writeln!(
            buf,
            "## Degree Changes ({})",
            report.edges.degree_changes.len()
        )
        .unwrap();
        writeln!(buf).unwrap();
        writeln!(
            buf,
            "| Node | In (before\u{2192}after) | Out (before\u{2192}after) |"
        )
        .unwrap();
        writeln!(buf, "|------|-------------------|-------------------|").unwrap();
        for dc in &report.edges.degree_changes {
            writeln!(
                buf,
                "| `{}` | {}\u{2192}{} ({:+}) | {}\u{2192}{} ({:+}) |",
                dc.id,
                dc.in_degree.before,
                dc.in_degree.after,
                dc.in_degree.change,
                dc.out_degree.before,
                dc.out_degree.after,
                dc.out_degree.change,
            )
            .unwrap();
        }
        writeln!(buf).unwrap();
    }

    // Cycle changes
    writeln!(buf, "## Cycle Changes").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "### Introduced ({})", report.cycles.introduced.len()).unwrap();
    writeln!(buf).unwrap();
    if report.cycles.introduced.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for cycle in &report.cycles.introduced {
            writeln!(buf, "- {}", format_cycle(cycle)).unwrap();
        }
    }
    writeln!(buf).unwrap();
    writeln!(buf, "### Resolved ({})", report.cycles.resolved.len()).unwrap();
    writeln!(buf).unwrap();
    if report.cycles.resolved.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for cycle in &report.cycles.resolved {
            writeln!(buf, "- {}", format_cycle(cycle)).unwrap();
        }
    }
    writeln!(buf).unwrap();

    // Hotspot movement — mirror-annotated entries are pulled into a
    // dedicated subsection below so the main Rising/Falling/Top-20 lists
    // stay focused on non-mirror drift.
    writeln!(buf, "## Hotspot Movement").unwrap();
    writeln!(buf).unwrap();
    let (rising_norm, rising_mirror) = partition_mirrors(&report.hotspots.rising);
    let (falling_norm, falling_mirror) = partition_mirrors(&report.hotspots.falling);
    let (new_norm, new_mirror) = partition_mirrors(&report.hotspots.new_hotspots);
    let (removed_norm, removed_mirror) = partition_mirrors(&report.hotspots.removed_hotspots);
    write_score_table(&mut buf, "Rising", &rising_norm);
    write_score_table(&mut buf, "Falling", &falling_norm);
    write_score_list(&mut buf, "New in Top 20", &new_norm, true);
    write_score_list(&mut buf, "Left Top 20", &removed_norm, false);
    write_intentional_mirrors_subsection(
        &mut buf,
        &rising_mirror,
        &falling_mirror,
        &new_mirror,
        &removed_mirror,
    );

    // Community shifts
    writeln!(buf, "## Community Shifts").unwrap();
    writeln!(buf).unwrap();
    if report.communities.moved_nodes.is_empty() {
        writeln!(buf, "No community changes detected.").unwrap();
    } else {
        writeln!(
            buf,
            "- **{} nodes** moved communities",
            report.communities.moved_nodes.len()
        )
        .unwrap();
        for mv in &report.communities.moved_nodes {
            writeln!(
                buf,
                "  - `{}`: community {} \u{2192} {}",
                mv.id, mv.from_community, mv.to_community
            )
            .unwrap();
        }
    }
    writeln!(
        buf,
        "- **{} nodes** stable",
        report.communities.stable_count
    )
    .unwrap();

    buf
}

fn write_summary_row(buf: &mut String, label: &str, delta: &graphify_core::diff::Delta<usize>) {
    let sign = if delta.change > 0 { "+" } else { "" };
    writeln!(
        buf,
        "| {} | {} | {} | {}{} |",
        label, delta.before, delta.after, sign, delta.change
    )
    .unwrap();
}

fn write_node_list(buf: &mut String, title: &str, nodes: &[String]) {
    writeln!(buf, "## {} ({})", title, nodes.len()).unwrap();
    writeln!(buf).unwrap();
    if nodes.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for n in nodes {
            writeln!(buf, "- `{}`", n).unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn write_score_table(buf: &mut String, title: &str, changes: &[graphify_core::diff::ScoreChange]) {
    writeln!(buf, "### {}", title).unwrap();
    writeln!(buf).unwrap();
    if changes.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        writeln!(buf, "| Node | Before | After | Delta |").unwrap();
        writeln!(buf, "|------|--------|-------|-------|").unwrap();
        for sc in changes {
            writeln!(
                buf,
                "| `{}` | {:.3} | {:.3} | {:+.3} |",
                sc.id, sc.before, sc.after, sc.delta
            )
            .unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn write_score_list(
    buf: &mut String,
    title: &str,
    changes: &[graphify_core::diff::ScoreChange],
    show_after: bool,
) {
    writeln!(buf, "### {}", title).unwrap();
    writeln!(buf).unwrap();
    if changes.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for sc in changes {
            let score = if show_after { sc.after } else { sc.before };
            writeln!(buf, "- `{}` (score: {:.3})", sc.id, score).unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn format_cycle(cycle: &[String]) -> String {
    let parts: Vec<String> = cycle.iter().map(|id| format!("`{}`", id)).collect();
    parts.join(" \u{2192} ")
}

/// Splits a score-change bucket into `(non_mirror, mirror)` slices based
/// on whether `intentional_mirror` is set. Clones entries so callers can
/// feed them into the existing `&[ScoreChange]`-typed writers without
/// signature churn — buckets are small (tens of entries).
fn partition_mirrors(bucket: &[ScoreChange]) -> (Vec<ScoreChange>, Vec<ScoreChange>) {
    let mut non_mirror = Vec::new();
    let mut mirror = Vec::new();
    for sc in bucket {
        if sc.intentional_mirror.is_some() {
            mirror.push(sc.clone());
        } else {
            non_mirror.push(sc.clone());
        }
    }
    (non_mirror, mirror)
}

/// Renders the `### Intentional mirrors` subsection under
/// `## Hotspot Movement`. Groups entries by mirror name (sorted) and
/// labels each with the bucket it came from so reviewers can trace back
/// where it would have appeared without the annotation.
fn write_intentional_mirrors_subsection(
    buf: &mut String,
    rising: &[ScoreChange],
    falling: &[ScoreChange],
    new_hotspots: &[ScoreChange],
    removed_hotspots: &[ScoreChange],
) {
    let total = rising.len() + falling.len() + new_hotspots.len() + removed_hotspots.len();
    if total == 0 {
        return;
    }

    writeln!(buf, "### Intentional mirrors ({})", total).unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "_Declared under `[consolidation.intentional_mirrors]`; excluded from the Rising/Falling/Top-20 lists above._"
    )
    .unwrap();
    writeln!(buf).unwrap();

    let mut groups: BTreeMap<String, Vec<(&'static str, &ScoreChange)>> = BTreeMap::new();
    collect_mirror_group(&mut groups, rising, "rising");
    collect_mirror_group(&mut groups, falling, "falling");
    collect_mirror_group(&mut groups, new_hotspots, "new");
    collect_mirror_group(&mut groups, removed_hotspots, "removed");

    for (group_name, entries) in &groups {
        writeln!(buf, "- **{}**", group_name).unwrap();
        for (bucket, sc) in entries {
            writeln!(
                buf,
                "  - `{}` ({}, {:.3} \u{2192} {:.3}, {:+.3})",
                sc.id, bucket, sc.before, sc.after, sc.delta
            )
            .unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn collect_mirror_group<'a>(
    groups: &mut BTreeMap<String, Vec<(&'static str, &'a ScoreChange)>>,
    entries: &'a [ScoreChange],
    bucket: &'static str,
) {
    for sc in entries {
        if let Some(group) = &sc.intentional_mirror {
            groups.entry(group.clone()).or_default().push((bucket, sc));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::diff::*;

    fn report_with_changes() -> DiffReport {
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 10,
                    after: 12,
                    change: 2,
                },
                edges: Delta {
                    before: 20,
                    after: 25,
                    change: 5,
                },
                communities: Delta {
                    before: 3,
                    after: 4,
                    change: 1,
                },
                cycles: Delta {
                    before: 1,
                    after: 0,
                    change: -1,
                },
            },
            edges: EdgeDiff {
                added_nodes: vec!["app.new".into()],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![],
                resolved: vec![vec!["a".into(), "b".into()]],
            },
            hotspots: HotspotDiff {
                rising: vec![ScoreChange {
                    id: "app.hot".into(),
                    before: 0.3,
                    after: 0.6,
                    delta: 0.3,
                    intentional_mirror: None,
                }],
                falling: vec![],
                new_hotspots: vec![],
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: vec![CommunityMove {
                    id: "app.moved".into(),
                    from_community: 0,
                    to_community: 2,
                }],
                stable_count: 9,
            },
        }
    }

    #[test]
    fn markdown_contains_expected_sections() {
        let md = render_diff_markdown(&report_with_changes());
        assert!(md.contains("# Architectural Drift Report"));
        assert!(md.contains("## Summary"));
        assert!(md.contains("## New Nodes (1)"));
        assert!(md.contains("## Removed Nodes (0)"));
        assert!(md.contains("## Cycle Changes"));
        assert!(md.contains("### Resolved (1)"));
        assert!(md.contains("## Hotspot Movement"));
        assert!(md.contains("## Community Shifts"));
    }

    #[test]
    fn markdown_summary_table_has_correct_values() {
        let md = render_diff_markdown(&report_with_changes());
        assert!(md.contains("| Nodes | 10 | 12 | +2 |"));
        assert!(md.contains("| Cycles | 1 | 0 | -1 |"));
    }

    #[test]
    fn write_diff_markdown_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drift-report.md");
        write_diff_markdown(&report_with_changes(), &path);
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Architectural Drift Report"));
    }

    fn report_with_mirror_rising() -> DiffReport {
        let mut r = report_with_changes();
        r.hotspots.rising.push(ScoreChange {
            id: "app.models.tokens.TokenUsage".into(),
            before: 0.30,
            after: 0.38,
            delta: 0.08,
            intentional_mirror: Some("TokenUsage".into()),
        });
        r.hotspots.new_hotspots.push(ScoreChange {
            id: "src.tokens.TokenUsage".into(),
            before: 0.0,
            after: 0.42,
            delta: 0.42,
            intentional_mirror: Some("TokenUsage".into()),
        });
        r.hotspots.rising.push(ScoreChange {
            id: "app.models.LessonType".into(),
            before: 0.25,
            after: 0.33,
            delta: 0.08,
            intentional_mirror: Some("LessonType".into()),
        });
        r
    }

    #[test]
    fn markdown_renders_intentional_mirrors_subsection() {
        let md = render_diff_markdown(&report_with_mirror_rising());
        assert!(
            md.contains("### Intentional mirrors (3)"),
            "expected mirror subsection header with count, got:\n{md}"
        );
        // Groups render sorted by name: LessonType before TokenUsage.
        let lesson_pos = md.find("**LessonType**").expect("LessonType group");
        let token_pos = md.find("**TokenUsage**").expect("TokenUsage group");
        assert!(lesson_pos < token_pos);
        // Bucket labels appear in entry lines.
        assert!(md.contains("(rising, 0.300 \u{2192} 0.380, +0.080)"));
        assert!(md.contains("(new, 0.000 \u{2192} 0.420, +0.420)"));
    }

    #[test]
    fn markdown_excludes_mirror_entries_from_rising_table() {
        let md = render_diff_markdown(&report_with_mirror_rising());
        // The non-mirror rising entry (app.hot) must still appear in the
        // Rising table.
        assert!(md.contains("| `app.hot` | 0.300 | 0.600 | +0.300 |"));
        // Mirror entries must NOT appear as rows in the Rising table; they
        // only appear inside the Intentional mirrors subsection.
        let rising_section_start = md.find("### Rising").expect("Rising section");
        let mirror_section_start = md.find("### Intentional mirrors").expect("mirrors section");
        let rising_chunk = &md[rising_section_start..mirror_section_start];
        assert!(
            !rising_chunk.contains("| `app.models.tokens.TokenUsage` |"),
            "mirror entry should be filtered out of Rising table, got:\n{rising_chunk}"
        );
    }

    #[test]
    fn markdown_omits_subsection_when_no_mirrors() {
        let md = render_diff_markdown(&report_with_changes());
        assert!(!md.contains("### Intentional mirrors"));
    }
}
