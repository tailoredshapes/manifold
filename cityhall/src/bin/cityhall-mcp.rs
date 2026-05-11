//! `cityhall-mcp` — Model Context Protocol server for the Cityhall catalogue
//! (org hierarchy, governance bylaws, change requests, deployment plans, and
//! Mermaid Gantt output). Speaks JSON-RPC 2.0 over stdio.
//!
//! Reads `CITYHALL_URL` from env (default `http://localhost:3000`, matching
//! groundwork-mcp's convention even though Cityhall's HTTP server binds 3002
//! by docker-compose convention — operators are expected to set the env var).
//!
//! The transport, REST client, and `catalog.*` tools live in `meshql-mcp`;
//! this binary just wires them together with the Cityhall-specific custom
//! tools.

use cityhall::mcp::custom_tools;
use meshql_mcp::{McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "CITYHALL_URL",
        "http://localhost:3000",
    ));
    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "cityhall-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client: client.clone(),
        entities: vec![
            "org_node",
            "bylaw",
            "change_request",
            "deployment_plan",
            "gantt_output",
        ],
        custom_tools: custom_tools(client),
    });
    server.serve_stdio().await
}
