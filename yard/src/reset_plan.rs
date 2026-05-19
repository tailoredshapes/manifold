//! Reset planner — given a target test-environment, compute the structured
//! procedure to refresh it.
//!
//! The shape mirrors cityhall's `ComputedPlan` so the two surfaces feel
//! symmetric: an ordered list of `steps`, an unordered list of `blockers`,
//! and a summary block. The frontend renders this directly.
//!
//! Inputs (read-only) — `test_environment`, `data_sync`, `test_run`,
//! `test_infrastructure`, and optionally `sync_run` repositories. The
//! planner doesn't mutate anything; the actual execution of the sync(s)
//! is out of scope for v0.1 (you'd execute and then `POST /sync_run/api`
//! a record of what happened).

use chrono::{DateTime, Utc};
use meshql_core::Repository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetStep {
    pub order: usize,
    pub data_sync_id: String,
    pub source_label: String,
    pub target_env_id: String,
    pub target_env_name: String,
    pub kind: String,
    pub refresh_policy: Option<String>,
    pub estimated_minutes: f64,
    pub estimated_cost: f64,
    pub masking_summary: Option<String>,
    /// Step orders that must complete before this one. Set when the source
    /// env itself has a feeding sync that needs to run first to refresh
    /// what we're about to pull from.
    pub predecessor_orders: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetBlocker {
    /// `in_flight_test_run` | `downstream_dependents` | `no_sync_registered`
    pub kind: String,
    pub message: String,
    /// IDs of the offending records (test_run ids, downstream env ids, …).
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetPlan {
    pub target_env_id: String,
    pub target_env_name: String,
    pub computed_at: String,
    /// Last successful `SyncRun` against this env — informs "data age" /
    /// "refresh overdue" computations in the UI.
    pub last_sync_at: Option<String>,
    pub estimated_total_minutes: f64,
    pub estimated_total_cost: f64,
    pub steps: Vec<ResetStep>,
    pub blockers: Vec<ResetBlocker>,
}

pub struct PlanInputs<'a> {
    pub target_env_id: &'a str,
    pub test_environment_repo: &'a Arc<dyn Repository>,
    pub test_infrastructure_repo: &'a Arc<dyn Repository>,
    pub data_sync_repo: &'a Arc<dyn Repository>,
    pub data_source_repo: &'a Arc<dyn Repository>,
    pub test_run_repo: &'a Arc<dyn Repository>,
    pub sync_run_repo: &'a Arc<dyn Repository>,
}

pub async fn compute(inputs: PlanInputs<'_>) -> anyhow::Result<ResetPlan> {
    let envs = load_envs(inputs.test_environment_repo).await?;
    let infras = load_infras(inputs.test_infrastructure_repo).await?;
    let syncs = load_syncs(inputs.data_sync_repo).await?;
    let sources = load_sources(inputs.data_source_repo).await?;

    let target_env = envs.get(inputs.target_env_id).ok_or_else(|| {
        anyhow::anyhow!("test environment {} not found", inputs.target_env_id)
    })?;

    // ── Steps: every sync whose target_env_id is us ──────────────────────
    let direct_syncs: Vec<&DataSync> = syncs
        .iter()
        .filter(|s| s.target_env_id == inputs.target_env_id)
        .collect();

    let mut steps: Vec<ResetStep> = Vec::new();
    let mut step_index_by_sync: HashMap<String, usize> = HashMap::new();
    let mut order: usize = 0;

    // For each direct sync, recursively include any sync that feeds OUR
    // source env (one hop — deep cascades are out of scope for v0.1).
    for sync in &direct_syncs {
        if let Some(src_env_id) = sync.source_env_id.as_deref() {
            let predecessor = syncs.iter().find(|s| s.target_env_id == src_env_id);
            if let Some(pre) = predecessor {
                if !step_index_by_sync.contains_key(&pre.id) {
                    order += 1;
                    let step = build_step(order, pre, &envs, &infras, &sources, &[]);
                    step_index_by_sync.insert(pre.id.clone(), order);
                    steps.push(step);
                }
            }
        }
    }
    for sync in &direct_syncs {
        if step_index_by_sync.contains_key(&sync.id) {
            continue;
        }
        let preds: Vec<usize> = sync
            .source_env_id
            .as_deref()
            .and_then(|src_id| syncs.iter().find(|s| s.target_env_id == src_id))
            .and_then(|pre| step_index_by_sync.get(&pre.id).copied())
            .map(|o| vec![o])
            .unwrap_or_default();
        order += 1;
        let step = build_step(order, sync, &envs, &infras, &sources, &preds);
        step_index_by_sync.insert(sync.id.clone(), order);
        steps.push(step);
    }

    // ── Blockers ────────────────────────────────────────────────────────
    let mut blockers: Vec<ResetBlocker> = Vec::new();

    if steps.is_empty() {
        blockers.push(ResetBlocker {
            kind: "no_sync_registered".into(),
            message: format!(
                "No data_sync rows have target_env_id = {}. Register at least one sync before this env can be reset.",
                inputs.target_env_id
            ),
            references: vec![],
        });
    }

    // In-flight test runs on the target env — reset would interrupt them.
    let in_flight = load_in_flight_runs(inputs.test_run_repo, inputs.target_env_id).await?;
    if !in_flight.is_empty() {
        blockers.push(ResetBlocker {
            kind: "in_flight_test_run".into(),
            message: format!(
                "{} test run{} in flight on this env. Drain or cancel before resetting.",
                in_flight.len(),
                if in_flight.len() == 1 { "" } else { "s" }
            ),
            references: in_flight,
        });
    }

    // Downstream envs — those that have a sync pulling FROM us. Resetting
    // us will stale their data until they re-pull.
    let downstream: Vec<String> = syncs
        .iter()
        .filter(|s| s.source_env_id.as_deref() == Some(inputs.target_env_id))
        .map(|s| s.target_env_id.clone())
        .collect();
    let unique_downstream: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        downstream.into_iter().filter(|x| seen.insert(x.clone())).collect()
    };
    if !unique_downstream.is_empty() {
        let downstream_names: Vec<String> = unique_downstream
            .iter()
            .map(|id| envs.get(id).map(|e| e.name.clone()).unwrap_or_else(|| id.clone()))
            .collect();
        blockers.push(ResetBlocker {
            kind: "downstream_dependents".into(),
            message: format!(
                "{} downstream env{} pull from this one and will go stale: {}",
                unique_downstream.len(),
                if unique_downstream.len() == 1 { "" } else { "s" },
                downstream_names.join(", ")
            ),
            references: unique_downstream,
        });
    }

    let estimated_total_minutes: f64 = steps.iter().map(|s| s.estimated_minutes).sum();
    let estimated_total_cost: f64 = steps.iter().map(|s| s.estimated_cost).sum();

    let last_sync_at = load_last_sync_at(inputs.sync_run_repo, inputs.target_env_id).await?;

    Ok(ResetPlan {
        target_env_id: inputs.target_env_id.into(),
        target_env_name: target_env.name.clone(),
        computed_at: Utc::now().to_rfc3339(),
        last_sync_at,
        estimated_total_minutes,
        estimated_total_cost,
        steps,
        blockers,
    })
}

fn build_step(
    order: usize,
    sync: &DataSync,
    envs: &HashMap<String, EnvLite>,
    infras: &HashMap<String, InfraLite>,
    sources: &HashMap<String, SourceLite>,
    preds: &[usize],
) -> ResetStep {
    let target_env = envs.get(&sync.target_env_id);
    let target_env_name = target_env
        .map(|e| e.name.clone())
        .unwrap_or_else(|| sync.target_env_id.clone());

    // Source label: prefer source env name, then source data name, then id.
    let source_label = if let Some(src_env_id) = sync.source_env_id.as_deref() {
        envs.get(src_env_id)
            .map(|e| format!("env {}", e.name))
            .unwrap_or_else(|| format!("env {}", src_env_id))
    } else if let Some(src_data_id) = sync.source_data_id.as_deref() {
        sources
            .get(src_data_id)
            .map(|d| format!("source {}", d.name))
            .unwrap_or_else(|| format!("source {}", src_data_id))
    } else {
        "(no source)".into()
    };

    let estimated_minutes = sync.estimated_minutes.unwrap_or(0.0);

    // Cost = (sync minutes) × (target env's infrastructure cost_per_hour) / 60.
    let target_cost_per_hour = target_env
        .and_then(|e| e.infrastructure_id.as_deref())
        .and_then(|inf_id| infras.get(inf_id))
        .and_then(|inf| inf.cost_per_hour)
        .or_else(|| target_env.and_then(|e| e.cost_per_hour))
        .unwrap_or(0.0);
    let estimated_cost = (estimated_minutes / 60.0) * target_cost_per_hour;

    ResetStep {
        order,
        data_sync_id: sync.id.clone(),
        source_label,
        target_env_id: sync.target_env_id.clone(),
        target_env_name,
        kind: sync.kind.clone(),
        refresh_policy: sync.refresh_policy.clone(),
        estimated_minutes,
        estimated_cost,
        masking_summary: sync.masking_summary.clone(),
        predecessor_orders: preds.to_vec(),
    }
}

// ── Lite repository projections ─────────────────────────────────────────────

#[derive(Debug, Clone)]
struct EnvLite {
    name: String,
    cost_per_hour: Option<f64>,
    infrastructure_id: Option<String>,
}

#[derive(Debug, Clone)]
struct InfraLite {
    cost_per_hour: Option<f64>,
}

#[derive(Debug, Clone)]
struct SourceLite {
    name: String,
}

#[derive(Debug, Clone)]
struct DataSync {
    id: String,
    kind: String,
    target_env_id: String,
    source_env_id: Option<String>,
    source_data_id: Option<String>,
    refresh_policy: Option<String>,
    estimated_minutes: Option<f64>,
    masking_summary: Option<String>,
}

fn parse_f64(v: &Value) -> Option<f64> {
    v.as_str()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .or_else(|| v.as_f64())
}

async fn load_envs(
    repo: &Arc<dyn Repository>,
) -> anyhow::Result<HashMap<String, EnvLite>> {
    let envelopes = repo.list(&[]).await?;
    let mut out = HashMap::new();
    for env in envelopes {
        let p = &env.payload;
        let name = p
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cost_per_hour = p.get("cost_per_hour").and_then(parse_f64);
        let infrastructure_id = p
            .get("infrastructure_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        out.insert(env.id.clone(), EnvLite { name, cost_per_hour, infrastructure_id });
    }
    Ok(out)
}

async fn load_infras(
    repo: &Arc<dyn Repository>,
) -> anyhow::Result<HashMap<String, InfraLite>> {
    let envelopes = repo.list(&[]).await?;
    let mut out = HashMap::new();
    for env in envelopes {
        let cost_per_hour = env.payload.get("cost_per_hour").and_then(parse_f64);
        out.insert(env.id.clone(), InfraLite { cost_per_hour });
    }
    Ok(out)
}

async fn load_syncs(repo: &Arc<dyn Repository>) -> anyhow::Result<Vec<DataSync>> {
    let envelopes = repo.list(&[]).await?;
    let mut out = Vec::with_capacity(envelopes.len());
    for env in envelopes {
        let p = &env.payload;
        let kind = p
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let target_env_id = p
            .get("target_env_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if target_env_id.is_empty() {
            continue;
        }
        out.push(DataSync {
            id: env.id.clone(),
            kind,
            target_env_id,
            source_env_id: p.get("source_env_id").and_then(|v| v.as_str()).map(String::from),
            source_data_id: p.get("source_data_id").and_then(|v| v.as_str()).map(String::from),
            refresh_policy: p.get("refresh_policy").and_then(|v| v.as_str()).map(String::from),
            estimated_minutes: p.get("estimated_minutes").and_then(parse_f64),
            masking_summary: p.get("masking_summary").and_then(|v| v.as_str()).map(String::from),
        });
    }
    Ok(out)
}

async fn load_sources(
    repo: &Arc<dyn Repository>,
) -> anyhow::Result<HashMap<String, SourceLite>> {
    let envelopes = repo.list(&[]).await?;
    let mut out = HashMap::new();
    for env in envelopes {
        let name = env
            .payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.insert(env.id.clone(), SourceLite { name });
    }
    Ok(out)
}

async fn load_in_flight_runs(
    test_run_repo: &Arc<dyn Repository>,
    env_id: &str,
) -> anyhow::Result<Vec<String>> {
    let envelopes = test_run_repo.list(&[]).await?;
    let mut out = Vec::new();
    for env in envelopes {
        let p = &env.payload;
        if p.get("test_environment_id").and_then(|v| v.as_str()) != Some(env_id) {
            continue;
        }
        let status = p.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status == "pending" || status == "running" {
            out.push(env.id.clone());
        }
    }
    Ok(out)
}

async fn load_last_sync_at(
    sync_run_repo: &Arc<dyn Repository>,
    env_id: &str,
) -> anyhow::Result<Option<String>> {
    let envelopes = sync_run_repo.list(&[]).await?;
    let mut latest: Option<DateTime<Utc>> = None;
    let mut latest_raw: Option<String> = None;
    for env in envelopes {
        let p = &env.payload;
        if p.get("target_env_id").and_then(|v| v.as_str()) != Some(env_id) {
            continue;
        }
        if p.get("status").and_then(|v| v.as_str()) != Some("succeeded") {
            continue;
        }
        let Some(finished) = p.get("finished_at").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(ts) = DateTime::parse_from_rfc3339(finished) else { continue };
        let ts_utc = ts.with_timezone(&Utc);
        if latest.is_none_or(|cur| ts_utc > cur) {
            latest = Some(ts_utc);
            latest_raw = Some(finished.to_string());
        }
    }
    Ok(latest_raw)
}
