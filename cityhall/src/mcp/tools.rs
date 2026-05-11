//! Cityhall-specific MCP tools. The generic primitives (`Tool`, `ToolHandler`,
//! `ToolFuture`, `wrap_text_result`) and the `catalog.*` family come from
//! `meshql-mcp`; this module contributes the governance / planning tools
//! layered on top of the Cityhall HTTP surface:
//!
//! - `org.ancestors` — chain from the enterprise root down to a leaf node
//! - `org.effective_bylaws` — cascade of bylaws applying at an org node
//! - `change_request.plan` — compute (and persist) a DeploymentPlan
//! - `deployment_plan.gantt` — render (and persist) a GanttOutput
//!
//! All four handlers are thin: validate `id`, format a URL path, call the
//! matching `MeshqlClient::*_path`, return the response verbatim. The cityhall
//! HTTP server already does the heavy lifting — these tools just surface it.

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use serde_json::{json, Value};
use std::sync::Arc;

// ── argument helpers ────────────────────────────────────────────────────────

/// Read a required string-shaped argument, erroring with a useful message
/// when missing or wrong-typed. Used for `id` / `org_node_id` /
/// `change_request_id` / `deployment_plan_id` — every Cityhall custom tool
/// is keyed on one of these.
fn required_string_arg(args: &Value, name: &str) -> anyhow::Result<String> {
    args.get(name)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("missing '{name}' argument"))
}

// ── tool handlers ───────────────────────────────────────────────────────────

fn ancestors_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "org_node_id")?;
        client
            .get_path(&format!("/org_node/{id}/ancestors"))
            .await
    })
}

fn effective_bylaws_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "org_node_id")?;
        client
            .get_path(&format!("/org_node/{id}/effective_bylaws"))
            .await
    })
}

fn plan_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "change_request_id")?;
        // Optional tier — pass through when present, send empty body when not
        // (cityhall's POST handler defaults the tier from the CR payload or
        // falls back to "dev").
        let body = match args.get("tier").and_then(|v| v.as_str()) {
            Some(tier) => json!({ "tier": tier }),
            None => json!({}),
        };
        client
            .post_path(&format!("/change_request/{id}/plan"), &body)
            .await
    })
}

fn gantt_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "deployment_plan_id")?;
        // Endpoint takes no body — POST an empty object.
        client
            .post_path(&format!("/deployment_plan/{id}/gantt"), &json!({}))
            .await
    })
}

// ── registry ────────────────────────────────────────────────────────────────

/// The Cityhall-specific tools to register alongside meshql-mcp's `catalog.*`
/// tools. The client argument lets handlers capture it into closures.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    vec![
        Tool {
            name: "org.ancestors",
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
            handler: Arc::new(ancestors_handler),
        },
        Tool {
            name: "org.effective_bylaws",
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
            handler: Arc::new(effective_bylaws_handler),
        },
        Tool {
            name: "change_request.plan",
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
            handler: Arc::new(plan_handler),
        },
        Tool {
            name: "deployment_plan.gantt",
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
            handler: Arc::new(gantt_handler),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_string_arg_reads_string() {
        let args = json!({ "org_node_id": "node-1" });
        assert_eq!(required_string_arg(&args, "org_node_id").unwrap(), "node-1");
    }

    #[test]
    fn required_string_arg_errors_when_missing() {
        let args = json!({});
        let err = required_string_arg(&args, "org_node_id").unwrap_err();
        assert!(err.to_string().contains("org_node_id"));
    }

    #[test]
    fn required_string_arg_errors_on_wrong_type() {
        let args = json!({ "org_node_id": 42 });
        let err = required_string_arg(&args, "org_node_id").unwrap_err();
        assert!(err.to_string().contains("org_node_id"));
    }

    #[test]
    fn custom_tools_lists_all_four() {
        let client = Arc::new(MeshqlClient::new("http://localhost"));
        let tools = custom_tools(client);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "org.ancestors",
                "org.effective_bylaws",
                "change_request.plan",
                "deployment_plan.gantt",
            ]
        );
    }
}
