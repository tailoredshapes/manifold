//! MCP server building blocks for `cityhall-mcp`.
//!
//! The transport, REST client, and generic `catalog.*` tools live in the
//! `meshql-mcp` crate. This module contributes Cityhall-specific custom
//! tools (`org.ancestors`, `org.effective_bylaws`, `change_request.plan`,
//! `deployment_plan.gantt`) and re-exports them so `bin/cityhall-mcp.rs`
//! can assemble them with the meshql-mcp pieces.

pub mod tools;

pub use tools::custom_tools;
