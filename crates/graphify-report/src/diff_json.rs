use std::path::Path;

use graphify_core::diff::DiffReport;

/// Writes the diff report as pretty-printed JSON to `path`.
///
/// # Panics
/// Panics if serialization or file I/O fails.
pub fn write_diff_json(report: &DiffReport, path: &Path) {
    let json = serde_json::to_string_pretty(report).expect("serialize diff JSON");
    std::fs::write(path, json).expect("write diff JSON");
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphify_core::diff::*;

    fn empty_report() -> DiffReport {
        DiffReport {
            summary_delta: SummaryDelta {
                nodes: Delta { before: 5, after: 5, change: 0 },
                edges: Delta { before: 10, after: 10, change: 0 },
                communities: Delta { before: 2, after: 2, change: 0 },
                cycles: Delta { before: 0, after: 0, change: 0 },
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
                stable_count: 5,
            },
        }
    }

    #[test]
    fn write_diff_json_creates_valid_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drift-report.json");
        let report = empty_report();
        write_diff_json(&report, &path);

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(value["summary_delta"]["nodes"]["change"], 0);
        assert_eq!(value["communities"]["stable_count"], 5);
    }

    #[test]
    fn write_diff_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drift-report.json");
        let report = empty_report();
        write_diff_json(&report, &path);

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(value["summary_delta"].is_object());
        assert!(value["edges"].is_object());
        assert!(value["cycles"].is_object());
        assert!(value["hotspots"].is_object());
        assert!(value["communities"].is_object());
    }
}
