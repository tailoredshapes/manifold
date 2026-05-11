//! `cityhall-mcp` — Model Context Protocol server for the Cityhall catalogue
//! (org hierarchy, governance bylaws, change requests, deployment plans, and
//! Mermaid Gantt output). Speaks JSON-RPC 2.0 over stdio.
//!
//! Reads `CITYHALL_URL` from env (default `http://localhost:3000`, matching
//! groundwork-mcp's convention even though Cityhall's HTTP server binds 3002
//! by docker-compose convention — operators are expected to set the env var).
//!
//! Catalog capabilities are auto-derived from `config/graph/*.graphql`;
//! Cityhall-specific custom capabilities (org ancestors, effective bylaws,
//! plan compilation, Gantt rendering) are layered on top with their dotted
//! names rewritten to underscore form.

use cityhall::mcp::custom_capabilities;
use meshql_mcp::{CapabilitiesBuilder, McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;

const ORG_NODE_GRAPHQL: &str = include_str!("../../config/graph/org_node.graphql");
const BYLAW_GRAPHQL: &str = include_str!("../../config/graph/bylaw.graphql");
const CHANGE_REQUEST_GRAPHQL: &str = include_str!("../../config/graph/change_request.graphql");
const DEPLOYMENT_PLAN_GRAPHQL: &str = include_str!("../../config/graph/deployment_plan.graphql");
const GANTT_OUTPUT_GRAPHQL: &str = include_str!("../../config/graph/gantt_output.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "CITYHALL_URL",
        "http://localhost:3000",
    ));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[
            ("org_node", "/org_node/graph", ORG_NODE_GRAPHQL),
            ("bylaw", "/bylaw/graph", BYLAW_GRAPHQL),
            (
                "change_request",
                "/change_request/graph",
                CHANGE_REQUEST_GRAPHQL,
            ),
            (
                "deployment_plan",
                "/deployment_plan/graph",
                DEPLOYMENT_PLAN_GRAPHQL,
            ),
            ("gantt_output", "/gantt_output/graph", GANTT_OUTPUT_GRAPHQL),
        ])
        .describe(
            "list_org_nodes",
            "List every node in the Cityhall org hierarchy (enterprise / division / domain / \
             team). Use ancestors_of_org_node to walk up from a leaf, or list_bylaws to see \
             the governance rules attached to nodes.",
        )
        .describe(
            "list_change_requests",
            "List every ChangeRequest with its summary, status, and tier. Use \
             compute_plan_for_change_request to compile a deployment plan against a CR.",
        )
        .add(custom_capabilities::ancestors_of_org_node())
        .add(custom_capabilities::effective_bylaws_for_org_node())
        .add(custom_capabilities::compute_plan_for_change_request(
            client.clone(),
        ))
        .add(custom_capabilities::render_gantt_for_plan())
        .build();

    eprintln!(
        "cityhall-mcp v{} → {} ({} capabilities)",
        env!("CARGO_PKG_VERSION"),
        client.base_url(),
        capabilities.len()
    );

    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "cityhall-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client,
        capabilities,
    });
    server.serve_stdio().await
}
