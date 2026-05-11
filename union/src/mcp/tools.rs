//! Union-specific MCP tools. The generic primitives (`Tool`, `ToolHandler`,
//! `ToolFuture`, `wrap_text_result`) and the `catalog.*` family come from
//! `meshql-mcp`; this module contributes the people-and-work tools layered
//! on top of the Union catalogue.
//!
//! Custom tools land in the next commit; this file currently registers none.

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use std::sync::Arc;

/// The Union-specific tools to register alongside meshql-mcp's `catalog.*`
/// tools. The client argument lets handlers capture it into closures.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    vec![]
}
