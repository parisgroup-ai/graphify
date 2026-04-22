use std::fs;

use graphify_core::history::{
    compute_trend_report, load_historical_snapshots, ConfidenceSummary, HistoricalCommunity,
    HistoricalNode, HistoricalSnapshot, HotspotEntry, SummarySnapshot,
};
use tempfile::TempDir;

fn make_snapshot(
    captured_at: u128,
    total_nodes: usize,
    total_edges: usize,
    total_cycles: usize,
    top_hotspots: Vec<(&str, f64)>,
    nodes: Vec<(&str, f64, usize)>,
    communities: Vec<(usize, Vec<&str>)>,
) -> HistoricalSnapshot {
    HistoricalSnapshot {
        captured_at,
        project: "demo".into(),
        summary: SummarySnapshot {
            total_nodes,
            total_edges,
            total_communities: communities.len(),
            total_cycles,
        },
        top_hotspots: top_hotspots
            .into_iter()
            .map(|(id, score)| HotspotEntry {
                id: id.into(),
                score,
            })
            .collect(),
        confidence_summary: ConfidenceSummary {
            extracted_count: 8,
            extracted_pct: 80.0,
            inferred_count: 2,
            inferred_pct: 20.0,
            ambiguous_count: 0,
            ambiguous_pct: 0.0,
            expected_external_count: 0,
            expected_external_pct: 0.0,
            mean_confidence: 0.92,
        },
        nodes: nodes
            .into_iter()
            .map(|(id, score, community_id)| HistoricalNode {
                id: id.into(),
                score,
                community_id,
                in_degree: 1,
                out_degree: 1,
                in_cycle: false,
            })
            .collect(),
        communities: communities
            .into_iter()
            .map(|(id, members)| HistoricalCommunity {
                id,
                members: members.into_iter().map(|member| member.into()).collect(),
                cohesion: 0.0,
            })
            .collect(),
    }
}

#[test]
fn compute_trend_report_sorts_limits_and_aggregates_adjacent_changes() {
    let snapshots = vec![
        make_snapshot(
            300,
            14,
            22,
            0,
            vec![("app.beta", 0.55), ("app.gamma", 0.51)],
            vec![
                ("app.alpha", 0.43, 4),
                ("app.beta", 0.55, 4),
                ("app.gamma", 0.51, 5),
            ],
            vec![(4, vec!["app.alpha", "app.beta"]), (5, vec!["app.gamma"])],
        ),
        make_snapshot(
            100,
            10,
            18,
            2,
            vec![("app.alpha", 0.40), ("app.beta", 0.35)],
            vec![("app.alpha", 0.40, 1), ("app.beta", 0.35, 2)],
            vec![(1, vec!["app.alpha"]), (2, vec!["app.beta"])],
        ),
        make_snapshot(
            200,
            12,
            20,
            1,
            vec![("app.alpha", 0.44), ("app.beta", 0.47)],
            vec![("app.alpha", 0.44, 8), ("app.beta", 0.47, 9)],
            vec![(8, vec!["app.alpha"]), (9, vec!["app.beta"])],
        ),
    ];

    let report = compute_trend_report("demo", &snapshots, Some(2)).expect("trend report");

    assert_eq!(report.project, "demo");
    assert_eq!(report.snapshot_count, 2);
    assert_eq!(report.window.first_captured_at, 200);
    assert_eq!(report.window.last_captured_at, 300);
    assert_eq!(report.points.len(), 2);
    assert_eq!(report.points[0].captured_at, 200);
    assert_eq!(report.points[1].captured_at, 300);
    assert_eq!(report.points[0].total_cycles, 1);
    assert_eq!(report.points[1].total_cycles, 0);

    assert_eq!(report.intervals.len(), 1);
    let interval = &report.intervals[0];
    assert_eq!(interval.summary_delta.nodes.before, 12);
    assert_eq!(interval.summary_delta.nodes.after, 14);
    assert_eq!(interval.summary_delta.nodes.change, 2);
    assert_eq!(interval.summary_delta.cycles.change, -1);
    assert_eq!(interval.hotspots.new_hotspots.len(), 1);
    assert_eq!(interval.hotspots.new_hotspots[0].id, "app.gamma");
    assert_eq!(interval.hotspots.removed_hotspots.len(), 1);
    assert_eq!(interval.hotspots.removed_hotspots[0].id, "app.alpha");
    assert_eq!(interval.hotspots.rising.len(), 1);
    assert_eq!(interval.hotspots.rising[0].id, "app.beta");
    assert_eq!(interval.communities.moved_nodes, 1);
    assert_eq!(interval.communities.stable_nodes, 1);
    assert_eq!(interval.communities.churn_pct, 50.0);
}

#[test]
fn load_historical_snapshots_reads_and_sorts_history_directory() {
    let tmp = TempDir::new().expect("temp dir");
    let history_dir = tmp.path().join("history");
    fs::create_dir_all(&history_dir).expect("history dir");

    let later = make_snapshot(
        200,
        12,
        20,
        1,
        vec![("app.alpha", 0.50)],
        vec![("app.alpha", 0.50, 1)],
        vec![(1, vec!["app.alpha"])],
    );
    let earlier = make_snapshot(
        100,
        10,
        18,
        2,
        vec![("app.alpha", 0.40)],
        vec![("app.alpha", 0.40, 1)],
        vec![(1, vec!["app.alpha"])],
    );

    fs::write(
        history_dir.join("200.json"),
        serde_json::to_string_pretty(&later).expect("serialize later"),
    )
    .expect("write later snapshot");
    fs::write(
        history_dir.join("100.json"),
        serde_json::to_string_pretty(&earlier).expect("serialize earlier"),
    )
    .expect("write earlier snapshot");

    let loaded = load_historical_snapshots(&history_dir).expect("load historical snapshots");

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].captured_at, 100);
    assert_eq!(loaded[1].captured_at, 200);
}
