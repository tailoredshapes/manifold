//! ChangeRequest → DeploymentPlan resolver.
//!
//! The planner is generic over a `GroundworkLookup` trait so tests can stub
//! Groundwork. The HTTP-backed implementation lives in `groundwork_client.rs`.

use crate::bylaw::{self, EffectiveBylaw};
use anyhow::Context;
use async_trait::async_trait;
use meshql_core::Repository;
use std::collections::{BTreeMap, HashSet, VecDeque};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What the planner needs to know about a deployable in Groundwork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployableSummary {
    pub id: String,
    pub name: String,
    /// Resolved Union team UUID, or `None` if Groundwork has it un-set.
    pub team_id: Option<String>,
    /// Other deployables this one depends on (by id).
    pub depends_on: Vec<String>,
}

#[async_trait]
pub trait GroundworkLookup: Send + Sync {
    /// Return the deployable, or `None` if it does not exist.
    async fn get_deployable(&self, id: &str) -> anyhow::Result<Option<DeployableSummary>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub order: usize,
    pub deployable_id: String,
    pub deployable_name: String,
    pub action: String, // "deploy" | "verify" — for v0.1 always "deploy"
    pub predecessor_orders: Vec<usize>,
    pub gates: Vec<PlanGate>,
    pub estimated_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanGate {
    pub gate_type: String,
    pub source_org_node: String,
    pub description: Option<String>,
    pub window: Option<String>,
    pub approvers: Option<String>,
    pub quiesce_for: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedPlan {
    pub change_request_id: String,
    pub change_request_summary: String,
    pub tier: String,
    pub steps: Vec<PlanStep>,
    pub blockers: Vec<String>,
    pub computed_at: String,
}

/// Inputs to the planner.
pub struct PlanInputs<'a> {
    pub change_request_id: String,
    pub change_request_summary: String,
    pub tier: String,
    pub target_deployable_ids: Vec<String>,
    pub org_node_repo: &'a Arc<dyn Repository>,
    pub bylaw_repo: &'a Arc<dyn Repository>,
    pub groundwork: &'a dyn GroundworkLookup,
}

/// Compute a deployment plan. Always returns a `ComputedPlan` — `blockers` may
/// be non-empty if e.g. an affected deployable has no team.
pub async fn compute_plan(inputs: PlanInputs<'_>) -> anyhow::Result<ComputedPlan> {
    // ── 1. Walk Groundwork dependencies, collecting every deployable reachable
    //       from the targets, depth-first. Cycles are tolerated; we just don't
    //       revisit nodes.
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

    // ── 2. For each summary with no team_id, register an orphan blocker.
    for (_id, s) in &summaries {
        if s.team_id.as_deref().map(str::is_empty).unwrap_or(true) {
            blockers.push(format!("orphan: {}", s.name));
        }
    }

    // ── 3. Topological sort by depends_on. Dependencies first.
    let order = topo_sort(&summaries, &mut blockers);

    // ── 4. For each ordered deployable, fetch effective bylaws via the team's
    //       OrgNode and turn them into PlanGates.
    let mut steps: Vec<PlanStep> = Vec::new();
    let order_index: BTreeMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();

    for (i, dep_id) in order.iter().enumerate() {
        let summary = &summaries[dep_id];
        let predecessor_orders: Vec<usize> = summary
            .depends_on
            .iter()
            .filter_map(|d| order_index.get(d).copied())
            .collect();

        let gates = match summary.team_id.as_deref() {
            Some(team_id) if !team_id.is_empty() => {
                let bylaws = effective_bylaws_for_team(
                    inputs.org_node_repo,
                    inputs.bylaw_repo,
                    team_id,
                )
                .await?;
                bylaws.into_iter().map(plan_gate_from_bylaw).collect()
            }
            _ => Vec::new(),
        };

        steps.push(PlanStep {
            order: i,
            deployable_id: summary.id.clone(),
            deployable_name: summary.name.clone(),
            action: "deploy".to_string(),
            predecessor_orders,
            gates,
            estimated_minutes: 10,
        });
    }

    Ok(ComputedPlan {
        change_request_id: inputs.change_request_id,
        change_request_summary: inputs.change_request_summary,
        tier: inputs.tier,
        steps,
        blockers,
        computed_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Topological sort of the deployable graph. Cycles are reported as blockers
/// and the remaining nodes are appended in arbitrary order so the plan still
/// surfaces them to the user.
fn topo_sort(
    summaries: &BTreeMap<String, DeployableSummary>,
    blockers: &mut Vec<String>,
) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut in_degree: BTreeMap<String, usize> = summaries.keys().map(|k| (k.clone(), 0)).collect();
    for (id, s) in summaries {
        for dep in &s.depends_on {
            if !summaries.contains_key(dep) {
                continue; // reference outside the affected set — don't fail
            }
            // The convention here: a depends_on edge means "this depends on dep".
            // We want dep to come first, so dep's count of dependents (this) bumps
            // *id's* in_degree.
            *in_degree.entry(id.clone()).or_insert(0) += 1;
            let _ = dep;
        }
    }

    // Kahn's algorithm — but the in_degree above counts inbound edges to `id`
    // from each thing it depends on. Process zero-in-degree nodes first.
    let mut ready: BTreeSet<String> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| k.clone())
        .collect();
    let mut out: Vec<String> = Vec::new();

    while let Some(id) = ready.iter().next().cloned() {
        ready.remove(&id);
        out.push(id.clone());
        // Anyone whose deps include `id` has its in-degree reduced.
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
        // Report by name so the blocker message is readable in the UI.
        // Fall back to the id only if a name is somehow missing.
        let names: Vec<&str> = leftover
            .iter()
            .map(|id| summaries.get(id).map(|s| s.name.as_str()).unwrap_or(id.as_str()))
            .collect();
        blockers.push(format!("dependency cycle involving: {}", names.join(", ")));
        out.extend(leftover);
    }

    out
}

/// Fetch effective bylaws for a Union Team id by finding the OrgNode that
/// references it. If no OrgNode points at this team, return an empty list —
/// the planner upstream is responsible for noting unmapped teams.
async fn effective_bylaws_for_team(
    org_node_repo: &Arc<dyn Repository>,
    bylaw_repo: &Arc<dyn Repository>,
    team_id: &str,
) -> anyhow::Result<Vec<EffectiveBylaw>> {
    let nodes = org_node_repo.list(&[]).await.context("listing org_nodes")?;
    let leaf = nodes
        .iter()
        .find(|env| env.payload.get("team_id").and_then(|v| v.as_str()) == Some(team_id));
    let Some(leaf) = leaf else {
        return Ok(Vec::new());
    };
    bylaw::effective_bylaws_for(org_node_repo, bylaw_repo, &leaf.id).await
}

fn plan_gate_from_bylaw(b: EffectiveBylaw) -> PlanGate {
    PlanGate {
        gate_type: b.gate_type,
        source_org_node: b.org_node_name,
        description: b.description,
        window: b.window,
        approvers: b.approvers,
        quiesce_for: b.quiesce_for,
    }
}
