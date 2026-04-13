use std::fmt::Write as FmtWrite;
use std::path::Path;

use graphify_core::history::{HotspotEntry, TrendInterval, TrendReport};

/// Writes the trend report as a human-readable Markdown file to `path`.
pub fn write_trend_markdown(report: &TrendReport, path: &Path) {
    let markdown = render_trend_markdown(report);
    std::fs::write(path, markdown).expect("write trend markdown");
}

fn render_trend_markdown(report: &TrendReport) -> String {
    let mut buf = String::new();

    writeln!(buf, "# Architectural Trend Report").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "- Project: `{}`", report.project).unwrap();
    writeln!(buf, "- Snapshots: {}", report.snapshot_count).unwrap();
    writeln!(
        buf,
        "- Window: {} → {}",
        report.window.first_captured_at, report.window.last_captured_at
    )
    .unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "## Summary Over Time").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "| Captured At | Nodes | Edges | Communities | Cycles | Mean Confidence |"
    )
    .unwrap();
    writeln!(
        buf,
        "|-------------|-------|-------|-------------|--------|-----------------|"
    )
    .unwrap();
    for point in &report.points {
        writeln!(
            buf,
            "| {} | {} | {} | {} | {} | {:.3} |",
            point.captured_at,
            point.total_nodes,
            point.total_edges,
            point.total_communities,
            point.total_cycles,
            point.mean_confidence,
        )
        .unwrap();
    }
    writeln!(buf).unwrap();

    writeln!(buf, "## Top Hotspots By Snapshot").unwrap();
    writeln!(buf).unwrap();
    for point in &report.points {
        writeln!(buf, "### {}", point.captured_at).unwrap();
        writeln!(buf).unwrap();
        write_hotspot_list(&mut buf, &point.top_hotspots);
    }

    writeln!(buf, "## Interval Changes").unwrap();
    writeln!(buf).unwrap();
    if report.intervals.is_empty() {
        writeln!(
            buf,
            "_A single snapshot is available; no interval changes yet._"
        )
        .unwrap();
        return buf;
    }

    for interval in &report.intervals {
        write_interval(&mut buf, interval);
    }

    buf
}

fn write_hotspot_list(buf: &mut String, hotspots: &[HotspotEntry]) {
    if hotspots.is_empty() {
        writeln!(buf, "_None_").unwrap();
    } else {
        for hotspot in hotspots {
            writeln!(buf, "- `{}` ({:.3})", hotspot.id, hotspot.score).unwrap();
        }
    }
    writeln!(buf).unwrap();
}

fn write_interval(buf: &mut String, interval: &TrendInterval) {
    writeln!(
        buf,
        "### {} → {}",
        interval.from_captured_at, interval.to_captured_at
    )
    .unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "- Nodes: {} → {} ({:+})",
        interval.summary_delta.nodes.before,
        interval.summary_delta.nodes.after,
        interval.summary_delta.nodes.change
    )
    .unwrap();
    writeln!(
        buf,
        "- Edges: {} → {} ({:+})",
        interval.summary_delta.edges.before,
        interval.summary_delta.edges.after,
        interval.summary_delta.edges.change
    )
    .unwrap();
    writeln!(
        buf,
        "- Cycles: {} → {} ({:+})",
        interval.summary_delta.cycles.before,
        interval.summary_delta.cycles.after,
        interval.summary_delta.cycles.change
    )
    .unwrap();
    writeln!(
        buf,
        "- Community churn: {} moved, {} stable ({:.1}%)",
        interval.communities.moved_nodes,
        interval.communities.stable_nodes,
        interval.communities.churn_pct
    )
    .unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "#### New Hotspots").unwrap();
    write_hotspot_list(buf, &interval.hotspots.new_hotspots);
    writeln!(buf, "#### Removed Hotspots").unwrap();
    write_hotspot_list(buf, &interval.hotspots.removed_hotspots);
    writeln!(buf, "#### Rising Hotspots").unwrap();
    if interval.hotspots.rising.is_empty() {
        writeln!(buf, "_None_").unwrap();
        writeln!(buf).unwrap();
    } else {
        for change in &interval.hotspots.rising {
            writeln!(
                buf,
                "- `{}`: {:.3} → {:.3} ({:+.3})",
                change.id, change.before, change.after, change.delta
            )
            .unwrap();
        }
        writeln!(buf).unwrap();
    }
    writeln!(buf, "#### Falling Hotspots").unwrap();
    if interval.hotspots.falling.is_empty() {
        writeln!(buf, "_None_").unwrap();
        writeln!(buf).unwrap();
    } else {
        for change in &interval.hotspots.falling {
            writeln!(
                buf,
                "- `{}`: {:.3} → {:.3} ({:+.3})",
                change.id, change.before, change.after, change.delta
            )
            .unwrap();
        }
        writeln!(buf).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::history::{
        CommunityChurn, HotspotEntry, TrendHotspotDelta, TrendInterval, TrendPoint, TrendReport,
        TrendSummaryDelta, TrendWindow,
    };

    fn sample_report() -> TrendReport {
        TrendReport {
            project: "demo".into(),
            snapshot_count: 2,
            window: TrendWindow {
                first_captured_at: 100,
                last_captured_at: 200,
            },
            points: vec![
                TrendPoint {
                    captured_at: 100,
                    total_nodes: 10,
                    total_edges: 20,
                    total_communities: 2,
                    total_cycles: 1,
                    top_hotspots: vec![HotspotEntry {
                        id: "app.alpha".into(),
                        score: 0.4,
                    }],
                    mean_confidence: 0.95,
                },
                TrendPoint {
                    captured_at: 200,
                    total_nodes: 12,
                    total_edges: 24,
                    total_communities: 3,
                    total_cycles: 0,
                    top_hotspots: vec![HotspotEntry {
                        id: "app.beta".into(),
                        score: 0.5,
                    }],
                    mean_confidence: 0.97,
                },
            ],
            intervals: vec![TrendInterval {
                from_captured_at: 100,
                to_captured_at: 200,
                summary_delta: TrendSummaryDelta {
                    nodes: graphify_core::diff::Delta {
                        before: 10,
                        after: 12,
                        change: 2,
                    },
                    edges: graphify_core::diff::Delta {
                        before: 20,
                        after: 24,
                        change: 4,
                    },
                    communities: graphify_core::diff::Delta {
                        before: 2,
                        after: 3,
                        change: 1,
                    },
                    cycles: graphify_core::diff::Delta {
                        before: 1,
                        after: 0,
                        change: -1,
                    },
                },
                hotspots: TrendHotspotDelta {
                    new_hotspots: vec![HotspotEntry {
                        id: "app.beta".into(),
                        score: 0.5,
                    }],
                    removed_hotspots: vec![HotspotEntry {
                        id: "app.alpha".into(),
                        score: 0.4,
                    }],
                    rising: vec![],
                    falling: vec![],
                },
                communities: CommunityChurn {
                    moved_nodes: 1,
                    stable_nodes: 4,
                    churn_pct: 20.0,
                },
            }],
        }
    }

    #[test]
    fn write_trend_markdown_includes_core_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trend-report.md");
        let report = sample_report();

        write_trend_markdown(&report, &path);

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("# Architectural Trend Report"));
        assert!(raw.contains("## Summary Over Time"));
        assert!(raw.contains("## Interval Changes"));
        assert!(raw.contains("Community churn: 1 moved, 4 stable (20.0%)"));
    }
}
