//! Groundwork-specific MCP tools. The generic primitives (`Tool`,
//! `ToolHandler`, `ToolFuture`, `wrap_text_result`) and the `catalog.*`
//! family come from `meshql-mcp`; this module just contributes the
//! `graph.*` family.

pub mod graph;

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use std::sync::Arc;

/// The Groundwork-specific tools to register alongside meshql-mcp's
/// `catalog.*` tools. Currently just the graph queries.
///
/// The client argument is unused here — graph handlers receive the
/// client from `MeshqlMcpServer` at call time — but the signature
/// keeps the door open for tools that need to capture it into a closure.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    graph::tools()
}
