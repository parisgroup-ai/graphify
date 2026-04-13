pub mod csv;
pub mod graphml;
pub mod html;
pub mod json;
pub mod markdown;
pub mod neo4j;
pub mod obsidian;

// Re-export the main write functions for convenience.
pub use csv::{write_edges_csv, write_nodes_csv};
pub use graphml::write_graphml;
pub use html::write_html;
pub use json::{write_analysis_json, write_graph_json};
pub use markdown::write_report;
pub use neo4j::write_cypher;
pub use obsidian::write_obsidian_vault;

// Re-export core types used across the report modules.
pub use graphify_core::community::Community;

/// A cycle represented as an ordered list of node IDs.
pub type Cycle = Vec<String>;
