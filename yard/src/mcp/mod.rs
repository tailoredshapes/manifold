//! MCP server building blocks for `yard-mcp`.
//!
//! The transport, REST client, and generic `catalog.*` tools live in the
//! `meshql-mcp` crate. This module contributes Yard-specific custom tools
//! (`environment.history`, `environment.availability`,
//! `change_request.estimate`, `data_sync.recommend`) and re-exports them so
//! `bin/yard-mcp.rs` can assemble them with the meshql-mcp pieces.

pub mod tools;

pub use tools::custom_tools;
