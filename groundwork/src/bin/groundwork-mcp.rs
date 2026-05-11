//! `groundwork-mcp` — Model Context Protocol server for the Groundwork
//! catalogue. Speaks JSON-RPC 2.0 over stdio.
//!
//! Reads `GROUNDWORK_URL` from env (default `http://localhost:3000`).
//!
//! The transport, REST client, and `catalog.*` tools live in `meshql-mcp`;
//! this binary just wires them together with the Groundwork-specific
//! `graph.*` tools.

use groundwork::mcp::custom_tools;
use meshql_mcp::{McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "GROUNDWORK_URL",
        "http://localhost:3000",
    ));
    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "groundwork-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client: client.clone(),
        entities: vec!["deployable", "service", "exposes", "dependency", "contract", "sla"],
        custom_tools: custom_tools(client),
    });
    server.serve_stdio().await
}
