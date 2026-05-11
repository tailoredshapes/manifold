//! `graph.blast_radius`, `graph.dependencies_of`, `graph.deployment_plan` —
//! each builds a fresh `Snapshot` from the live catalogue and dispatches.

use super::{Tool, ToolFuture};
use crate::mcp::graph::Snapshot;
use meshql_mcp::MeshqlClient as GroundworkClient;
use serde_json::{json, Value};
use std::sync::Arc;

fn depth_arg(args: &Value, default: usize) -> usize {
    args.get("depth")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
}

fn blast_handler(client: Arc<GroundworkClient>, args: Value) -> ToolFuture {
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
}

fn dependencies_handler(client: Arc<GroundworkClient>, args: Value) -> ToolFuture {
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
}

fn plan_handler(client: Arc<GroundworkClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let deployable_id = args
            .get("deployable_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'deployable_id' argument"))?
            .to_string();
        let snap = Snapshot::build(&client).await?;
        Ok(snap.deployment_plan(&deployable_id))
    })
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "graph.blast_radius",
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
            handler: Arc::new(blast_handler),
        },
        Tool {
            name: "graph.dependencies_of",
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
            handler: Arc::new(dependencies_handler),
        },
        Tool {
            name: "graph.deployment_plan",
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
            handler: Arc::new(plan_handler),
        },
    ]
}
