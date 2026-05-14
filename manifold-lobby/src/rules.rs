//! Advisory derivation rules.
//!
//! Each rule is a pure function over a [`GraphSnapshot`] returning the set
//! of advisories it would raise *if it were the only rule running*. The
//! engine reconciles the rule output against existing Advisory envelopes:
//! new outputs become `raised`; outputs that previously existed are kept
//! (possibly transitioning state through ack/dismiss/escalate independently);
//! previous outputs no longer in the set transition to `resolved`.
//!
//! Rules are versioned via the `RULE` constant (e.g. `"blocked-upstream@v1"`).
//! Changing a rule's logic means bumping the version so existing dismissals
//! aren't retroactively invalidated.

use crate::snapshot::*;
use std::collections::{HashMap, HashSet};

/// One advisory as a rule wants to surface it, before reconciliation with
/// existing state. The engine turns this into an Advisory envelope.
#[derive(Debug, Clone)]
pub struct DerivedAdvisory {
    pub kind: &'static str,
    pub subject_type: &'static str,
    pub subject_id: String,
    pub subject_name: String,
    pub severity: Severity,
    pub rule: &'static str,
    pub explain: String,
    /// Identifying ids referenced by the explain — let the engine attribute
    /// the advisory to the events/entities that caused it.
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warn,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Critical => "critical",
        }
    }
}

/// The set of all rules to run.
pub fn all_rules() -> Vec<RuleFn> {
    vec![
        blocked_upstream_v1,
        circular_dependency_v1,
        undocumented_interface_v1,
        watershed_mismatch_v1,
        missing_environment_v1,
        schedule_contention_v1,
    ]
}

pub type RuleFn = fn(&GraphSnapshot) -> Vec<DerivedAdvisory>;

// ─── BlockedUpstream@v1 ────────────────────────────────────────────────────

/// Detects blocked-upstream by combining two signals:
///
/// 1. A `ChangeRequest` whose `status` is `blocked` declares its
///    `target_deployables` as blocked.
/// 2. A `WorkOrder` whose `status` is `blocked` declares its
///    `deployable_id` as blocked.
///
/// For each blocked deployable B, every deployable A that has a `Dependency`
/// targeting B (via `service_id` matching B's id as the v1 proxy for
/// "depends on") gets a BlockedUpstream advisory.
pub fn blocked_upstream_v1(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    const RULE: &str = "blocked-upstream@v1";

    // Collect blockers from both CRs and work orders.
    let mut blocker_by_dep: HashMap<String, String> = HashMap::new();
    for cr in &snap.change_requests {
        if cr.status.as_deref() != Some("blocked") {
            continue;
        }
        let Some(targets) = cr.target_deployables.as_deref() else {
            continue;
        };
        let summary = cr.summary.as_deref().unwrap_or(&cr.id);
        for t in targets.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            blocker_by_dep
                .entry(t.to_string())
                .or_insert_with(|| format!("blocked CR: {summary}"));
        }
    }
    for wo in &snap.work_orders {
        if wo.status.as_deref() != Some("blocked") {
            continue;
        }
        let Some(d) = wo.deployable_id.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        blocker_by_dep
            .entry(d.to_string())
            .or_insert_with(|| "blocked work order".into());
    }
    if blocker_by_dep.is_empty() {
        return Vec::new();
    }

    // Resolve names.
    let name_of: HashMap<&str, &str> = snap
        .deployables
        .iter()
        .map(|d| (d.id.as_str(), d.name.as_str()))
        .collect();

    // Build dependency graph at the deployable level (the cleaner shape).
    // Edge A → B means deployable A depends on deployable B (a `Dependency`
    // where deployable_id=A, service_id=S, and S resolves via some Service
    // to a deployable). We don't have service→deployable links explicitly,
    // so for v1 we propagate blockers through `Dependency.service_id`
    // only when the service's id MATCHES a deployable id (an
    // approximation: services and deployables often share short identifiers
    // in the demo data).
    //
    // Real federation will tighten this once `Exposes` is wired across.

    let dep_ids: HashSet<&str> = snap.deployables.iter().map(|d| d.id.as_str()).collect();

    // Index `exposes` so we can resolve a `Dependency.service_id` → the
    // deployable(s) that expose that service.
    let exposers_for_service = exposers_index(&snap.exposes);

    // For each blocked deployable B, find every deployable A that depends
    // (via Dependency → service → Exposes) on B. Emit one advisory per
    // (depender, B) pair.
    let mut out = Vec::new();
    for (blocked_id, blocker_msg) in &blocker_by_dep {
        let blocked_id = blocked_id.as_str();
        if !dep_ids.contains(blocked_id) {
            continue;
        }
        let blocked_name = name_of.get(blocked_id).copied().unwrap_or(blocked_id);
        for edge in &snap.dependencies {
            // Does this Dependency edge resolve to the blocked deployable?
            let exposers = exposers_for_service.get(edge.service_id.as_str());
            let Some(exposers) = exposers else { continue };
            if !exposers.contains(&blocked_id) {
                continue;
            }
            let depender = edge.deployable_id.as_str();
            if !dep_ids.contains(depender) || depender == blocked_id {
                continue;
            }
            let depender_name = name_of.get(depender).copied().unwrap_or(depender);
            let crit = edge.criticality.as_deref().unwrap_or("normal");
            let severity = match crit {
                "high" | "critical" => Severity::Critical,
                "medium" => Severity::Warn,
                _ => Severity::Info,
            };
            out.push(DerivedAdvisory {
                kind: "BlockedUpstream",
                subject_type: "deployable",
                subject_id: depender.to_string(),
                subject_name: depender_name.to_string(),
                severity,
                rule: RULE,
                explain: format!(
                    "{depender_name} depends on {blocked_name} ({crit} criticality). {blocked_name}: {blocker_msg}"
                ),
                caused_by: vec![blocked_id.to_string(), edge.id.clone()],
            });
        }
    }
    out
}

/// Build a map of `service_id` → `Vec<deployable_id>` from the snapshot's
/// Exposes edges. Used by both BlockedUpstream and CircularDependency to
/// resolve "this dependency on service S lands on which deployable?"
fn exposers_index<'a>(exposes: &'a [Exposes]) -> HashMap<&'a str, Vec<&'a str>> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in exposes {
        out.entry(e.service_id.as_str())
            .or_default()
            .push(e.deployable_id.as_str());
    }
    out
}

// ─── CircularDependency@v1 ─────────────────────────────────────────────────

/// Tarjan's strongly-connected-components over the deployable dependency
/// graph. For every SCC of size >= 2, emit one advisory per cycle (subject =
/// "first deployable in the cycle" alphabetically — stable identity).
pub fn circular_dependency_v1(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    const RULE: &str = "circular-dependency@v1";

    let dep_ids: Vec<&str> = snap.deployables.iter().map(|d| d.id.as_str()).collect();
    let index_of: HashMap<&str, usize> = dep_ids.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let n = dep_ids.len();
    let exposers_for_service = exposers_index(&snap.exposes);

    // Build adjacency: for each Dependency edge A → S, look up which
    // deployable(s) expose S; add A → B edges for each exposer B.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for d in &snap.dependencies {
        let Some(&si) = index_of.get(d.deployable_id.as_str()) else {
            continue;
        };
        let Some(targets) = exposers_for_service.get(d.service_id.as_str()) else {
            continue;
        };
        for target in targets {
            if let Some(&ti) = index_of.get(target) {
                if si != ti {
                    adj[si].push(ti);
                }
            }
        }
    }

    let sccs = tarjan_scc(n, &adj);

    let mut out = Vec::new();
    for scc in sccs.into_iter().filter(|c| c.len() >= 2) {
        let mut names: Vec<(&str, &str)> = scc
            .iter()
            .map(|&i| {
                let id = dep_ids[i];
                let nm = snap
                    .deployables
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| d.name.as_str())
                    .unwrap_or(id);
                (id, nm)
            })
            .collect();
        names.sort_by_key(|(_, n)| *n);
        let subject_id = names[0].0.to_string();
        let subject_name = names[0].1.to_string();
        let cycle_text = names
            .iter()
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
            .join(" → ");
        out.push(DerivedAdvisory {
            kind: "CircularDependency",
            subject_type: "deployable",
            subject_id,
            subject_name,
            severity: Severity::Warn,
            rule: RULE,
            explain: format!(
                "Dependency cycle involving {} deployables: {} → ({} cycles back to start)",
                names.len(),
                cycle_text,
                names[0].1
            ),
            caused_by: names.iter().map(|(id, _)| id.to_string()).collect(),
        });
    }
    out
}

fn tarjan_scc(n: usize, adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut index = 0usize;
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack = vec![false; n];
    let mut indices = vec![usize::MAX; n];
    let mut lowlinks = vec![usize::MAX; n];
    let mut out: Vec<Vec<usize>> = Vec::new();

    fn strongconnect(
        v: usize,
        adj: &[Vec<usize>],
        index: &mut usize,
        stack: &mut Vec<usize>,
        on_stack: &mut [bool],
        indices: &mut [usize],
        lowlinks: &mut [usize],
        out: &mut Vec<Vec<usize>>,
    ) {
        indices[v] = *index;
        lowlinks[v] = *index;
        *index += 1;
        stack.push(v);
        on_stack[v] = true;

        for &w in &adj[v] {
            if indices[w] == usize::MAX {
                strongconnect(w, adj, index, stack, on_stack, indices, lowlinks, out);
                lowlinks[v] = lowlinks[v].min(lowlinks[w]);
            } else if on_stack[w] {
                lowlinks[v] = lowlinks[v].min(indices[w]);
            }
        }

        if lowlinks[v] == indices[v] {
            let mut scc = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            out.push(scc);
        }
    }

    for v in 0..n {
        if indices[v] == usize::MAX {
            strongconnect(
                v,
                adj,
                &mut index,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlinks,
                &mut out,
            );
        }
    }
    out
}

// ─── UndocumentedInterface@v1 ──────────────────────────────────────────────

/// For every service that is depended on (i.e. appears as `service_id` on at
/// least one Dependency edge) but has no Contract published, emit on the
/// service. Severity scales with dependent count.
pub fn undocumented_interface_v1(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    const RULE: &str = "undocumented-interface@v1";

    let documented: HashSet<&str> = snap
        .contracts
        .iter()
        .map(|c| c.service_id.as_str())
        .collect();

    let mut dependent_count: HashMap<&str, usize> = HashMap::new();
    for d in &snap.dependencies {
        *dependent_count.entry(d.service_id.as_str()).or_insert(0) += 1;
    }

    let mut out = Vec::new();
    for s in &snap.services {
        let n = dependent_count.get(s.id.as_str()).copied().unwrap_or(0);
        if n == 0 || documented.contains(s.id.as_str()) {
            continue;
        }
        let severity = if n >= 3 {
            Severity::Warn
        } else {
            Severity::Info
        };
        out.push(DerivedAdvisory {
            kind: "UndocumentedInterface",
            subject_type: "service",
            subject_id: s.id.clone(),
            subject_name: s.name.clone(),
            severity,
            rule: RULE,
            explain: format!(
                "{} is depended on by {n} deployable(s) but publishes no contract.",
                s.name
            ),
            caused_by: vec![s.id.clone()],
        });
    }
    out
}

// ─── WatershedMismatch@v1 ──────────────────────────────────────────────────

/// A test environment that declares a watershed of `prod-like` must have at
/// least one DataSync targeting it (a prod snapshot pipe). If no sync is
/// declared, surface the mismatch — the env claims a fidelity it doesn't
/// actually deliver.
///
/// Future enrichment: also check Cityhall plans that promote *through* tiers
/// in a way that skips watersheds. That requires plan-tier metadata which
/// isn't in the current schema.
pub fn watershed_mismatch_v1(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    const RULE: &str = "watershed-mismatch@v1";

    let synced_targets: HashSet<&str> = snap
        .data_syncs
        .iter()
        .filter_map(|s| s.target_env_id.as_deref())
        .collect();

    let mut out = Vec::new();
    for env in &snap.test_environments {
        let Some(ws) = env.watershed.as_deref() else {
            continue;
        };
        if ws == "prod-like" && !synced_targets.contains(env.id.as_str()) {
            out.push(DerivedAdvisory {
                kind: "WatershedMismatch",
                subject_type: "test_environment",
                subject_id: env.id.clone(),
                subject_name: env.name.clone(),
                severity: Severity::Warn,
                rule: RULE,
                explain: format!(
                    "{} claims watershed=prod-like but has no DataSync feeding it (no path for prod-like fidelity).",
                    env.name
                ),
                caused_by: vec![env.id.clone()],
            });
        }
    }
    out
}

// ─── MissingEnvironment@v1 ─────────────────────────────────────────────────

/// For each change request, look at its target deployables, and check that
/// at least one TestEnvironment exists per deployable for the tier the CR
/// targets. If not, raise on the CR.
pub fn missing_environment_v1(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    const RULE: &str = "missing-environment@v1";

    // Index environments by deployable_id.
    let mut envs_for_dep: HashMap<&str, Vec<&TestEnvironment>> = HashMap::new();
    for env in &snap.test_environments {
        if let Some(d) = env.deployable_id.as_deref().filter(|s| !s.is_empty()) {
            envs_for_dep.entry(d).or_default().push(env);
        }
    }

    let name_of: HashMap<&str, &str> = snap
        .deployables
        .iter()
        .map(|d| (d.id.as_str(), d.name.as_str()))
        .collect();

    let mut out = Vec::new();
    for cr in &snap.change_requests {
        let Some(targets) = cr.target_deployables.as_deref() else {
            continue;
        };
        let targets: Vec<&str> = targets
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if targets.is_empty() {
            continue;
        }
        let missing: Vec<&str> = targets
            .iter()
            .copied()
            .filter(|d| !envs_for_dep.contains_key(*d))
            .collect();
        if missing.is_empty() {
            continue;
        }
        let cr_summary = cr.summary.clone().unwrap_or_else(|| cr.id.clone());
        let missing_names: Vec<&str> = missing
            .iter()
            .map(|id| name_of.get(*id).copied().unwrap_or(*id))
            .collect();
        out.push(DerivedAdvisory {
            kind: "MissingEnvironment",
            subject_type: "change_request",
            subject_id: cr.id.clone(),
            subject_name: cr_summary.clone(),
            severity: Severity::Warn,
            rule: RULE,
            explain: format!(
                "{cr_summary}: no test environment registered for: {}",
                missing_names.join(", ")
            ),
            caused_by: missing.iter().map(|s| s.to_string()).collect(),
        });
    }
    out
}

// ─── ScheduleContention@v1 ─────────────────────────────────────────────────

/// For every pair of plan-step windows targeting the same deployable that
/// overlap in time, surface contention on the CRs the plans belong to.
///
/// Two-pair output: each side learns about the other. Stable subject id
/// is the *smaller* of the two CR ids (so reconciliation is idempotent).
pub fn schedule_contention_v1(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    const RULE: &str = "schedule-contention@v1";

    // Index plans by CR.
    let mut plan_for_cr: HashMap<&str, &DeploymentPlan> = HashMap::new();
    for plan in &snap.deployment_plans {
        plan_for_cr.insert(plan.change_request_id.as_str(), plan);
    }

    let cr_summary: HashMap<&str, &str> = snap
        .change_requests
        .iter()
        .map(|cr| {
            (
                cr.id.as_str(),
                cr.summary.as_deref().unwrap_or(cr.id.as_str()),
            )
        })
        .collect();

    // Collect all (cr_id, deployable_id, start, end) windows.
    struct Window<'a> {
        cr_id: &'a str,
        deployable_id: &'a str,
        deployable_name: &'a str,
        start: String,
        end: String,
    }
    let mut windows: Vec<Window> = Vec::new();
    for (cr_id, plan) in plan_for_cr.iter() {
        for s in &plan.steps {
            if let (Some(ws), Some(we)) = (s.window_start.as_deref(), s.window_end.as_deref()) {
                windows.push(Window {
                    cr_id,
                    deployable_id: s.deployable_id.as_str(),
                    deployable_name: s.deployable_name.as_str(),
                    start: ws.to_string(),
                    end: we.to_string(),
                });
            }
        }
    }

    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for i in 0..windows.len() {
        for j in (i + 1)..windows.len() {
            let a = &windows[i];
            let b = &windows[j];
            if a.cr_id == b.cr_id {
                continue;
            }
            if a.deployable_id != b.deployable_id {
                continue;
            }
            // Overlap if a.start < b.end AND b.start < a.end (string lex
            // works for ISO-8601).
            if a.start < b.end && b.start < a.end {
                let (lo, hi) = if a.cr_id < b.cr_id {
                    (a.cr_id, b.cr_id)
                } else {
                    (b.cr_id, a.cr_id)
                };
                let key = (lo.to_string(), hi.to_string());
                if !seen.insert(key.clone()) {
                    continue;
                }
                let lo_summary = cr_summary.get(lo).copied().unwrap_or(lo);
                let hi_summary = cr_summary.get(hi).copied().unwrap_or(hi);
                out.push(DerivedAdvisory {
                    kind: "ScheduleContention",
                    subject_type: "change_request",
                    subject_id: lo.to_string(),
                    subject_name: lo_summary.to_string(),
                    severity: Severity::Warn,
                    rule: RULE,
                    explain: format!(
                        "{lo_summary} and {hi_summary} both claim {} during overlapping windows.",
                        a.deployable_name
                    ),
                    caused_by: vec![lo.to_string(), hi.to_string(), a.deployable_id.to_string()],
                });
            }
        }
    }
    out
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(id: &str, name: &str, team: Option<&str>) -> Deployable {
        Deployable {
            id: id.into(),
            name: name.into(),
            description: None,
            team_id: team.map(String::from),
            deployment_status: None,
        }
    }

    fn edge(id: &str, from: &str, to_svc: &str, crit: Option<&str>) -> Dependency {
        Dependency {
            id: id.into(),
            deployable_id: from.into(),
            service_id: to_svc.into(),
            criticality: crit.map(String::from),
        }
    }

    fn exposes(id: &str, dep: &str, svc: &str) -> Exposes {
        Exposes {
            id: id.into(),
            deployable_id: dep.into(),
            service_id: svc.into(),
        }
    }

    #[test]
    fn blocked_upstream_emits_one_per_dependent() {
        let mut snap = GraphSnapshot::default();
        snap.deployables = vec![
            dep("auth", "Auth", None),
            dep("invoice", "Invoice", None),
            dep("oms", "OMS", None),
        ];
        snap.exposes = vec![exposes("e1", "auth", "auth-svc")];
        snap.dependencies = vec![
            edge("d1", "invoice", "auth-svc", Some("high")),
            edge("d2", "oms", "auth-svc", Some("medium")),
        ];
        snap.change_requests = vec![ChangeRequest {
            id: "cr1".into(),
            summary: Some("Auth MFA rollout".into()),
            status: Some("blocked".into()),
            tier: None,
            target_deployables: Some("auth".into()),
        }];
        let advisories = blocked_upstream_v1(&snap);
        assert_eq!(advisories.len(), 2);
        assert!(advisories
            .iter()
            .any(|a| a.subject_id == "invoice" && a.severity == Severity::Critical));
        assert!(advisories
            .iter()
            .any(|a| a.subject_id == "oms" && a.severity == Severity::Warn));
        assert!(advisories[0].explain.contains("blocked CR"));
    }

    #[test]
    fn circular_dependency_detects_two_cycle() {
        let mut snap = GraphSnapshot::default();
        snap.deployables = vec![dep("a", "A", None), dep("b", "B", None)];
        snap.exposes = vec![exposes("e1", "a", "a-svc"), exposes("e2", "b", "b-svc")];
        snap.dependencies = vec![
            edge("d1", "a", "b-svc", None),
            edge("d2", "b", "a-svc", None),
        ];
        let advisories = circular_dependency_v1(&snap);
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].kind, "CircularDependency");
        assert!(advisories[0].caused_by.contains(&"a".to_string()));
        assert!(advisories[0].caused_by.contains(&"b".to_string()));
    }

    #[test]
    fn circular_dependency_ignores_non_cycle_chain() {
        let mut snap = GraphSnapshot::default();
        snap.deployables = vec![
            dep("a", "A", None),
            dep("b", "B", None),
            dep("c", "C", None),
        ];
        snap.exposes = vec![
            exposes("e1", "a", "a-svc"),
            exposes("e2", "b", "b-svc"),
            exposes("e3", "c", "c-svc"),
        ];
        snap.dependencies = vec![
            edge("d1", "a", "b-svc", None),
            edge("d2", "b", "c-svc", None),
        ];
        assert!(circular_dependency_v1(&snap).is_empty());
    }

    #[test]
    fn undocumented_interface_only_fires_when_dependents_exist_and_no_contract() {
        let mut snap = GraphSnapshot::default();
        snap.services = vec![
            Service {
                id: "svc1".into(),
                name: "Public API".into(),
                kind: Some("rest".into()),
                description: None,
            },
            Service {
                id: "svc2".into(),
                name: "Lonely API".into(),
                kind: Some("rest".into()),
                description: None,
            },
        ];
        snap.dependencies = vec![edge("d1", "x", "svc1", None), edge("d2", "y", "svc1", None)];
        snap.contracts = vec![]; // svc1 has dependents and no contract → fires
        let advisories = undocumented_interface_v1(&snap);
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].subject_id, "svc1");
    }

    #[test]
    fn watershed_mismatch_fires_when_prod_like_lacks_sync() {
        let mut snap = GraphSnapshot::default();
        snap.test_environments = vec![
            TestEnvironment {
                id: "env1".into(),
                name: "OMS UAT".into(),
                kind: Some("multi-tenant".into()),
                deployable_id: None,
                watershed: Some("prod-like".into()),
            },
            TestEnvironment {
                id: "env2".into(),
                name: "OMS Sandbox".into(),
                kind: Some("sandbox".into()),
                deployable_id: None,
                watershed: Some("path-to-prod".into()),
            },
        ];
        // No syncs.
        let advisories = watershed_mismatch_v1(&snap);
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].subject_id, "env1");
    }

    #[test]
    fn missing_environment_fires_per_cr_targeting_unknown_deployable() {
        let mut snap = GraphSnapshot::default();
        snap.deployables = vec![dep("d1", "X", None), dep("d2", "Y", None)];
        snap.test_environments = vec![TestEnvironment {
            id: "e1".into(),
            name: "X sandbox".into(),
            kind: Some("sandbox".into()),
            deployable_id: Some("d1".into()),
            watershed: None,
        }];
        snap.change_requests = vec![ChangeRequest {
            id: "cr1".into(),
            summary: Some("Ship Y".into()),
            status: Some("approved".into()),
            tier: Some("staging".into()),
            target_deployables: Some("d2".into()),
        }];
        let advisories = missing_environment_v1(&snap);
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].subject_id, "cr1");
        assert!(advisories[0].explain.contains("Y"));
    }

    #[test]
    fn schedule_contention_pairs_overlapping_cr_windows_on_same_deployable() {
        let mut snap = GraphSnapshot::default();
        snap.change_requests = vec![
            ChangeRequest {
                id: "cr1".into(),
                summary: Some("Release v1".into()),
                ..Default::default()
            },
            ChangeRequest {
                id: "cr2".into(),
                summary: Some("Hotfix v2".into()),
                ..Default::default()
            },
        ];
        snap.deployment_plans = vec![
            DeploymentPlan {
                id: "p1".into(),
                change_request_id: "cr1".into(),
                steps: vec![PlanStepLite {
                    deployable_id: "d1".into(),
                    deployable_name: "OMS".into(),
                    order: 0,
                    window_start: Some("2026-05-13T10:00:00Z".into()),
                    window_end: Some("2026-05-13T10:30:00Z".into()),
                    test_environment_id: None,
                }],
            },
            DeploymentPlan {
                id: "p2".into(),
                change_request_id: "cr2".into(),
                steps: vec![PlanStepLite {
                    deployable_id: "d1".into(),
                    deployable_name: "OMS".into(),
                    order: 0,
                    window_start: Some("2026-05-13T10:15:00Z".into()),
                    window_end: Some("2026-05-13T10:45:00Z".into()),
                    test_environment_id: None,
                }],
            },
        ];
        let advisories = schedule_contention_v1(&snap);
        assert_eq!(advisories.len(), 1);
        assert_eq!(advisories[0].kind, "ScheduleContention");
        // Subject is the lexicographically smaller CR id.
        assert_eq!(advisories[0].subject_id, "cr1");
        assert!(advisories[0].caused_by.contains(&"cr2".to_string()));
    }
}
