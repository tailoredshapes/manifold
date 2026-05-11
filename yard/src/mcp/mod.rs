//! MCP server building blocks for `yard-mcp`.
//!
//! The transport, HTTP client, and schema-driven `CapabilitiesBuilder`
//! machinery live in the `meshql-mcp` crate. This module just contributes
//! Yard-specific custom capabilities (`history_for_environment`,
//! `availability_for_environment`, `estimate_for_change_request`,
//! `recommend_data_sync`).

pub mod custom_capabilities;
