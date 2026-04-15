//! Install integrations subcommand: copies agent/skill/command artifacts to
//! the user's AI-client directories and registers the graphify-mcp server.

pub mod manifest;
pub mod frontmatter;
pub mod mcp_merge;
pub mod copy_plan;
pub mod codex_bridge;
