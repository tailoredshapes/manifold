//! Yard-specific custom capabilities. The catalog list/get/find/by-FK
//! capabilities are auto-derived from the GraphQL schemas in
//! `CapabilitiesBuilder::auto_from_schemas`; this module contributes the
//! analytics capabilities layered on top of Yard's HTTP surface:
//!
//! - `history_for_environment` — aggregated TestRun stats for an environment
//! - `availability_for_environment` — concurrent occupancy + contention
//! - `estimate_for_change_request` — resource-estimation envelope for a CR
//! - `recommend_data_sync` — recommended DataSync.kind for a dep-edge
//!
//! The two GET capabilities use templated `RestGet`. The recommend POST
//! takes a single required `edge` body field and uses templated `RestPost`.
//! The CR estimate POST accepts an *optional* `tier` body field — a
//! `RestPost` body template would always emit a `tier` placeholder, so we
//! drop to a `Custom` handler that only includes the field when supplied.

use meshql_mcp::{Capability, CapabilityHandler, MeshqlClient, ToolFuture, ToolHandler};
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// `history_for_environment` — GET
/// `/test_environment/{test_environment_id}/history`.
pub fn history_for_environment() -> Capability {
    Capability {
        name: "history_for_environment",
        description: "How has this test environment performed recently? Returns aggregated \
                      TestRun statistics for the given environment: run count, pass/fail \
                      counts, average duration and cost, pass rate. Useful for capacity \
                      planning and for the change-request estimator.",
        input_schema: json!({
            "type": "object",
            "required": ["test_environment_id"],
            "properties": {
                "test_environment_id": {
                    "type": "string",
                    "description": "Yard TestEnvironment id."
                }
            }
        }),
        handler: CapabilityHandler::RestGet {
            path_template: "/test_environment/{test_environment_id}/history".to_string(),
        },
    }
}

/// `availability_for_environment` — GET
/// `/test_environment/{test_environment_id}/availability`.
pub fn availability_for_environment() -> Capability {
    Capability {
        name: "availability_for_environment",
        description: "Can this test environment take another run right now? Returns the \
                      environment's current running count alongside its concurrency and \
                      contractual limits, and a derived availability flag (with reason \
                      when unavailable).",
        input_schema: json!({
            "type": "object",
            "required": ["test_environment_id"],
            "properties": {
                "test_environment_id": {
                    "type": "string",
                    "description": "Yard TestEnvironment id."
                }
            }
        }),
        handler: CapabilityHandler::RestGet {
            path_template: "/test_environment/{test_environment_id}/availability".to_string(),
        },
    }
}

/// `estimate_for_change_request` — POST
/// `/change_request/{change_request_id}/estimate` with an optional `tier`.
/// Custom handler so the body is `{}` (not `{"tier": null}`) when the
/// caller omits the tier.
pub fn estimate_for_change_request(_client: Arc<MeshqlClient>) -> Capability {
    let handler: ToolHandler = Arc::new(move |client, args| -> ToolFuture {
        Box::pin(async move {
            let id = args
                .get("change_request_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'change_request_id' argument"))?
                .to_string();
            let body = match args.get("tier").and_then(|v| v.as_str()) {
                Some(tier) => {
                    let mut m = Map::new();
                    m.insert("tier".into(), Value::String(tier.to_string()));
                    Value::Object(m)
                }
                None => json!({}),
            };
            client
                .post_path(&format!("/change_request/{id}/estimate"), &body)
                .await
        })
    });

    Capability {
        name: "estimate_for_change_request",
        description: "Estimate the resources a ChangeRequest will consume to test. Walks \
                      the target deployables, looks up their test environments and \
                      infrastructures, and returns a resource-estimation envelope (cost, \
                      duration, contention warnings). Reads the CR from Cityhall.",
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

/// `recommend_data_sync` — POST `/data_sync/recommend` with `{ edge: ... }`.
pub fn recommend_data_sync() -> Capability {
    Capability {
        name: "recommend_data_sync",
        description: "Given a Groundwork dependency edge between two deployables, recommend \
                      the right DataSync.kind (push / pull / shared) for the equivalent \
                      test-env edge and explain the rationale. Accepted edge aliases \
                      include event/api/shared_db (and common synonyms).",
        input_schema: json!({
            "type": "object",
            "required": ["edge"],
            "properties": {
                "edge": {
                    "type": "string",
                    "description": "Dependency-edge identifier — one of \"event\", \"api\", \
                                    \"shared_db\" (or aliases like \"events\", \"rest\", \
                                    \"database\")."
                }
            }
        }),
        handler: CapabilityHandler::RestPost {
            path_template: "/data_sync/recommend".to_string(),
            body_template: Some(json!({ "edge": "{edge}" })),
        },
    }
}
