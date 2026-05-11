//! Yard-specific MCP tools. (Stub for Task 16 — populated in Task 17.)

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use std::sync::Arc;

/// The Yard-specific tools to register alongside meshql-mcp's `catalog.*`
/// tools. The client argument lets handlers capture it into closures.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    vec![]
}
