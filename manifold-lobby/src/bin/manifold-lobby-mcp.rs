//! MCP server for manifold-lobby. Exposes auto-derived list/get/find +
//! create/update/delete tools for advisories, programs, lifecycle entries,
//! saved views, and comments. Writes require `MANIFOLD_USER_ID`.

use meshql_mcp::{CapabilitiesBuilder, McpServerConfig, MeshqlClient, MeshqlMcpServer};
use std::sync::Arc;

const ADVISORY_GRAPHQL: &str = include_str!("../../config/graph/advisory.graphql");
const PROGRAM_GRAPHQL: &str = include_str!("../../config/graph/program.graphql");
const PROGRAM_MEMBERSHIP_GRAPHQL: &str =
    include_str!("../../config/graph/program_membership.graphql");
const LIFECYCLE_GRAPHQL: &str = include_str!("../../config/graph/lifecycle_entry.graphql");
const SAVED_VIEW_GRAPHQL: &str = include_str!("../../config/graph/saved_view.graphql");
const COMMENT_GRAPHQL: &str = include_str!("../../config/graph/comment.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env("LOBBY_URL", "http://localhost:3055"));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[
            ("advisory", "/advisory/graph", ADVISORY_GRAPHQL),
            ("program", "/program/graph", PROGRAM_GRAPHQL),
            (
                "program_membership",
                "/program_membership/graph",
                PROGRAM_MEMBERSHIP_GRAPHQL,
            ),
            (
                "lifecycle_entry",
                "/lifecycle_entry/graph",
                LIFECYCLE_GRAPHQL,
            ),
            ("saved_view", "/saved_view/graph", SAVED_VIEW_GRAPHQL),
            ("comment", "/comment/graph", COMMENT_GRAPHQL),
        ])
        .describe(
            "list_advisorys",
            "List every advisory in Lobby (federated read across all kinds, states, and programs). Use getByState/getByKind to filter.",
        )
        .describe(
            "create_advisory",
            "Create an advisory directly. Note: humans normally do NOT create advisories — the derivation engine raises them from the federated event stream. Use this only for synthetic advisories or imports.",
        )
        .build();

    let server = MeshqlMcpServer::new(McpServerConfig {
        server_name: "manifold-lobby".into(),
        server_version: env!("CARGO_PKG_VERSION").into(),
        client,
        capabilities,
    });

    server.serve_stdio().await
}
