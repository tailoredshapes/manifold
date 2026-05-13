//! MCP server for manifold-ingest. Exposes the auto-derived list/get/find +
//! create/update/delete capabilities for `Ingestion` records. Writes require
//! `MANIFOLD_USER_ID` in this binary's env (typically set in the MCP
//! client's config).

use meshql_mcp::{CapabilitiesBuilder, McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;

const INGESTION_GRAPHQL: &str = include_str!("../../config/graph/ingestion.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "INGEST_URL",
        "http://localhost:3000",
    ));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[("ingestion", "/ingestion/graph", INGESTION_GRAPHQL)])
        .describe(
            "create_ingestion",
            "Record a provenance row linking an external-system identifier \
             (e.g. a GitHub repo `owner/repo`, an Okta user id, a compose \
             filename) to the canonical id of a record created in a primary \
             Manifold domain. Adapters and LLM-driven imports MUST call this \
             after every primary-domain write so audit and disaster recovery \
             can reconstruct the link.",
        )
        .build();

    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "manifold-ingest".into(),
        server_version: env!("CARGO_PKG_VERSION").into(),
        client,
        capabilities,
    });

    server.serve_stdio().await
}
