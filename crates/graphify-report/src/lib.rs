pub mod csv;
pub mod json;
pub mod markdown;

// Re-export the main write functions for convenience.
pub use csv::{write_edges_csv, write_nodes_csv};
pub use json::{write_analysis_json, write_graph_json};
pub use markdown::write_report;

// Re-export core types used across the report modules.
pub use graphify_core::community::Community;

/// A cycle represented as an ordered list of node IDs.
pub type Cycle = Vec<String>;
