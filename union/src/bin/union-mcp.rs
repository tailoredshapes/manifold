//! `union-mcp` — Model Context Protocol server for the Union catalogue
//! (people, teams, team members, work orders). Speaks JSON-RPC 2.0 over
//! stdio.
//!
//! Reads `UNION_URL` from env (default `http://localhost:3001`, the port
//! Union's HTTP server binds by docker-compose convention).
//!
//! Catalog capabilities are auto-derived from `config/graph/*.graphql`;
//! Union-specific custom capabilities (team capacity, members of a team,
//! a person's open assignments) are layered on top.

use meshql_mcp::{CapabilitiesBuilder, McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;
use union::mcp::custom_capabilities;

const PERSON_GRAPHQL: &str = include_str!("../../config/graph/person.graphql");
const TEAM_GRAPHQL: &str = include_str!("../../config/graph/team.graphql");
const TEAM_MEMBER_GRAPHQL: &str = include_str!("../../config/graph/team_member.graphql");
const WORK_ORDER_GRAPHQL: &str = include_str!("../../config/graph/work_order.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env(
        "UNION_URL",
        "http://localhost:3001",
    ));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[
            ("person", "/person/graph", PERSON_GRAPHQL),
            ("team", "/team/graph", TEAM_GRAPHQL),
            ("team_member", "/team_member/graph", TEAM_MEMBER_GRAPHQL),
            ("work_order", "/work_order/graph", WORK_ORDER_GRAPHQL),
        ])
        .describe(
            "list_persons",
            "List every person registered in Union, including role and contact metadata. \
             Note: the auto-generated plural is `persons` (Union's `Person` entity); the \
             singular accessor is get_person_by_id.",
        )
        .describe(
            "list_teams",
            "List every team in Union with its kind and description. Use team_capacity to \
             gauge load for a specific team.",
        )
        .add(custom_capabilities::team_capacity(client.clone()))
        .add(custom_capabilities::members_of_team(client.clone()))
        .add(custom_capabilities::assignments_for_person(client.clone()))
        .build();

    eprintln!(
        "union-mcp v{} → {} ({} capabilities)",
        env!("CARGO_PKG_VERSION"),
        client.base_url(),
        capabilities.len()
    );

    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "union-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        client,
        capabilities,
    });
    server.serve_stdio().await
}
