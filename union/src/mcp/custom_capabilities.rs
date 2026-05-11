//! Union-specific custom capabilities — the cross-entity rollups that don't
//! fit a templated dispatcher:
//!
//! - `team_capacity` — sums open story points for a team
//! - `members_of_team` — resolves Person rows for a team via TeamMember
//! - `assignments_for_person` — open WorkOrders across a person's teams
//!
//! All three internally read via REST (`MeshqlClient::list`) — these are
//! aggregating handlers that need full entity lists and would gain little
//! from per-call GraphQL field-selection trimming. The MCP server's
//! schema-driven catalog reads use GraphQL; these custom rollups use REST.

use meshql_mcp::{Capability, CapabilityHandler, MeshqlClient, ToolFuture, ToolHandler};
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

// ── capabilities ─────────────────────────────────────────────────────────────

/// `team_capacity` — sums open story points for a team alongside open
/// work-order count and member count.
pub fn team_capacity(_client: Arc<MeshqlClient>) -> Capability {
    let handler: ToolHandler = Arc::new(move |client, args| -> ToolFuture {
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
    });

    Capability {
        name: "team_capacity",
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
        handler: CapabilityHandler::Custom(handler),
    }
}

/// `members_of_team` — resolves the Person records that belong to the team
/// via TeamMember rows. Returns an array of Person envelopes.
pub fn members_of_team(_client: Arc<MeshqlClient>) -> Capability {
    let handler: ToolHandler = Arc::new(move |client, args| -> ToolFuture {
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
    });

    Capability {
        name: "members_of_team",
        description: "Who is on this team? Resolves the Person records that belong to the team \
                      via TeamMember rows. Returns an array of Person envelopes (id + payload).",
        input_schema: json!({
            "type": "object",
            "required": ["team_id"],
            "properties": {
                "team_id": { "type": "string", "description": "Union Team id." }
            }
        }),
        handler: CapabilityHandler::Custom(handler),
    }
}

/// `assignments_for_person` — open WorkOrders across the teams a person
/// belongs to.
pub fn assignments_for_person(_client: Arc<MeshqlClient>) -> Capability {
    let handler: ToolHandler = Arc::new(move |client, args| -> ToolFuture {
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
    });

    Capability {
        name: "assignments_for_person",
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
        handler: CapabilityHandler::Custom(handler),
    }
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
