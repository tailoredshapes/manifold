//! MCP server building blocks for `cityhall-mcp`.
//!
//! The transport, HTTP client, and schema-driven `CapabilitiesBuilder`
//! machinery live in the `meshql-mcp` crate. This module just contributes
//! Cityhall-specific custom capabilities (`ancestors_of_org_node`,
//! `effective_bylaws_for_org_node`, `compute_plan_for_change_request`,
//! `render_gantt_for_plan`).

pub mod custom_capabilities;
