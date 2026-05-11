//! `groundwork-mcp` — Model Context Protocol server for the Groundwork
//! catalogue. Speaks JSON-RPC 2.0 over stdio.
//!
//! Reads `GROUNDWORK_URL` from env (default `http://localhost:3000`).
//!
//! The transport, REST/GraphQL client, and `CapabilitiesBuilder` machinery
//! live in `meshql-mcp`; this binary just wires schema-driven catalog
//! capabilities (auto-derived from the `config/graph/*.graphql` files) with
//! the Groundwork-specific graph-traversal capabilities (`blast_radius`,
//! `dependencies_of`, `deployment_plan`).

use groundwork::mcp::custom_capabilities;
use meshql_mcp::{CapabilitiesBuilder, McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;

const DEPLOYABLE_GRAPHQL: &str = include_str!("../../config/graph/deployable.graphql");
const SERVICE_GRAPHQL: &str = include_str!("../../config/graph/service.graphql");
const EXPOSES_GRAPHQL: &str = include_str!("../../config/graph/exposes.graphql");
const DEPENDENCY_GRAPHQL: &str = include_str!("../../config/graph/dependency.graphql");
const CONTRACT_GRAPHQL: &str = include_str!("../../config/graph/contract.graphql");
const SLA_GRAPHQL: &str = include_str!("../../config/graph/sla.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "GROUNDWORK_URL",
        "http://localhost:3000",
    ));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[
            ("deployable", "/deployable/graph", DEPLOYABLE_GRAPHQL),
            ("service", "/service/graph", SERVICE_GRAPHQL),
            ("exposes", "/exposes/graph", EXPOSES_GRAPHQL),
            ("dependency", "/dependency/graph", DEPENDENCY_GRAPHQL),
            ("contract", "/contract/graph", CONTRACT_GRAPHQL),
            ("sla", "/sla/graph", SLA_GRAPHQL),
        ])
        .describe(
            "list_deployables",
            "List every deployable in the Groundwork catalogue, including federated team \
             metadata and current deployment_status. Use this for inventory; for one record \
             by id use get_deployable_by_id.",
        )
        .describe(
            "get_service_by_id",
            "Fetch one service by UUID. Use blast_radius_for_service if you want to know \
             which deployables depend on it.",
        )
        .add(custom_capabilities::blast_radius(client.clone()))
        .add(custom_capabilities::dependencies_of(client.clone()))
        .add(custom_capabilities::deployment_plan(client.clone()))
        .build();

    eprintln!(
        "groundwork-mcp v{} → {} ({} capabilities)",
        env!("CARGO_PKG_VERSION"),
        client.base_url(),
        capabilities.len()
    );

    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "groundwork-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client,
        capabilities,
    });
    server.serve_stdio().await
}
