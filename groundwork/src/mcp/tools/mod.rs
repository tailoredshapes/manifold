//! MCP tool registry. Each tool is a name + JSON-Schema input + handler that
//! produces a `serde_json::Value` result.
//!
//! Phase 5 ships the read-only graph-interrogation subset (catalog + graph).
//! The IaC import/export tools are scoped out for this phase.

pub mod catalog;
pub mod graph;

use crate::mcp::client::GroundworkClient;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type ToolFuture = Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>;
pub type ToolHandler = Arc<dyn Fn(Arc<GroundworkClient>, Value) -> ToolFuture + Send + Sync>;

#[derive(Clone)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: ToolHandler,
}

pub fn all_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    tools.extend(catalog::tools());
    tools.extend(graph::tools());
    tools
}

/// Helper: wrap a `Value` result so MCP `tools/call` returns the
/// `{ content: [{ type: "text", text: "..." }] }` shape clients expect.
pub fn wrap_text_result(value: &Value) -> Value {
    serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(value).unwrap_or_default(),
        }],
        "structuredContent": value,
    })
}
