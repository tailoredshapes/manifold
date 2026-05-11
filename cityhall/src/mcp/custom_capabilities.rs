//! Cityhall-specific custom capabilities. The catalog list/get/find/by-FK
//! capabilities are auto-derived from the GraphQL schemas in
//! `CapabilitiesBuilder::auto_from_schemas`; this module contributes the
//! governance / planning capabilities that wrap Cityhall's computed REST
//! endpoints:
//!
//! - `ancestors_of_org_node` — chain from root to the given org node
//! - `effective_bylaws_for_org_node` — bylaw cascade resolved at a node
//! - `compute_plan_for_change_request` — compile + persist a DeploymentPlan
//! - `render_gantt_for_plan` — render + persist a Mermaid Gantt for a plan
//!
//! `ancestors` and `effective_bylaws` use templated `RestGet` handlers. The
//! gantt endpoint takes no body and uses a templated `RestPost` with an
//! empty body. The plan endpoint accepts an *optional* `tier` field that we
//! only want in the request body when the caller supplied it — `RestPost`'s
//! body-template substitution would leave a stray `{tier}` placeholder when
//! the arg is absent, so we drop down to a `Custom` handler.

use meshql_mcp::{Capability, CapabilityHandler, MeshqlClient, ToolFuture, ToolHandler};
use serde_json::json;
use std::sync::Arc;

/// `ancestors_of_org_node` — GET `/org_node/{org_node_id}/ancestors`.
pub fn ancestors_of_org_node() -> Capability {
    Capability {
        name: "ancestors_of_org_node",
        description: "Where in the org chart does this node sit? Returns the chain from \
                      the enterprise root down to (and including) the given node. Useful for \
                      building breadcrumbs or for reasoning about bylaw inheritance.",
        input_schema: json!({
            "type": "object",
            "required": ["org_node_id"],
            "properties": {
                "org_node_id": { "type": "string", "description": "Cityhall OrgNode id." }
            }
        }),
        handler: CapabilityHandler::RestGet {
            path_template: "/org_node/{org_node_id}/ancestors".to_string(),
        },
    }
}

/// `effective_bylaws_for_org_node` — GET
/// `/org_node/{org_node_id}/effective_bylaws`.
pub fn effective_bylaws_for_org_node() -> Capability {
    Capability {
        name: "effective_bylaws_for_org_node",
        description: "Which governance bylaws apply here? Returns the cascade of bylaws \
                      attached to this node and any ancestor — the same resolution the \
                      plan compiler uses when evaluating gates for a deployment.",
        input_schema: json!({
            "type": "object",
            "required": ["org_node_id"],
            "properties": {
                "org_node_id": { "type": "string", "description": "Cityhall OrgNode id." }
            }
        }),
        handler: CapabilityHandler::RestGet {
            path_template: "/org_node/{org_node_id}/effective_bylaws".to_string(),
        },
    }
}

/// `compute_plan_for_change_request` — POST
/// `/change_request/{change_request_id}/plan` with an optional `tier`. Uses
/// a `Custom` handler because the body should only carry `tier` when the
/// caller supplied it; the templated `RestPost` would always include it.
pub fn compute_plan_for_change_request(_client: Arc<MeshqlClient>) -> Capability {
    let handler: ToolHandler = Arc::new(move |client, args| -> ToolFuture {
        Box::pin(async move {
            let id = args
                .get("change_request_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'change_request_id' argument"))?
                .to_string();
            let body = match args.get("tier").and_then(|v| v.as_str()) {
                Some(tier) => json!({ "tier": tier }),
                None => json!({}),
            };
            client
                .post_path(&format!("/change_request/{id}/plan"), &body)
                .await
        })
    });

    Capability {
        name: "compute_plan_for_change_request",
        description: "Compile a DeploymentPlan for a ChangeRequest. Resolves the target \
                      deployables and their dependencies via Groundwork, applies effective \
                      bylaws as gates, and persists the resulting plan envelope. Returns \
                      the saved plan (id + payload with steps, blockers, computed_at).",
        input_schema: json!({
            "type": "object",
            "required": ["change_request_id"],
            "properties": {
                "change_request_id": {
                    "type": "string",
                    "description": "Cityhall ChangeRequest id."
                },
                "tier": {
                    "type": "string",
                    "description": "Optional deployment tier (e.g. \"dev\", \"prod\"). \
                                    Defaults to the CR's tier, or \"dev\" if unset."
                }
            }
        }),
        handler: CapabilityHandler::Custom(handler),
    }
}

/// `render_gantt_for_plan` — POST `/deployment_plan/{deployment_plan_id}/gantt`
/// with an empty body. The endpoint takes no parameters.
pub fn render_gantt_for_plan() -> Capability {
    Capability {
        name: "render_gantt_for_plan",
        description: "Render a Mermaid Gantt chart for a DeploymentPlan and persist the \
                      GanttOutput envelope. Returns the saved gantt (id + payload.mermaid). \
                      Output is deterministic for a given plan.",
        input_schema: json!({
            "type": "object",
            "required": ["deployment_plan_id"],
            "properties": {
                "deployment_plan_id": {
                    "type": "string",
                    "description": "Cityhall DeploymentPlan id."
                }
            }
        }),
        handler: CapabilityHandler::RestPost {
            path_template: "/deployment_plan/{deployment_plan_id}/gantt".to_string(),
            body_template: Some(json!({})),
        },
    }
}
