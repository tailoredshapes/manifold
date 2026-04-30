//! `catalog.list`, `catalog.get`, `catalog.search` — thin wrappers over the
//! Groundwork REST API so an LLM can interrogate the catalogue without
//! constructing HTTP calls itself.

use super::{Tool, ToolFuture};
use crate::mcp::client::GroundworkClient;
use serde_json::{json, Value};
use std::sync::Arc;

const ENTITIES: &[&str] = &[
    "deployable",
    "service",
    "exposes",
    "dependency",
    "contract",
    "sla",
];

fn entity_arg(args: &Value) -> anyhow::Result<String> {
    let entity = args
        .get("entity")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'entity' argument"))?;
    if !ENTITIES.contains(&entity) {
        anyhow::bail!(
            "entity must be one of {:?}, got {:?}",
            ENTITIES,
            entity
        );
    }
    Ok(entity.to_string())
}

fn list_handler(client: Arc<GroundworkClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let entity = entity_arg(&args)?;
        client.list(&entity).await
    })
}

fn get_handler(client: Arc<GroundworkClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let entity = entity_arg(&args)?;
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'id' argument"))?
            .to_string();
        match client.get(&entity, &id).await? {
            Some(v) => Ok(v),
            None => Ok(Value::Null),
        }
    })
}

fn search_handler(client: Arc<GroundworkClient>, args: Value) -> ToolFuture {
    Box::pin(async move {
        let entity = entity_arg(&args)?;
        let needle = args
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let all = client.list(&entity).await?;
        let arr = all.as_array().cloned().unwrap_or_default();
        if let Some(needle) = needle {
            let lower = needle.to_lowercase();
            let filtered: Vec<Value> = arr
                .into_iter()
                .filter(|env| {
                    let n = name_of(env);
                    n.to_lowercase().contains(&lower)
                })
                .collect();
            return Ok(Value::Array(filtered));
        }
        Ok(Value::Array(arr))
    })
}

fn name_of(env: &Value) -> String {
    if let Some(p) = env.get("payload").and_then(|p| p.as_object()) {
        if let Some(s) = p.get("name").and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    env.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
}

pub fn tools() -> Vec<Tool> {
    let entity_enum: Vec<Value> = ENTITIES.iter().map(|e| json!(e)).collect();

    vec![
        Tool {
            name: "catalog.list",
            description: "List every record of a Groundwork entity type \
                          (deployable, service, exposes, dependency, contract, sla). \
                          Use this for a high-level inventory.",
            input_schema: json!({
                "type": "object",
                "required": ["entity"],
                "properties": {
                    "entity": {
                        "type": "string",
                        "enum": entity_enum,
                        "description": "Which entity to list."
                    }
                }
            }),
            handler: Arc::new(list_handler),
        },
        Tool {
            name: "catalog.get",
            description: "Fetch a single Groundwork record by id. Returns null if not found.",
            input_schema: json!({
                "type": "object",
                "required": ["entity", "id"],
                "properties": {
                    "entity": { "type": "string", "enum": ENTITIES },
                    "id":     { "type": "string" }
                }
            }),
            handler: Arc::new(get_handler),
        },
        Tool {
            name: "catalog.search",
            description: "Find Groundwork records whose name contains a substring (case-insensitive). \
                          Returns an array of envelopes.",
            input_schema: json!({
                "type": "object",
                "required": ["entity"],
                "properties": {
                    "entity": { "type": "string", "enum": ENTITIES },
                    "name":   { "type": "string", "description": "Substring to match against the record's name." }
                }
            }),
            handler: Arc::new(search_handler),
        },
    ]
}
