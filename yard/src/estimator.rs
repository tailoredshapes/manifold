//! ChangeRequest → infrastructure / data / coordination tasks for the Cityhall
//! Gantt.
//!
//! The estimator is generic over `GroundworkLookup` and `ChangeRequestLookup`
//! so the BDD harness can stub both. The HTTP-backed implementations live in
//! `groundwork_client.rs` and `cityhall_client.rs`.

use crate::sync::{recommend_sync, DependencyEdge, RecommendedSync};
use anyhow::Context;
use async_trait::async_trait;
use meshql_core::Repository;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::Arc;

/// What the estimator needs to know about a Groundwork deployable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployableSummary {
    pub id: String,
    pub name: String,
    pub team_id: Option<String>,
    pub depends_on: Vec<String>,
    /// Services this deployable consumes (used to characterise the dep edge).
    pub depends_on_services: Vec<String>,
}

#[async_trait]
pub trait GroundworkLookup: Send + Sync {
    async fn get_deployable(&self, id: &str) -> anyhow::Result<Option<DeployableSummary>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRequestSummary {
    pub id: String,
    pub summary: String,
    pub tier: Option<String>,
    pub target_deployables: Vec<String>,
}

#[async_trait]
pub trait ChangeRequestLookup: Send + Sync {
    async fn get_change_request(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<ChangeRequestSummary>>;
}

// ── Output shape ─────────────────────────────────────────────────────────────

/// One scheduled item in the Cityhall Gantt: an infrastructure spin-up,
/// a data sync, or a coordination wait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimateTask {
    pub kind: String, // "infrastructure" | "data" | "coordination" | "test"
    pub label: String,
    pub deployable_id: Option<String>,
    pub test_environment_id: Option<String>,
    pub estimated_minutes: u32,
    pub estimated_cost: f64,
    pub predecessors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedEstimate {
    pub change_request_id: String,
    pub change_request_summary: String,
    pub tier: String,
    pub tasks: Vec<EstimateTask>,
    pub blockers: Vec<String>,
    pub total_minutes: u32,
    pub total_cost: f64,
    pub computed_at: String,
}

/// Inputs to the estimator.
pub struct EstimateInputs<'a> {
    pub change_request_id: String,
    pub change_request_summary: String,
    pub tier: String,
    pub target_deployable_ids: Vec<String>,
    pub test_environment_repo: &'a Arc<dyn Repository>,
    pub test_infrastructure_repo: &'a Arc<dyn Repository>,
    pub data_sync_repo: &'a Arc<dyn Repository>,
    pub groundwork: &'a dyn GroundworkLookup,
}

/// Compute an estimate. Always returns a `ComputedEstimate`; missing envs
/// are surfaced as blockers, never panics.
pub async fn compute_estimate(inputs: EstimateInputs<'_>) -> anyhow::Result<ComputedEstimate> {
    // ── 1. Walk Groundwork deps from the targets, depth-first.
    let mut summaries: BTreeMap<String, DeployableSummary> = BTreeMap::new();
    let mut blockers: Vec<String> = Vec::new();
    let mut queue: VecDeque<String> = inputs.target_deployable_ids.iter().cloned().collect();
    let mut seen: HashSet<String> = HashSet::new();

    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let summary = inputs
            .groundwork
            .get_deployable(&id)
            .await
            .with_context(|| format!("groundwork.get_deployable({id})"))?;
        let Some(summary) = summary else {
            blockers.push(format!("unknown deployable: {id}"));
            continue;
        };
        for dep in &summary.depends_on {
            queue.push_back(dep.clone());
        }
        summaries.insert(id, summary);
    }

    // ── 2. Map each deployable to a TestEnvironment (or flag a blocker).
    //       Pick the first TestEnvironment whose payload.deployable_id matches.
    let test_envs = inputs
        .test_environment_repo
        .list(&[])
        .await
        .context("listing test_environments")?;
    let infrastructures = inputs
        .test_infrastructure_repo
        .list(&[])
        .await
        .context("listing test_infrastructures")?;

    let infra_cost: BTreeMap<String, f64> = infrastructures
        .iter()
        .map(|env| {
            let cost = env
                .payload
                .get("cost_per_hour")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            (env.id.clone(), cost)
        })
        .collect();

    let mut env_for_dep: BTreeMap<String, EnvForDep> = BTreeMap::new();
    for env in &test_envs {
        let dep_id = env.payload.get("deployable_id").and_then(|v| v.as_str()).unwrap_or("");
        if dep_id.is_empty() {
            continue;
        }
        if env_for_dep.contains_key(dep_id) {
            continue; // first match wins
        }
        let env_cost_per_hour = env
            .payload
            .get("cost_per_hour")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let infra_id = env
            .payload
            .get("infrastructure_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let infra_cost_per_hour = infra_id
            .as_ref()
            .and_then(|id| infra_cost.get(id).copied())
            .unwrap_or(0.0);
        let spinup = env
            .payload
            .get("spinup_minutes")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(15);
        let kind = env
            .payload
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = env
            .payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        env_for_dep.insert(
            dep_id.to_string(),
            EnvForDep {
                env_id: env.id.clone(),
                env_name: name,
                env_kind: kind,
                spinup_minutes: spinup,
                cost_per_hour: env_cost_per_hour + infra_cost_per_hour,
            },
        );
    }

    for dep_id in summaries.keys() {
        if !env_for_dep.contains_key(dep_id) {
            blockers.push(format!("no test_environment for deployable: {dep_id}"));
        }
    }

    // ── 3. Topo-sort by depends_on; predecessors first.
    let order = topo_sort(&summaries, &mut blockers);

    // ── 4. Emit infrastructure tasks per deployable.
    let mut tasks: Vec<EstimateTask> = Vec::new();
    let mut infra_task_id_for_dep: BTreeMap<String, String> = BTreeMap::new();
    for dep_id in &order {
        let Some(env) = env_for_dep.get(dep_id) else { continue };
        let summary = &summaries[dep_id];
        let predecessor_envs: Vec<String> = summary
            .depends_on
            .iter()
            .filter_map(|d| infra_task_id_for_dep.get(d).cloned())
            .collect();
        let task_id = format!("infra-{}", env.env_id);
        tasks.push(EstimateTask {
            kind: "infrastructure".into(),
            label: format!("spin up {} ({} env)", env.env_name, env.env_kind),
            deployable_id: Some(dep_id.clone()),
            test_environment_id: Some(env.env_id.clone()),
            estimated_minutes: env.spinup_minutes,
            estimated_cost: env.cost_per_hour * (env.spinup_minutes as f64 / 60.0),
            predecessors: predecessor_envs,
        });
        infra_task_id_for_dep.insert(dep_id.clone(), task_id);
    }

    // ── 5. Emit data-sync tasks per Groundwork dep edge.
    let mut sync_total_minutes: BTreeMap<String, u32> = BTreeMap::new();
    let existing_syncs = inputs
        .data_sync_repo
        .list(&[])
        .await
        .context("listing data_syncs")?;
    for dep_id in &order {
        let Some(target_env) = env_for_dep.get(dep_id) else { continue };
        let summary = &summaries[dep_id];
        for upstream_id in &summary.depends_on {
            let Some(source_env) = env_for_dep.get(upstream_id) else { continue };

            let edge = classify_dep_edge(summary, upstream_id);
            let recommended = recommend_sync(edge);
            let estimated_minutes =
                find_existing_sync_minutes(&existing_syncs, &source_env.env_id, &target_env.env_id)
                    .unwrap_or_else(|| default_sync_minutes(&recommended));

            tasks.push(EstimateTask {
                kind: "data".into(),
                label: format!(
                    "sync {} → {} ({})",
                    source_env.env_name,
                    target_env.env_name,
                    recommended.kind,
                ),
                deployable_id: Some(dep_id.clone()),
                test_environment_id: Some(target_env.env_id.clone()),
                estimated_minutes,
                estimated_cost: 0.0,
                predecessors: vec![format!("infra-{}", source_env.env_id)],
            });
            *sync_total_minutes.entry(target_env.env_id.clone()).or_insert(0) += estimated_minutes;
        }
    }

    // ── 6. Emit a coordination task for any env whose `rate_limit` looks
    //       capped and is referenced by a sync; this gives Cityhall a "wait
    //       for rate-limit window" gantt entry.
    for env in &test_envs {
        let env_id = env.id.clone();
        let rate_limit = env
            .payload
            .get("rate_limit")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let kind = env
            .payload
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if rate_limit.is_empty() || kind != "external" {
            continue;
        }
        let used_as_target = sync_total_minutes.contains_key(&env_id)
            || env_for_dep.values().any(|e| e.env_id == env_id);
        if !used_as_target {
            continue;
        }
        tasks.push(EstimateTask {
            kind: "coordination".into(),
            label: format!(
                "wait for {} rate-limit window ({})",
                env.payload.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                rate_limit
            ),
            deployable_id: None,
            test_environment_id: Some(env_id.clone()),
            estimated_minutes: 5,
            estimated_cost: 0.0,
            predecessors: vec![format!("infra-{env_id}")],
        });
    }

    let total_minutes: u32 = tasks.iter().map(|t| t.estimated_minutes).sum();
    let total_cost: f64 = tasks.iter().map(|t| t.estimated_cost).sum();

    Ok(ComputedEstimate {
        change_request_id: inputs.change_request_id,
        change_request_summary: inputs.change_request_summary,
        tier: inputs.tier,
        tasks,
        blockers,
        total_minutes,
        total_cost,
        computed_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[derive(Clone)]
struct EnvForDep {
    env_id: String,
    env_name: String,
    env_kind: String,
    spinup_minutes: u32,
    cost_per_hour: f64,
}

fn classify_dep_edge(summary: &DeployableSummary, _upstream_id: &str) -> DependencyEdge {
    // For v0.1 we don't yet know per-edge protocol (Groundwork doesn't expose
    // it back through the dependency walk). Use the heuristic: any consumed
    // service ⇒ API-based; no services ⇒ shared db; explicit "event" in name
    // ⇒ event-based. The estimator and the standalone /data_sync/recommend
    // endpoint take the same DependencyEdge so callers can override.
    if summary
        .depends_on_services
        .iter()
        .any(|s| s.to_lowercase().contains("event") || s.to_lowercase().contains("topic"))
    {
        DependencyEdge::Event
    } else if summary.depends_on_services.is_empty() {
        DependencyEdge::SharedDb
    } else {
        DependencyEdge::Api
    }
}

fn default_sync_minutes(rec: &RecommendedSync) -> u32 {
    match rec.kind.as_str() {
        "push" => 30,
        "pull" => 45,
        "shared" => 5,
        _ => 30,
    }
}

fn find_existing_sync_minutes(
    syncs: &[meshql_core::Envelope],
    source_env_id: &str,
    target_env_id: &str,
) -> Option<u32> {
    for env in syncs {
        let s = env.payload.get("source_env_id").and_then(|v| v.as_str()).unwrap_or("");
        let t = env.payload.get("target_env_id").and_then(|v| v.as_str()).unwrap_or("");
        if s == source_env_id && t == target_env_id {
            if let Some(m) = env
                .payload
                .get("estimated_minutes")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u32>().ok())
            {
                return Some(m);
            }
        }
    }
    None
}

/// Topo-sort summaries by `depends_on`. Cycles are reported as blockers and
/// the leftover nodes are appended in arbitrary order.
fn topo_sort(
    summaries: &BTreeMap<String, DeployableSummary>,
    blockers: &mut Vec<String>,
) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut in_degree: BTreeMap<String, usize> = summaries.keys().map(|k| (k.clone(), 0)).collect();
    for (id, s) in summaries {
        for dep in &s.depends_on {
            if !summaries.contains_key(dep) {
                continue;
            }
            *in_degree.entry(id.clone()).or_insert(0) += 1;
        }
    }

    let mut ready: BTreeSet<String> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut out: Vec<String> = Vec::new();

    while let Some(id) = ready.iter().next().cloned() {
        ready.remove(&id);
        out.push(id.clone());
        for (other_id, other) in summaries {
            if other.depends_on.iter().any(|d| d == &id) {
                if let Some(d) = in_degree.get_mut(other_id) {
                    *d = d.saturating_sub(1);
                    if *d == 0 && !out.contains(other_id) {
                        ready.insert(other_id.clone());
                    }
                }
            }
        }
    }

    if out.len() < summaries.len() {
        let leftover: Vec<String> = summaries
            .keys()
            .filter(|k| !out.contains(k))
            .cloned()
            .collect();
        blockers.push(format!("dependency cycle involving: {leftover:?}"));
        out.extend(leftover);
    }

    out
}
