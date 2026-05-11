//! Union-specific MCP tools. The generic primitives (`Tool`, `ToolHandler`,
//! `ToolFuture`, `wrap_text_result`) and the `catalog.*` family come from
//! `meshql-mcp`; this module contributes the people-and-work tools layered
//! on top of the Union catalogue:
//!
//! - `team.capacity` — sums open story points for a team
//! - `team.members` — resolves Person rows for a team via TeamMember
//! - `person.assignments` — open WorkOrders across the teams a person belongs to
//!
//! All three handlers fetch the relevant entity lists with `client.list(...)`
//! and filter / map in memory. Cheap data, simple semantics — if any of this
//! shows up hot in profiling it can grow a snapshot/cache later.

pub use meshql_mcp::{wrap_text_result, Tool, ToolFuture, ToolHandler};

use meshql_mcp::MeshqlClient;
use serde_json::{json, Value};
use std::sync::Arc;

// ── envelope field helpers ───────────────────────────────────────────────────

/// Read `env.id` as a string.
fn env_id(env: &Value) -> String {
    env.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// Read either `env.payload.<key>` (nested) or `env.<key>` (flat) — meshql
/// envelopes nest user fields under `payload` but some endpoints flatten.
fn payload_str(env: &Value, key: &str) -> String {
    if let Some(p) = env.get("payload").and_then(|p| p.as_object()) {
        if let Some(v) = p.get(key).and_then(|v| v.as_str()) {
            return v.to_string();
        }
    }
    env.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// Read `env.payload.<key>` (or flat fallback) as an i64. Treats missing /
/// null / wrong-type as 0 — matches the union frontend's `Number.isFinite`
/// guard around `story_points`.
fn payload_i64(env: &Value, key: &str) -> i64 {
    let fetch = |obj: &Value| obj.get(key).and_then(|v| v.as_i64());
    if let Some(p) = env.get("payload") {
        if let Some(n) = fetch(p) {
            return n;
        }
    }
    fetch(env).unwrap_or(0)
}

// ── tool handlers ────────────────────────────────────────────────────────────

fn capacity_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let team_id = args
            .get("team_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'team_id' argument"))?
            .to_string();

        let (teams_v, work_orders_v, members_v) = tokio::try_join!(
            client.list("team"),
            client.list("work_order"),
            client.list("team_member"),
        )?;

        let team_name = teams_v
            .as_array()
            .and_then(|arr| arr.iter().find(|e| env_id(e) == team_id))
            .map(|e| payload_str(e, "name"))
            .unwrap_or_default();

        let mut points_in_flight: i64 = 0;
        let mut open_count: u64 = 0;
        for wo in work_orders_v.as_array().cloned().unwrap_or_default() {
            if payload_str(&wo, "team_id") != team_id {
                continue;
            }
            if payload_str(&wo, "status") == "done" {
                continue;
            }
            open_count += 1;
            points_in_flight += payload_i64(&wo, "story_points");
        }

        let member_count = members_v
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter(|m| payload_str(m, "team_id") == team_id)
                    .count() as u64
            })
            .unwrap_or(0);

        Ok(json!({
            "team_id": team_id,
            "team_name": team_name,
            "points_in_flight": points_in_flight,
            "open_work_order_count": open_count,
            "member_count": member_count,
        }))
    })
}

fn members_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let team_id = args
            .get("team_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'team_id' argument"))?
            .to_string();

        let (members_v, persons_v) =
            tokio::try_join!(client.list("team_member"), client.list("person"))?;

        let person_ids: Vec<String> = members_v
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| payload_str(m, "team_id") == team_id)
            .map(|m| payload_str(&m, "person_id"))
            .filter(|s| !s.is_empty())
            .collect();

        let persons: Vec<Value> = persons_v
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| person_ids.iter().any(|id| id == &env_id(p)))
            .collect();

        Ok(Value::Array(persons))
    })
}

fn assignments_handler(client: Arc<MeshqlClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let person_id = args
            .get("person_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'person_id' argument"))?
            .to_string();

        let (members_v, work_orders_v) =
            tokio::try_join!(client.list("team_member"), client.list("work_order"))?;

        let team_ids: Vec<String> = members_v
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| payload_str(m, "person_id") == person_id)
            .map(|m| payload_str(&m, "team_id"))
            .filter(|s| !s.is_empty())
            .collect();

        let work_orders: Vec<Value> = work_orders_v
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|wo| {
                let tid = payload_str(wo, "team_id");
                let status = payload_str(wo, "status");
                team_ids.iter().any(|t| t == &tid) && status != "done"
            })
            .collect();

        Ok(Value::Array(work_orders))
    })
}

// ── registry ─────────────────────────────────────────────────────────────────

/// The Union-specific tools to register alongside meshql-mcp's `catalog.*`
/// tools. The client argument lets handlers capture it into closures.
pub fn custom_tools(_client: Arc<MeshqlClient>) -> Vec<Tool> {
    vec![
        Tool {
            name: "team.capacity",
            description: "How loaded is this team? Sums story points across the team's open \
                          (status != \"done\") work orders, alongside open work-order count and \
                          member count. Use this to gauge whether a team has bandwidth for new work.",
            input_schema: json!({
                "type": "object",
                "required": ["team_id"],
                "properties": {
                    "team_id": { "type": "string", "description": "Union Team id." }
                }
            }),
            handler: Arc::new(capacity_handler),
        },
        Tool {
            name: "team.members",
            description: "Who is on this team? Resolves the Person records that belong to the team \
                          via TeamMember rows. Returns an array of Person envelopes (id + payload).",
            input_schema: json!({
                "type": "object",
                "required": ["team_id"],
                "properties": {
                    "team_id": { "type": "string", "description": "Union Team id." }
                }
            }),
            handler: Arc::new(members_handler),
        },
        Tool {
            name: "person.assignments",
            description: "What's on this person's plate? Finds the teams the person belongs to via \
                          TeamMember rows, then returns the open (status != \"done\") work orders \
                          across all of those teams. Returns an array of WorkOrder envelopes.",
            input_schema: json!({
                "type": "object",
                "required": ["person_id"],
                "properties": {
                    "person_id": { "type": "string", "description": "Union Person id." }
                }
            }),
            handler: Arc::new(assignments_handler),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(id: &str, payload: Value) -> Value {
        json!({ "id": id, "payload": payload })
    }

    #[test]
    fn payload_i64_treats_missing_as_zero() {
        let e = env("wo-1", json!({ "team_id": "t" }));
        assert_eq!(payload_i64(&e, "story_points"), 0);
    }

    #[test]
    fn payload_i64_treats_null_as_zero() {
        let e = env("wo-1", json!({ "story_points": Value::Null }));
        assert_eq!(payload_i64(&e, "story_points"), 0);
    }

    #[test]
    fn payload_i64_reads_nested() {
        let e = env("wo-1", json!({ "story_points": 5 }));
        assert_eq!(payload_i64(&e, "story_points"), 5);
    }

    #[test]
    fn payload_str_reads_flat_fallback() {
        let e = json!({ "id": "p-1", "name": "Ada" });
        assert_eq!(payload_str(&e, "name"), "Ada");
    }
}
