//! MCP server building blocks for `union-mcp`.
//!
//! The transport, REST client, and generic `catalog.*` tools live in the
//! `meshql-mcp` crate. This module just contributes Union-specific custom
//! tools (`team.capacity`, `team.members`, `person.assignments`) and
//! re-exports them so `bin/union-mcp.rs` can assemble them with the
//! meshql-mcp pieces.

pub mod tools;

pub use tools::custom_tools;
