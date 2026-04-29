//! Bylaw + OrgNode validation and the layering resolver.

use anyhow::Context;
use meshql_core::{Repository, Stash};
use meshql_server::{ValidatorContext, ValidatorFn};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Custom validator for OrgNode. Beyond required-field + enum checks:
///
/// - if `kind == "enterprise"`, `parent_id` MUST be absent;
/// - if `kind != "enterprise"`, `parent_id` MUST be present and non-empty.
pub fn org_node_validator(schema: &serde_json::Value) -> ValidatorFn {
    let base = base_schema_validator(schema);
    Arc::new(move |payload: &Stash, ctx: &ValidatorContext| {
        base(payload, ctx)?;
        let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let parent_id_present = payload
            .get("parent_id")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        match (kind, parent_id_present) {
            ("enterprise", true) => Err(
                "Enterprise OrgNode must not have a parent_id".to_string(),
            ),
            ("enterprise", false) => Ok(()),
            (_, false) => Err(format!("OrgNode kind '{kind}' must have a parent_id")),
            _ => Ok(()),
        }
    })
}

/// Custom validator for Bylaw. Required-field + enum, plus per-gate-type
/// requirements:
///
/// - WindowGate / FreezePeriod require `window`.
/// - QuiesceGate requires `quiesce_for`.
/// - ApprovalGate requires `approvers`.
/// - AutoGate has no extra requirements.
pub fn bylaw_validator(schema: &serde_json::Value) -> ValidatorFn {
    let base = base_schema_validator(schema);
    Arc::new(move |payload: &Stash, ctx: &ValidatorContext| {
        base(payload, ctx)?;
        let gate_type = payload.get("gate_type").and_then(|v| v.as_str()).unwrap_or("");
        let needs: &[&str] = match gate_type {
            "WindowGate" | "FreezePeriod" => &["window"],
            "QuiesceGate" => &["quiesce_for"],
            "ApprovalGate" => &["approvers"],
            _ => &[],
        };
        for field in needs {
            let present = payload
                .get(*field)
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !present {
                return Err(format!(
                    "Bylaw gate_type '{gate_type}' requires field '{field}'"
                ));
            }
        }
        Ok(())
    })
}

/// Generic required-field + JSON-Schema-enum validator. Reused as the base for
/// the custom OrgNode and Bylaw validators, and used directly for entities that
/// don't need extra rules.
pub fn base_schema_validator(schema: &serde_json::Value) -> ValidatorFn {
    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_default();

    let enums: BTreeMap<String, Vec<String>> = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| {
            props
                .iter()
                .filter_map(|(k, v)| {
                    v.get("enum").and_then(|e| e.as_array()).map(|arr| {
                        (
                            k.clone(),
                            arr.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Arc::new(move |payload: &Stash, _ctx: &ValidatorContext| {
        for field in &required {
            match payload.get(field.as_str()) {
                None => return Err(format!("Required field '{}' is missing", field)),
                Some(v) if v.as_str().map(|s| s.trim().is_empty()).unwrap_or(false) => {
                    return Err(format!("Required field '{}' cannot be empty", field));
                }
                _ => {}
            }
        }
        for (field, allowed) in &enums {
            if let Some(v) = payload.get(field.as_str()).and_then(|x| x.as_str()) {
                if !allowed.iter().any(|a| a == v) {
                    return Err(format!(
                        "Field '{}' must be one of {:?}, got {:?}",
                        field, allowed, v
                    ));
                }
            }
        }
        Ok(())
    })
}

// ── Layering resolver ───────────────────────────────────────────────────────

/// One resolved bylaw, with its source OrgNode along the ancestor chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveBylaw {
    pub bylaw_id: String,
    pub org_node_id: String,
    pub org_node_name: String,
    pub gate_type: String,
    pub priority: Option<String>,
    pub description: Option<String>,
    pub conditions: Option<String>,
    pub window: Option<String>,
    pub quiesce_for: Option<String>,
    pub approvers: Option<String>,
}

/// Walk the ancestor chain of `start_id` (inclusive) and return root-first.
/// Each element is (id, name) for that ancestor.
pub async fn ancestors_of(
    org_node_repo: &Arc<dyn Repository>,
    start_id: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let mut chain: Vec<(String, String)> = Vec::new();
    let mut cursor = start_id.to_string();

    loop {
        let env = org_node_repo
            .read(&cursor, &[], None)
            .await
            .with_context(|| format!("OrgNode read for id={cursor}"))?;
        let Some(env) = env else { break };
        let name = env
            .payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        chain.push((env.id.clone(), name));
        let parent_id = env
            .payload
            .get("parent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if parent_id.is_empty() {
            break;
        }
        cursor = parent_id;
    }

    chain.reverse(); // root-first
    Ok(chain)
}

/// Collect every bylaw attached to any OrgNode along the ancestor chain of
/// `org_node_id`. Returns root-first; within an OrgNode, bylaws are emitted in
/// the order the repository yields them.
pub async fn effective_bylaws_for(
    org_node_repo: &Arc<dyn Repository>,
    bylaw_repo: &Arc<dyn Repository>,
    org_node_id: &str,
) -> anyhow::Result<Vec<EffectiveBylaw>> {
    let chain = ancestors_of(org_node_repo, org_node_id).await?;
    let all_bylaws = bylaw_repo
        .list(&[])
        .await
        .context("listing bylaws")?;
    let mut out: Vec<EffectiveBylaw> = Vec::new();

    for (node_id, node_name) in chain {
        for env in &all_bylaws {
            let on = env
                .payload
                .get("org_node_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if on != node_id {
                continue;
            }
            out.push(EffectiveBylaw {
                bylaw_id: env.id.clone(),
                org_node_id: node_id.clone(),
                org_node_name: node_name.clone(),
                gate_type: env
                    .payload
                    .get("gate_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                priority: env.payload.get("priority").and_then(|v| v.as_str()).map(String::from),
                description: env.payload.get("description").and_then(|v| v.as_str()).map(String::from),
                conditions: env.payload.get("conditions").and_then(|v| v.as_str()).map(String::from),
                window: env.payload.get("window").and_then(|v| v.as_str()).map(String::from),
                quiesce_for: env.payload.get("quiesce_for").and_then(|v| v.as_str()).map(String::from),
                approvers: env.payload.get("approvers").and_then(|v| v.as_str()).map(String::from),
            });
        }
    }

    Ok(out)
}
