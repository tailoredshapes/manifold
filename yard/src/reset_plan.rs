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
use std::collections::{HashMap, HashSet};
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

    // Walk the full dataSync dependency chain → ordered steps + cycle blockers.
    let (steps, mut blockers) =
        plan_steps_and_cycles(inputs.target_env_id, &syncs, &envs, &infras, &sources);

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

    // `+ 0.0` strips negative-zero — summing an empty step list produces
    // -0.0 on some platforms and the wire JSON renders as `-0.0`, which is
    // ugly. Doesn't change any non-zero value.
    let estimated_total_minutes: f64 = steps.iter().map(|s| s.estimated_minutes).sum::<f64>() + 0.0;
    let estimated_total_cost: f64 = steps.iter().map(|s| s.estimated_cost).sum::<f64>() + 0.0;

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

/// Pure planning core: turns the loaded sync graph into an ordered list of
/// steps plus any cycle / ambiguous-feeder blockers.
///
/// **Algorithm.** Each `DataSync` is a node. An edge A → B exists when
/// B.target_env_id == A.source_env_id — i.e. B must complete before A so the
/// data A is about to pull is fresh. We DFS from every sync whose
/// `target_env_id` matches the requested target env, following source_env_id
/// chains as far as they go. A standard white/grey/black coloring detects
/// back-edges (cycles). Post-order emission yields a topological order
/// (predecessors before dependents). Cycles don't abort the walk: the
/// non-cyclic frontier is still planned, and each distinct cycle is reported
/// as a `cycle_detected` blocker.
fn plan_steps_and_cycles(
    target_env_id: &str,
    syncs: &[DataSync],
    envs: &HashMap<String, EnvLite>,
    infras: &HashMap<String, InfraLite>,
    sources: &HashMap<String, SourceLite>,
) -> (Vec<ResetStep>, Vec<ResetBlocker>) {
    let sync_by_id: HashMap<&str, &DataSync> =
        syncs.iter().map(|s| (s.id.as_str(), s)).collect();

    // For env→sync lookup we need at most one feeder per env. If multiple
    // syncs target the same env, prefer a deterministic pick (lowest id);
    // we surface the duplication as a blocker so operators can untangle it.
    let mut feeder_by_env: HashMap<&str, &DataSync> = HashMap::new();
    let mut envs_with_multiple_feeders: HashSet<&str> = HashSet::new();
    for sync in syncs {
        let target = sync.target_env_id.as_str();
        match feeder_by_env.get(target) {
            None => {
                feeder_by_env.insert(target, sync);
            }
            Some(existing) => {
                envs_with_multiple_feeders.insert(target);
                if sync.id < existing.id {
                    feeder_by_env.insert(target, sync);
                }
            }
        }
    }

    let direct_syncs: Vec<&DataSync> = syncs
        .iter()
        .filter(|s| s.target_env_id == target_env_id)
        .collect();

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Grey,
        Black,
    }
    let mut color: HashMap<&str, Color> =
        syncs.iter().map(|s| (s.id.as_str(), Color::White)).collect();
    let mut stack_envs: Vec<&str> = Vec::new();
    let mut post_order: Vec<&str> = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    fn visit<'a>(
        sync_id: &'a str,
        sync_by_id: &HashMap<&'a str, &'a DataSync>,
        feeder_by_env: &HashMap<&'a str, &'a DataSync>,
        color: &mut HashMap<&'a str, Color>,
        stack_envs: &mut Vec<&'a str>,
        post_order: &mut Vec<&'a str>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        let Some(sync) = sync_by_id.get(sync_id).copied() else {
            return;
        };
        match color.get(sync_id).copied().unwrap_or(Color::White) {
            Color::Black => return,
            Color::Grey => return, // shouldn't happen — caller guards
            Color::White => {}
        }
        color.insert(sync_id, Color::Grey);
        stack_envs.push(sync.target_env_id.as_str());

        if let Some(src_env_id) = sync.source_env_id.as_deref() {
            if let Some(cycle_start) = stack_envs.iter().position(|e| *e == src_env_id) {
                // Back-edge to an env on the current path → cycle.
                let mut cyc: Vec<String> = stack_envs[cycle_start..]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                cyc.push(src_env_id.to_string()); // close the loop visually
                cycles.push(cyc);
            } else if let Some(pred_sync) = feeder_by_env.get(src_env_id).copied() {
                if color
                    .get(pred_sync.id.as_str())
                    .copied()
                    .unwrap_or(Color::White)
                    != Color::Black
                {
                    visit(
                        pred_sync.id.as_str(),
                        sync_by_id,
                        feeder_by_env,
                        color,
                        stack_envs,
                        post_order,
                        cycles,
                    );
                }
            }
        }

        stack_envs.pop();
        color.insert(sync_id, Color::Black);
        post_order.push(sync_id);
    }

    for sync in &direct_syncs {
        if color.get(sync.id.as_str()).copied().unwrap_or(Color::White) == Color::White {
            visit(
                sync.id.as_str(),
                &sync_by_id,
                &feeder_by_env,
                &mut color,
                &mut stack_envs,
                &mut post_order,
                &mut cycles,
            );
        }
    }

    let mut steps: Vec<ResetStep> = Vec::new();
    let mut step_index_by_sync: HashMap<String, usize> = HashMap::new();
    for (idx, sync_id) in post_order.iter().enumerate() {
        let order = idx + 1;
        let Some(sync) = sync_by_id.get(sync_id).copied() else {
            continue;
        };
        let preds: Vec<usize> = sync
            .source_env_id
            .as_deref()
            .and_then(|src_id| feeder_by_env.get(src_id).copied())
            .and_then(|pre| step_index_by_sync.get(&pre.id).copied())
            .map(|o| vec![o])
            .unwrap_or_default();
        let step = build_step(order, sync, envs, infras, sources, &preds);
        step_index_by_sync.insert(sync.id.clone(), order);
        steps.push(step);
    }

    let mut blockers: Vec<ResetBlocker> = Vec::new();

    if !cycles.is_empty() {
        // One blocker per distinct cycle. Dedupe on the env-id set, since
        // the same SCC can be reached from multiple DFS entry points.
        let mut seen: HashSet<Vec<String>> = HashSet::new();
        for cyc in &cycles {
            let mut key: Vec<String> = cyc
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            key.sort();
            if !seen.insert(key) {
                continue;
            }
            let names: Vec<String> = cyc
                .iter()
                .map(|id| {
                    envs.get(id)
                        .map(|e| e.name.clone())
                        .unwrap_or_else(|| id.clone())
                })
                .collect();
            let refs: Vec<String> = {
                let mut s: HashSet<String> = HashSet::new();
                cyc.iter()
                    .filter(|id| s.insert((*id).clone()))
                    .cloned()
                    .collect()
            };
            blockers.push(ResetBlocker {
                kind: "cycle_detected".into(),
                message: format!(
                    "Cycle in dataSync source chain: {}",
                    names.join(" \u{2192} ")
                ),
                references: refs,
            });
        }
    }

    if !envs_with_multiple_feeders.is_empty() {
        let mut env_ids: Vec<&str> = envs_with_multiple_feeders.iter().copied().collect();
        env_ids.sort(); // deterministic order for stable test assertions
        let names: Vec<String> = env_ids
            .iter()
            .map(|id| {
                envs.get(*id)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| (*id).to_string())
            })
            .collect();
        blockers.push(ResetBlocker {
            kind: "ambiguous_feeder".into(),
            message: format!(
                "{} env{} have multiple syncs targeting them; planner picked one deterministically — review: {}",
                env_ids.len(),
                if env_ids.len() == 1 { "" } else { "s" },
                names.join(", ")
            ),
            references: env_ids.iter().map(|s| (*s).to_string()).collect(),
        });
    }

    (steps, blockers)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env(id: &str, name: &str) -> (String, EnvLite) {
        (
            id.to_string(),
            EnvLite {
                name: name.to_string(),
                cost_per_hour: None,
                infrastructure_id: None,
            },
        )
    }

    fn sync(id: &str, target: &str, source_env: Option<&str>) -> DataSync {
        DataSync {
            id: id.to_string(),
            kind: "snapshot".into(),
            target_env_id: target.to_string(),
            source_env_id: source_env.map(String::from),
            source_data_id: None,
            refresh_policy: None,
            estimated_minutes: Some(1.0),
            masking_summary: None,
        }
    }

    #[test]
    fn multi_hop_chain_orders_predecessors_first() {
        // C ← B ← A (target A pulls from B, B pulls from C)
        let envs: HashMap<String, EnvLite> =
            [env("env-a", "A"), env("env-b", "B"), env("env-c", "C")]
                .into_iter()
                .collect();
        let syncs = vec![
            sync("sync-a", "env-a", Some("env-b")),
            sync("sync-b", "env-b", Some("env-c")),
            sync("sync-c", "env-c", None),
        ];
        let (steps, blockers) = plan_steps_and_cycles(
            "env-a",
            &syncs,
            &envs,
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(blockers.len(), 0, "no blockers expected: {:?}", blockers);
        assert_eq!(steps.len(), 3, "expected 3 steps, got {:?}", steps);

        // Topological order: sync-c (no source) → sync-b → sync-a.
        assert_eq!(steps[0].data_sync_id, "sync-c");
        assert_eq!(steps[0].order, 1);
        assert!(steps[0].predecessor_orders.is_empty());

        assert_eq!(steps[1].data_sync_id, "sync-b");
        assert_eq!(steps[1].order, 2);
        assert_eq!(steps[1].predecessor_orders, vec![1]);

        assert_eq!(steps[2].data_sync_id, "sync-a");
        assert_eq!(steps[2].order, 3);
        assert_eq!(steps[2].predecessor_orders, vec![2]);
    }

    #[test]
    fn cycle_emits_cycle_detected_blocker_and_still_plans_acyclic_portion() {
        // env-a ← env-b ← env-a  (two-cycle)
        let envs: HashMap<String, EnvLite> =
            [env("env-a", "A"), env("env-b", "B")].into_iter().collect();
        let syncs = vec![
            sync("sync-a", "env-a", Some("env-b")),
            sync("sync-b", "env-b", Some("env-a")),
        ];
        let (steps, blockers) = plan_steps_and_cycles(
            "env-a",
            &syncs,
            &envs,
            &HashMap::new(),
            &HashMap::new(),
        );

        let cycle_blockers: Vec<&ResetBlocker> = blockers
            .iter()
            .filter(|b| b.kind == "cycle_detected")
            .collect();
        assert_eq!(
            cycle_blockers.len(),
            1,
            "expected exactly one cycle blocker, got: {:?}",
            blockers
        );
        let cycle = cycle_blockers[0];
        assert!(
            cycle.message.contains("Cycle in dataSync source chain"),
            "unexpected cycle message: {}",
            cycle.message
        );
        assert!(cycle.references.contains(&"env-a".to_string()));
        assert!(cycle.references.contains(&"env-b".to_string()));

        // Best-effort: both syncs should still be planned (post-order off the
        // grey-hit doesn't abort the parent's emission).
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn three_cycle_is_detected() {
        // env-a ← env-b ← env-c ← env-a
        let envs: HashMap<String, EnvLite> =
            [env("env-a", "A"), env("env-b", "B"), env("env-c", "C")]
                .into_iter()
                .collect();
        let syncs = vec![
            sync("sync-a", "env-a", Some("env-b")),
            sync("sync-b", "env-b", Some("env-c")),
            sync("sync-c", "env-c", Some("env-a")),
        ];
        let (_steps, blockers) = plan_steps_and_cycles(
            "env-a",
            &syncs,
            &envs,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(
            blockers.iter().any(|b| b.kind == "cycle_detected"),
            "expected cycle_detected blocker, got: {:?}",
            blockers
        );
    }

    #[test]
    fn sync_with_only_source_data_id_has_no_predecessor() {
        // External data source, no upstream env to refresh first.
        let envs: HashMap<String, EnvLite> = [env("env-a", "A")].into_iter().collect();
        let sources: HashMap<String, SourceLite> = [(
            "src-1".to_string(),
            SourceLite {
                name: "prod-snapshot".into(),
            },
        )]
        .into_iter()
        .collect();
        let mut s = sync("sync-a", "env-a", None);
        s.source_data_id = Some("src-1".into());
        let syncs = vec![s];
        let (steps, blockers) =
            plan_steps_and_cycles("env-a", &syncs, &envs, &HashMap::new(), &sources);
        assert!(blockers.is_empty(), "unexpected blockers: {:?}", blockers);
        assert_eq!(steps.len(), 1);
        assert!(steps[0].predecessor_orders.is_empty());
        assert!(steps[0].source_label.contains("prod-snapshot"));
    }

    #[test]
    fn multiple_feeders_for_one_env_surfaces_ambiguous_feeder_blocker() {
        let envs: HashMap<String, EnvLite> =
            [env("env-a", "A"), env("env-b", "B")].into_iter().collect();
        let syncs = vec![
            sync("sync-a", "env-a", Some("env-b")),
            // Two syncs feeding env-b — operator misconfig.
            sync("sync-b1", "env-b", None),
            sync("sync-b2", "env-b", None),
        ];
        let (steps, blockers) = plan_steps_and_cycles(
            "env-a",
            &syncs,
            &envs,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(
            blockers.iter().any(|b| b.kind == "ambiguous_feeder"),
            "expected ambiguous_feeder blocker, got: {:?}",
            blockers
        );
        // Deterministic pick: lowest id (sync-b1) is the chosen feeder.
        let env_b_step = steps
            .iter()
            .find(|s| s.target_env_id == "env-b")
            .expect("expected an env-b step");
        assert_eq!(env_b_step.data_sync_id, "sync-b1");
    }

    #[test]
    fn single_direct_sync_with_no_source_emits_one_step() {
        let envs: HashMap<String, EnvLite> = [env("env-a", "A")].into_iter().collect();
        let syncs = vec![sync("sync-a", "env-a", None)];
        let (steps, blockers) = plan_steps_and_cycles(
            "env-a",
            &syncs,
            &envs,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(blockers.is_empty());
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].order, 1);
        assert!(steps[0].predecessor_orders.is_empty());
    }
}
