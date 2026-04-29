//! Required-field + JSON-Schema-enum validators for Yard entities.
//!
//! Mirrors the cityhall/union pattern: a generic base validator handles
//! required + enum, and per-entity validators layer additional rules.

use meshql_core::Stash;
use meshql_server::{ValidatorContext, ValidatorFn};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Generic required-field + JSON-Schema-enum validator.
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

/// TestEnvironment custom validator: enforces backing-resource expectations
/// per kind on top of base required + enum checks:
///
/// - `external` — `contractual_limit` MUST be present (someone has to know
///   how many envs the SaaS contract permits).
/// - `mock` / `stub` — `mock_source_id` MUST be present (where does the mock
///   live?).
pub fn test_environment_validator(schema: &serde_json::Value) -> ValidatorFn {
    let base = base_schema_validator(schema);
    Arc::new(move |payload: &Stash, ctx: &ValidatorContext| {
        base(payload, ctx)?;
        let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "external" => require_field(payload, "contractual_limit", "TestEnvironment kind 'external'"),
            "mock" | "stub" => require_field(payload, "mock_source_id", "TestEnvironment kind 'mock' or 'stub'"),
            _ => Ok(()),
        }
    })
}

/// DataSync custom validator: source must come from somewhere (env or data
/// source), and `shared` syncs must reference an env, never a static data
/// source — sharing a fixture file is meaningless.
pub fn data_sync_validator(schema: &serde_json::Value) -> ValidatorFn {
    let base = base_schema_validator(schema);
    Arc::new(move |payload: &Stash, ctx: &ValidatorContext| {
        base(payload, ctx)?;
        let env_src = nonempty_string(payload, "source_env_id");
        let data_src = nonempty_string(payload, "source_data_id");
        if !env_src && !data_src {
            return Err(
                "DataSync requires either source_env_id or source_data_id".to_string(),
            );
        }
        let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "shared" && !env_src {
            return Err(
                "DataSync kind 'shared' requires source_env_id (cannot share a static data source)"
                    .to_string(),
            );
        }
        Ok(())
    })
}

fn require_field(payload: &Stash, field: &str, ctx: &str) -> Result<(), String> {
    if nonempty_string(payload, field) {
        Ok(())
    } else {
        Err(format!("{ctx} requires field '{field}'"))
    }
}

fn nonempty_string(payload: &Stash, field: &str) -> bool {
    payload
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}
