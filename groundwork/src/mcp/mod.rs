//! MCP server building blocks for `groundwork-mcp`.
//!
//! The transport, REST client, and generic `catalog.*` tools live in the
//! `meshql-mcp` crate. This module just contributes the Groundwork-specific
//! `graph.*` tools and re-exports them so `bin/groundwork-mcp.rs` can
//! assemble them with the meshql-mcp pieces.

pub mod graph;
pub mod tools;

pub use tools::custom_tools;
