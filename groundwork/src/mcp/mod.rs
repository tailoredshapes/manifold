//! MCP server building blocks for `groundwork-mcp`.
//!
//! The transport, REST/GraphQL client, and schema-driven `Capability`
//! machinery live in the `meshql-mcp` crate. This module just contributes
//! the Groundwork-specific graph-traversal capabilities (`blast_radius`,
//! `dependencies_of`, `deployment_plan`) and re-exports them so
//! `bin/groundwork-mcp.rs` can assemble them with the meshql-mcp pieces.

pub mod custom_capabilities;
pub mod graph;
