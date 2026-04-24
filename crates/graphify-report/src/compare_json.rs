use std::path::Path;

use graphify_core::diff::DiffReport;
use serde::Serialize;

#[derive(Serialize)]
struct CompareReportEnvelope<'a> {
    left_label: &'a str,
    right_label: &'a str,
    diff: &'a DiffReport,
}

/// Writes a compare-oriented diff report as pretty-printed JSON to `path`.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_compare_json(report: &DiffReport, left_label: &str, right_label: &str, path: &Path) {
    let envelope = CompareReportEnvelope {
        left_label,
        right_label,
        diff: report,
    };
    let json = serde_json::to_string_pretty(&envelope).expect("serialize compare JSON");
    std::fs::write(path, json).expect("write compare JSON");
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::diff::*;

    fn empty_report() -> DiffReport {
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                edges: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                communities: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
                cycles: Delta {
                    before: 0,
                    after: 0,
                    change: 0,
                },
            },
            edges: EdgeDiff {
                added_nodes: vec![],
                removed_nodes: vec![],
                degree_changes: vec![],
            },
            cycles: CycleDiff {
                introduced: vec![],
                resolved: vec![],
            },
            hotspots: HotspotDiff {
                rising: vec![],
                falling: vec![],
                new_hotspots: vec![],
                removed_hotspots: vec![],
            },
            communities: CommunityDiff {
                moved_nodes: vec![],
                stable_count: 0,
            },
        }
    }

    #[test]
    fn write_compare_json_wraps_labels_and_diff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("compare-report.json");
        write_compare_json(&empty_report(), "PR-1", "PR-2", &path);

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(json["left_label"], "PR-1");
        assert_eq!(json["right_label"], "PR-2");
        assert_eq!(json["diff"]["summary_delta"]["nodes"]["change"], 0);
    }
}
