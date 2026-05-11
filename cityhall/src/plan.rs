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

/// A blocker that prevents the plan from running clean. `message` is the
/// human-readable summary; `mermaid`, when present, is a small Mermaid graph
/// the frontend can render alongside the message (used for `dependency_cycle`
/// blockers where the structural shape matters more than the prose).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocker {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mermaid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedPlan {
    pub change_request_id: String,
    pub change_request_summary: String,
    pub tier: String,
    pub steps: Vec<PlanStep>,
    pub blockers: Vec<Blocker>,
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
    let mut blockers: Vec<Blocker> = Vec::new();
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
            blockers.push(Blocker {
                kind: "unknown_deployable".into(),
                message: format!("unknown deployable: {id}"),
                mermaid: None,
            });
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
            blockers.push(Blocker {
                kind: "orphan".into(),
                message: format!("orphan: {}", s.name),
                mermaid: None,
            });
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
    blockers: &mut Vec<Blocker>,
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
        let message = format!("dependency cycle involving: {}", names.join(", "));
        let mermaid = render_cycle_mermaid(&leftover, summaries);
        blockers.push(Blocker {
            kind: "dependency_cycle".into(),
            message,
            mermaid: Some(mermaid),
        });
        out.extend(leftover);
    }

    out
}

/// Render the cycle subgraph as a small Mermaid `graph LR`. Only edges where
/// both endpoints are in the cycle's node set are emitted — edges to nodes
/// outside the cycle aren't part of the cycle itself.
///
/// Nodes and edges that participate in an actual cycle (vs. dependents that
/// were merely dragged into the leftover set) are decorated:
///   - cycle nodes get class `cyclic` (red stroke + light red fill)
///   - cycle edges get a `linkStyle` directive (red, thicker)
///
/// Cycle membership is computed by reachability: a node is in a cycle if it
/// can reach itself via at least one edge; an edge (u, v) is on a cycle if v
/// can reach u.
fn render_cycle_mermaid(
    cycle_ids: &[String],
    summaries: &BTreeMap<String, DeployableSummary>,
) -> String {
    use std::collections::HashMap;
    let id_to_idx: HashMap<&str, usize> = cycle_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    // Build the adjacency list restricted to the cycle node set so the
    // reachability checks below stay inside the subgraph we're rendering.
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for id in cycle_ids {
        let entry = adj.entry(id.as_str()).or_default();
        let Some(s) = summaries.get(id) else { continue };
        for dep_id in &s.depends_on {
            if id_to_idx.contains_key(dep_id.as_str()) {
                entry.push(dep_id.as_str());
            }
        }
    }

    let on_cycle_node: HashMap<&str, bool> = cycle_ids
        .iter()
        .map(|id| (id.as_str(), reachable_nontrivial(id, id, &adj)))
        .collect();

    let mut out = String::from("graph LR\n");

    // Nodes. Cycle-participating nodes get the `:::cyclic` class suffix.
    for (i, id) in cycle_ids.iter().enumerate() {
        let name = summaries
            .get(id)
            .map(|s| s.name.as_str())
            .unwrap_or(id.as_str());
        let safe = name.replace('"', r#"\""#);
        let suffix = if *on_cycle_node.get(id.as_str()).unwrap_or(&false) {
            ":::cyclic"
        } else {
            ""
        };
        out.push_str(&format!("    n{i}[\"{safe}\"]{suffix}\n"));
    }

    // Intra-cycle-set edges. Track which ones are on a cycle so we can emit
    // matching linkStyle directives by their 0-based emission index.
    let mut edge_idx: usize = 0;
    let mut cyclic_edge_indices: Vec<usize> = Vec::new();
    for id in cycle_ids {
        let Some(&from) = id_to_idx.get(id.as_str()) else { continue };
        let Some(s) = summaries.get(id) else { continue };
        for dep_id in &s.depends_on {
            if let Some(&to) = id_to_idx.get(dep_id.as_str()) {
                out.push_str(&format!("    n{from} --> n{to}\n"));
                if reachable_nontrivial(dep_id, id, &adj) {
                    cyclic_edge_indices.push(edge_idx);
                }
                edge_idx += 1;
            }
        }
    }

    // Style decorations. Mermaid applies classDef + linkStyle after all
    // node/edge declarations.
    if on_cycle_node.values().any(|&v| v) {
        out.push_str(
            "    classDef cyclic stroke:#dc2626,stroke-width:2px,fill:#fef2f2,color:#7f1d1d\n",
        );
    }
    for idx in cyclic_edge_indices {
        out.push_str(&format!(
            "    linkStyle {idx} stroke:#dc2626,stroke-width:2.5px\n"
        ));
    }

    out
}

/// Returns true iff there's a path of length ≥ 1 from `src` to `dst` in `adj`.
/// Used by `render_cycle_mermaid` to classify nodes (self-reachable = on a
/// cycle) and edges (target reaches source = edge is on a cycle).
fn reachable_nontrivial(
    src: &str,
    dst: &str,
    adj: &std::collections::HashMap<&str, Vec<&str>>,
) -> bool {
    use std::collections::HashSet;
    let Some(start) = adj.get(src) else { return false };
    let mut stack: Vec<&str> = start.iter().copied().collect();
    let mut visited: HashSet<&str> = HashSet::new();
    while let Some(node) = stack.pop() {
        if node == dst {
            return true;
        }
        if !visited.insert(node) {
            continue;
        }
        if let Some(neighbours) = adj.get(node) {
            for n in neighbours {
                stack.push(n);
            }
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, name: &str, depends_on: &[&str]) -> (String, DeployableSummary) {
        (
            id.to_string(),
            DeployableSummary {
                id: id.to_string(),
                name: name.to_string(),
                team_id: Some("t".into()),
                depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
            },
        )
    }

    #[test]
    fn cycle_mermaid_highlights_only_real_cyclic_nodes_and_edges() {
        // Three nodes in the leftover set:
        //   a (dragged-in dependent) depends_on b
        //   b ↔ c (the actual cycle)
        // Expected: b and c get classed `cyclic`; the b↔c edges get linkStyle;
        // a stays unstyled and the a→b edge stays default.
        let mut summaries: BTreeMap<String, DeployableSummary> = BTreeMap::new();
        let (k, v) = summary("a", "A", &["b"]);
        summaries.insert(k, v);
        let (k, v) = summary("b", "B", &["c"]);
        summaries.insert(k, v);
        let (k, v) = summary("c", "C", &["b"]);
        summaries.insert(k, v);

        let cycle_ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let out = render_cycle_mermaid(&cycle_ids, &summaries);

        // Mermaid graph header
        assert!(out.starts_with("graph LR\n"), "got: {out}");
        // a is dragged-in: NO cyclic class
        assert!(out.contains("n0[\"A\"]\n"), "a should be unclassed: {out}");
        // b and c are on the cycle: classed cyclic
        assert!(out.contains("n1[\"B\"]:::cyclic\n"), "b should be cyclic: {out}");
        assert!(out.contains("n2[\"C\"]:::cyclic\n"), "c should be cyclic: {out}");
        // Edges emitted in order: a→b (idx 0, NOT cyclic), b→c (idx 1, cyclic), c→b (idx 2, cyclic)
        assert!(out.contains("n0 --> n1\n"), "a→b edge missing: {out}");
        assert!(out.contains("n1 --> n2\n"), "b→c edge missing: {out}");
        assert!(out.contains("n2 --> n1\n"), "c→b edge missing: {out}");
        // linkStyle: idx 1 and idx 2 are cyclic; idx 0 (a→b) is not
        assert!(out.contains("linkStyle 1 stroke:#dc2626"), "b→c edge should be styled: {out}");
        assert!(out.contains("linkStyle 2 stroke:#dc2626"), "c→b edge should be styled: {out}");
        assert!(!out.contains("linkStyle 0"), "a→b edge must not be styled: {out}");
        // classDef appears once
        assert_eq!(
            out.matches("classDef cyclic").count(),
            1,
            "classDef must appear once: {out}"
        );
    }

    #[test]
    fn cycle_mermaid_with_no_actual_cycle_emits_no_classdef() {
        // Pathological "leftover" where there's no actual cycle (e.g., if the
        // caller passed a chain that topo-sort already handled). Defensive:
        // no classDef, no linkStyle.
        let mut summaries: BTreeMap<String, DeployableSummary> = BTreeMap::new();
        let (k, v) = summary("a", "A", &["b"]);
        summaries.insert(k, v);
        let (k, v) = summary("b", "B", &[]);
        summaries.insert(k, v);
        let out = render_cycle_mermaid(&["a".to_string(), "b".to_string()], &summaries);
        assert!(out.contains("n0[\"A\"]\n"));
        assert!(out.contains("n1[\"B\"]\n"));
        assert!(out.contains("n0 --> n1\n"));
        assert!(!out.contains("classDef cyclic"), "no classDef expected: {out}");
        assert!(!out.contains("linkStyle"), "no linkStyle expected: {out}");
    }
}
