pub mod check_report;
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
pub mod pr_summary;
pub mod trend_json;
pub mod trend_markdown;

// Re-export the main write functions for convenience.
pub use check_report::{
    CheckLimits, CheckReport, CheckViolation, PolicyCheckSummary, ProjectCheckResult,
    ProjectCheckSummary,
};
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

// Re-export core types used across the report modules.
pub use graphify_core::community::Community;

/// A cycle represented as an ordered list of node IDs.
pub type Cycle = Vec<String>;
