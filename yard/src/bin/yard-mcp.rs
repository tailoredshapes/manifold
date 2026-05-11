//! `yard-mcp` — Model Context Protocol server for the Yard catalogue (test
//! environments, test infrastructure, mock sources, data sources, data syncs,
//! test runs, test suites) and Yard's analytics surface (history,
//! availability, change-request estimates, sync recommendations). Speaks
//! JSON-RPC 2.0 over stdio.
//!
//! Reads `YARD_URL` from env (default `http://localhost:3000`, matching the
//! groundwork-mcp / cityhall-mcp convention even though Yard's HTTP server
//! binds 3003 by docker-compose convention — operators are expected to set
//! the env var).
//!
//! The transport, REST client, and `catalog.*` tools live in `meshql-mcp`;
//! this binary just wires them together with the Yard-specific custom tools.

use meshql_mcp::{McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;
use yard::mcp::custom_tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "YARD_URL",
        "http://localhost:3000",
    ));
    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "yard-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client: client.clone(),
        entities: vec![
            "test_environment",
            "test_infrastructure",
            "mock_source",
            "data_source",
            "data_sync",
            "test_run",
            "test_suite",
        ],
        custom_tools: custom_tools(client),
    });
    server.serve_stdio().await
}
