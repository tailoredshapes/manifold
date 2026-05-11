//! Groundwork-specific custom capabilities: graph traversals (`blast_radius`,
//! `dependencies_of`, `deployment_plan`) that operate on an in-memory
//! snapshot of the catalogue (see [`super::graph::Snapshot`]).
//!
//! Each is a `Capability` with a `CapabilityHandler::Custom` handler — they
//! don't fit the templated `GraphQuery`/`RestGet`/`RestPost` dispatchers
//! because they need to fetch multiple entity collections and run a
//! domain-specific traversal in memory before responding.

use crate::mcp::graph::Snapshot;
use meshql_mcp::{Capability, CapabilityHandler, MeshqlClient, ToolFuture};
use serde_json::{json, Value};
use std::sync::Arc;

fn depth_arg(args: &Value, default: usize) -> usize {
    args.get("depth")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
}

/// `blast_radius_for_service` — given a service id, return every deployable
/// that depends on it transitively, along with the services those deployables
/// expose. Use to scope outage risk.
pub fn blast_radius(_client: Arc<MeshqlClient>) -> Capability {
    let handler: meshql_mcp::ToolHandler = Arc::new(move |client, args| -> ToolFuture {
        Box::pin(async move {
            let service_id = args
                .get("service_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'service_id' argument"))?
                .to_string();
            let depth = depth_arg(&args, 5);
            let snap = Snapshot::build(&client).await?;
            Ok(snap.blast_radius(&service_id, depth))
        })
    });

    Capability {
        name: "blast_radius_for_service",
        description: "If this service goes down, which deployables — and which services those \
                      deployables expose — break, transitively? Walks reverse-dependency edges \
                      from the named service. Use this to assess the risk of taking a service \
                      down for maintenance, or to scope the impact of an outage.",
        input_schema: json!({
            "type": "object",
            "required": ["service_id"],
            "properties": {
                "service_id": { "type": "string", "description": "Groundwork Service id." },
                "depth":      { "type": "integer", "minimum": 1, "maximum": 10, "default": 5 }
            }
        }),
        handler: CapabilityHandler::Custom(handler),
    }
}

/// `dependencies_of_deployable` — walks forward through dependency edges
/// from a deployable, recursing through publishing deployables. Returns a
/// tree where leaves are either external services or already-visited
/// deployables.
pub fn dependencies_of(_client: Arc<MeshqlClient>) -> Capability {
    let handler: meshql_mcp::ToolHandler = Arc::new(move |client, args| -> ToolFuture {
        Box::pin(async move {
            let deployable_id = args
                .get("deployable_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'deployable_id' argument"))?
                .to_string();
            let depth = depth_arg(&args, 5);
            let snap = Snapshot::build(&client).await?;
            Ok(snap.dependencies_of(&deployable_id, depth))
        })
    });

    Capability {
        name: "dependencies_of_deployable",
        description: "What does this deployable consume? Walks forward through dependency edges \
                      from the named deployable, recursing through publishing deployables. \
                      Returns a tree where leaves are either external services (no publisher in \
                      the catalogue) or already-visited deployables.",
        input_schema: json!({
            "type": "object",
            "required": ["deployable_id"],
            "properties": {
                "deployable_id": { "type": "string", "description": "Groundwork Deployable id." },
                "depth":         { "type": "integer", "minimum": 1, "maximum": 10, "default": 5 }
            }
        }),
        handler: CapabilityHandler::Custom(handler),
    }
}

/// `deployment_plan_for_deployable` — topologically sorts every deployable
/// transitively required by the target so dependencies come first. Services
/// with no publishing deployable surface as external prerequisites. Cycles
/// are reported as an error.
pub fn deployment_plan(_client: Arc<MeshqlClient>) -> Capability {
    let handler: meshql_mcp::ToolHandler = Arc::new(move |client, args| -> ToolFuture {
        Box::pin(async move {
            let deployable_id = args
                .get("deployable_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'deployable_id' argument"))?
                .to_string();
            let snap = Snapshot::build(&client).await?;
            Ok(snap.deployment_plan(&deployable_id))
        })
    });

    Capability {
        name: "deployment_plan_for_deployable",
        description: "What order should I deploy this stack in? Topologically sorts every \
                      deployable transitively required by the target so dependencies come first. \
                      Services with no publishing deployable in the catalogue are surfaced as \
                      external prerequisites. Cycles are reported as an error.",
        input_schema: json!({
            "type": "object",
            "required": ["deployable_id"],
            "properties": {
                "deployable_id": { "type": "string", "description": "Groundwork Deployable id." }
            }
        }),
        handler: CapabilityHandler::Custom(handler),
    }
}
