//! `union-mcp` — Model Context Protocol server for the Union catalogue
//! (people, teams, team members, work orders). Speaks JSON-RPC 2.0 over
//! stdio.
//!
//! Reads `UNION_URL` from env (default `http://localhost:3001`, the port
//! Union's HTTP server binds by docker-compose convention).
//!
//! The transport, REST client, and `catalog.*` tools live in `meshql-mcp`;
//! this binary just wires them together with the Union-specific custom
//! tools.

use meshql_mcp::{McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;
use union::mcp::custom_tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "UNION_URL",
        "http://localhost:3001",
    ));
    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "union-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client: client.clone(),
        entities: vec!["person", "team", "team_member", "work_order"],
        custom_tools: custom_tools(client),
    });
    server.serve_stdio().await
}
