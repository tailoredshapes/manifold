//! Cityhall-specific MCP tools. The generic primitives (`Tool`, `ToolHandler`,
//! `ToolFuture`, `wrap_text_result`) and the `catalog.*` family come from
//! `meshql-mcp`; this module contributes the governance / planning tools
//! layered on top of the Cityhall HTTP surface.

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use std::sync::Arc;

/// The Cityhall-specific tools to register alongside meshql-mcp's `catalog.*`
/// tools. Phase-13 scaffold: handlers added in Task 14.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    vec![]
}
