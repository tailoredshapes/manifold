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
//! Catalog capabilities are auto-derived from `config/graph/*.graphql`;
//! Yard-specific analytics capabilities (environment history/availability,
//! CR estimates, sync recommendations) are layered on top with their dotted
//! names rewritten to underscore form.

use meshql_mcp::{CapabilitiesBuilder, McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;
use yard::mcp::custom_capabilities;

const TEST_ENVIRONMENT_GRAPHQL: &str = include_str!("../../config/graph/test_environment.graphql");
const TEST_INFRASTRUCTURE_GRAPHQL: &str =
    include_str!("../../config/graph/test_infrastructure.graphql");
const MOCK_SOURCE_GRAPHQL: &str = include_str!("../../config/graph/mock_source.graphql");
const DATA_SOURCE_GRAPHQL: &str = include_str!("../../config/graph/data_source.graphql");
const DATA_SYNC_GRAPHQL: &str = include_str!("../../config/graph/data_sync.graphql");
const TEST_RUN_GRAPHQL: &str = include_str!("../../config/graph/test_run.graphql");
const TEST_SUITE_GRAPHQL: &str = include_str!("../../config/graph/test_suite.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env("YARD_URL", "http://localhost:3000"));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[
            (
                "test_environment",
                "/test_environment/graph",
                TEST_ENVIRONMENT_GRAPHQL,
            ),
            (
                "test_infrastructure",
                "/test_infrastructure/graph",
                TEST_INFRASTRUCTURE_GRAPHQL,
            ),
            ("mock_source", "/mock_source/graph", MOCK_SOURCE_GRAPHQL),
            ("data_source", "/data_source/graph", DATA_SOURCE_GRAPHQL),
            ("data_sync", "/data_sync/graph", DATA_SYNC_GRAPHQL),
            ("test_run", "/test_run/graph", TEST_RUN_GRAPHQL),
            ("test_suite", "/test_suite/graph", TEST_SUITE_GRAPHQL),
        ])
        .describe(
            "list_test_environments",
            "List every TestEnvironment with its kind, federated deployable/service/\
             infrastructure metadata, and the contractual / concurrency limits. Use \
             history_for_environment for past runs, availability_for_environment for \
             real-time occupancy.",
        )
        .describe(
            "list_test_runs",
            "List every TestRun with its status, duration, and federated \
             environment/change_request/team/suite metadata. For aggregated stats per \
             environment use history_for_environment.",
        )
        .add(custom_capabilities::history_for_environment())
        .add(custom_capabilities::availability_for_environment())
        .add(custom_capabilities::estimate_for_change_request(
            client.clone(),
        ))
        .add(custom_capabilities::recommend_data_sync())
        .build();

    eprintln!(
        "yard-mcp v{} → {} ({} capabilities)",
        env!("CARGO_PKG_VERSION"),
        client.base_url(),
        capabilities.len()
    );

    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "yard-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client,
        capabilities,
    });
    server.serve_stdio().await
}
