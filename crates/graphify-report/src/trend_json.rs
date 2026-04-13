use std::path::Path;

use graphify_core::history::TrendReport;

/// Writes the trend report as pretty-printed JSON to `path`.
pub fn write_trend_json(report: &TrendReport, path: &Path) {
    let json = serde_json::to_string_pretty(report).expect("serialize trend JSON");
    std::fs::write(path, json).expect("write trend JSON");
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
            points: vec![TrendPoint {
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
            }],
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
                    removed_hotspots: vec![],
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
    fn write_trend_json_creates_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trend-report.json");
        let report = sample_report();

        write_trend_json(&report, &path);

        let raw = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["project"], "demo");
        assert_eq!(value["snapshot_count"], 2);
        assert_eq!(value["window"]["first_captured_at"], 100);
    }
}
