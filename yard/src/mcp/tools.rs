//! Yard-specific MCP tools. The generic primitives (`Tool`, `ToolHandler`,
//! `ToolFuture`, `wrap_text_result`) and the `catalog.*` family come from
//! `meshql-mcp`; this module contributes the analytics tools layered on top
//! of the Yard HTTP surface:
//!
//! - `environment.history` — aggregated TestRun statistics for an environment
//! - `environment.availability` — concurrent occupancy + contention windows
//! - `change_request.estimate` — resource-estimation envelope for a CR
//! - `data_sync.recommend` — recommended DataSync kind for a dependency edge
//!
//! All four handlers are thin: validate args, format a URL (or JSON body),
//! call the matching `MeshqlClient::*_path`, return the response verbatim.
//! Yard's HTTP server already does the heavy lifting — these tools just
//! surface it through MCP.

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use serde_json::{json, Map, Value};
use std::sync::Arc;

// ── argument helpers ────────────────────────────────────────────────────────

/// Read a required string-shaped argument, erroring with a useful message
/// when missing or wrong-typed. Used for `test_environment_id` /
/// `change_request_id` / `edge`.
fn required_string_arg(args: &Value, name: &str) -> anyhow::Result<String> {
    args.get(name)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("missing '{name}' argument"))
}

/// Read an optional string-shaped argument. Missing → `None`; present but
/// non-string → `None` (the caller is responsible for any stricter typing).
fn optional_string_arg(args: &Value, name: &str) -> Option<String> {
    args.get(name).and_then(|v| v.as_str()).map(String::from)
}

// ── tool handlers ───────────────────────────────────────────────────────────

fn environment_history_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "test_environment_id")?;
        client
            .get_path(&format!("/test_environment/{id}/history"))
            .await
    })
}

fn environment_availability_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "test_environment_id")?;
        client
            .get_path(&format!("/test_environment/{id}/availability"))
            .await
    })
}

fn change_request_estimate_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let id = required_string_arg(&args, "change_request_id")?;
        // Optional tier — pass through when present, send empty body when not
        // (yard's POST handler defaults the tier from the CR payload or falls
        // back to "dev").
        let body = match optional_string_arg(&args, "tier") {
            Some(tier) => {
                let mut m = Map::new();
                m.insert("tier".into(), Value::String(tier));
                Value::Object(m)
            }
            None => json!({}),
        };
        client
            .post_path(&format!("/change_request/{id}/estimate"), &body)
            .await
    })
}

fn data_sync_recommend_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let edge = required_string_arg(&args, "edge")?;
        let body = json!({ "edge": edge });
        client.post_path("/data_sync/recommend", &body).await
    })
}

// ── registry ────────────────────────────────────────────────────────────────

/// The Yard-specific tools to register alongside meshql-mcp's `catalog.*`
/// tools. The client argument lets handlers capture it into closures.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    vec![
        Tool {
            name: "environment.history",
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
            handler: Arc::new(environment_history_handler),
        },
        Tool {
            name: "environment.availability",
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
            handler: Arc::new(environment_availability_handler),
        },
        Tool {
            name: "change_request.estimate",
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
            handler: Arc::new(change_request_estimate_handler),
        },
        Tool {
            name: "data_sync.recommend",
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
            handler: Arc::new(data_sync_recommend_handler),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_string_arg_reads_string() {
        let args = json!({ "test_environment_id": "env-1" });
        assert_eq!(
            required_string_arg(&args, "test_environment_id").unwrap(),
            "env-1"
        );
    }

    #[test]
    fn required_string_arg_errors_when_missing() {
        let args = json!({});
        let err = required_string_arg(&args, "test_environment_id").unwrap_err();
        assert!(err.to_string().contains("test_environment_id"));
    }

    #[test]
    fn required_string_arg_errors_on_wrong_type() {
        let args = json!({ "test_environment_id": 42 });
        let err = required_string_arg(&args, "test_environment_id").unwrap_err();
        assert!(err.to_string().contains("test_environment_id"));
    }

    #[test]
    fn optional_string_arg_returns_some_when_present() {
        let args = json!({ "tier": "prod" });
        assert_eq!(optional_string_arg(&args, "tier"), Some("prod".to_string()));
    }

    #[test]
    fn optional_string_arg_returns_none_when_missing_or_wrong_type() {
        assert_eq!(optional_string_arg(&json!({}), "tier"), None);
        assert_eq!(optional_string_arg(&json!({ "tier": 42 }), "tier"), None);
    }

    #[test]
    fn custom_tools_lists_all_four() {
        let client = Arc::new(MeshqlClient::new("http://localhost"));
        let tools = custom_tools(client);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "environment.history",
                "environment.availability",
                "change_request.estimate",
                "data_sync.recommend",
            ]
        );
    }
}
