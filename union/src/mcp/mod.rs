//! MCP server building blocks for `union-mcp`.
//!
//! The transport, REST/GraphQL client, and the schema-driven
//! `CapabilitiesBuilder` machinery live in the `meshql-mcp` crate. This
//! module just contributes Union-specific custom capabilities
//! (team_capacity, members_of_team, assignments_for_person).

pub mod custom_capabilities;
