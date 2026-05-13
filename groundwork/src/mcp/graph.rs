//! In-memory snapshot of the Groundwork catalogue plus the three graph queries
//! the MCP server exposes: `blast_radius`, `dependencies_of`, `deployment_plan`.
//!
//! A snapshot is rebuilt per tool call (small data, simple semantics). If
//! profiling later shows it's a hot path we can wrap it in a TTL cache.

use anyhow::Context;
use meshql_mcp::MeshqlClient as GroundworkClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

const DEPTH_HARD_CAP: usize = 10;

#[derive(Debug, Clone)]
pub struct Deployable {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Service {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Exposes {
    pub deployable_id: String,
    pub service_id: String,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub deployable_id: String,
    pub service_id: String,
}

#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    pub deployables_by_id: HashMap<String, Deployable>,
    pub services_by_id: HashMap<String, Service>,
    pub exposes: Vec<Exposes>,
    pub dependencies: Vec<Dependency>,
    /// service_id → deployables that expose that service
    pub deployables_for_service: HashMap<String, Vec<String>>,
    /// service_id → deployables that depend on that service
    pub dependents_of_service: HashMap<String, Vec<String>>,
    /// deployable_id → services it consumes
    pub services_for_deployable: HashMap<String, Vec<String>>,
}

impl Snapshot {
    /// Pull the full catalogue in parallel via GraphQL `getAll` queries and
    /// build the indices. Each query selects only the fields the snapshot
    /// needs (`id` + `name` for deployable/service, `id` + the two FK ids
    /// for exposes/dependency); the responses are flat GraphQL rows under
    /// `data.getAll`, not REST envelopes.
    pub async fn build(client: &GroundworkClient) -> anyhow::Result<Self> {
        let (deps_v, svcs_v, exp_v, dep_edges_v) = tokio::try_join!(
            client.gql("/deployable/graph", "{ getAll { id name } }"),
            client.gql("/service/graph", "{ getAll { id name } }"),
            client.gql(
                "/exposes/graph",
                "{ getAll { id deployable_id service_id } }"
            ),
            client.gql(
                "/dependency/graph",
                "{ getAll { id deployable_id service_id } }"
            ),
        )
        .context("snapshot fetch")?;

        let mut snap = Snapshot::default();

        for row in get_all_rows(&deps_v) {
            let id = string_field(row, "id");
            let name = string_field(row, "name");
            if !id.is_empty() {
                snap.deployables_by_id
                    .insert(id.clone(), Deployable { id, name });
            }
        }
        for row in get_all_rows(&svcs_v) {
            let id = string_field(row, "id");
            let name = string_field(row, "name");
            if !id.is_empty() {
                snap.services_by_id.insert(id.clone(), Service { id, name });
            }
        }
        for row in get_all_rows(&exp_v) {
            let d = string_field(row, "deployable_id");
            let s = string_field(row, "service_id");
            if d.is_empty() || s.is_empty() {
                continue;
            }
            snap.exposes.push(Exposes {
                deployable_id: d.clone(),
                service_id: s.clone(),
            });
            snap.deployables_for_service.entry(s).or_default().push(d);
        }
        for row in get_all_rows(&dep_edges_v) {
            let d = string_field(row, "deployable_id");
            let s = string_field(row, "service_id");
            if d.is_empty() || s.is_empty() {
                continue;
            }
            snap.dependencies.push(Dependency {
                deployable_id: d.clone(),
                service_id: s.clone(),
            });
            snap.dependents_of_service
                .entry(s.clone())
                .or_default()
                .push(d.clone());
            snap.services_for_deployable.entry(d).or_default().push(s);
        }

        Ok(snap)
    }

    /// "If this service goes down, what breaks?" — walks reverse-dependency
    /// edges from `service_id`. At each step the dependents are deployables
    /// that consume the current service; recursion follows their *exposed*
    /// services outward.
    pub fn blast_radius(&self, service_id: &str, depth: usize) -> Value {
        let depth = depth.clamp(1, DEPTH_HARD_CAP);
        let svc = self.services_by_id.get(service_id);
        let svc_name = svc.map(|s| s.name.clone()).unwrap_or_default();

        let mut visited_services: HashSet<String> = HashSet::new();
        let mut visited_deployables: HashSet<String> = HashSet::new();
        visited_services.insert(service_id.to_string());

        let mut direct: Vec<Value> = Vec::new();
        let mut transitive: Vec<Value> = Vec::new();
        let mut queue: VecDeque<(String, usize, bool)> = VecDeque::new();
        queue.push_back((service_id.to_string(), 0, true));

        while let Some((svc_id, level, is_root)) = queue.pop_front() {
            if level >= depth {
                continue;
            }
            let dependents = self
                .dependents_of_service
                .get(&svc_id)
                .cloned()
                .unwrap_or_default();
            for dep_id in dependents {
                if !visited_deployables.insert(dep_id.clone()) {
                    continue;
                }
                let dep = self.deployables_by_id.get(&dep_id);
                let exposed = self.deployable_exposes(&dep_id);
                let entry = json!({
                    "deployable_id": dep_id,
                    "deployable_name": dep.map(|d| d.name.clone()).unwrap_or_default(),
                    "via_service_id": svc_id,
                    "via_service_name": self.service_name(&svc_id),
                    "depth": level + 1,
                    "exposes": exposed.iter().map(|sid| json!({
                        "service_id": sid,
                        "service_name": self.service_name(sid),
                    })).collect::<Vec<_>>(),
                });
                if is_root {
                    direct.push(entry);
                } else {
                    transitive.push(entry);
                }
                for s in exposed {
                    if visited_services.insert(s.clone()) {
                        queue.push_back((s, level + 1, false));
                    }
                }
            }
        }

        json!({
            "service_id": service_id,
            "service_name": svc_name,
            "depth": depth,
            "direct_dependents": direct,
            "transitive_dependents": transitive,
        })
    }

    /// "What does this deployable consume?" — walks forward through dependency
    /// edges. Each edge resolves the depended-on service to its publishing
    /// deployable (if any) and recurses on that deployable's dependencies.
    pub fn dependencies_of(&self, deployable_id: &str, depth: usize) -> Value {
        let depth = depth.clamp(1, DEPTH_HARD_CAP);
        let dep = self.deployables_by_id.get(deployable_id);
        let dep_name = dep.map(|d| d.name.clone()).unwrap_or_default();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(deployable_id.to_string());
        let tree = self.dependencies_of_inner(deployable_id, depth, &mut visited);
        json!({
            "deployable_id": deployable_id,
            "deployable_name": dep_name,
            "depth": depth,
            "depends_on": tree,
        })
    }

    fn dependencies_of_inner(
        &self,
        deployable_id: &str,
        depth_remaining: usize,
        visited: &mut HashSet<String>,
    ) -> Vec<Value> {
        if depth_remaining == 0 {
            return Vec::new();
        }
        let services = self
            .services_for_deployable
            .get(deployable_id)
            .cloned()
            .unwrap_or_default();
        let mut out: Vec<Value> = Vec::new();
        for svc_id in services {
            let svc_name = self.service_name(&svc_id);
            let publishers = self
                .deployables_for_service
                .get(&svc_id)
                .cloned()
                .unwrap_or_default();
            if publishers.is_empty() {
                out.push(json!({
                    "service_id": svc_id,
                    "service_name": svc_name,
                    "external": true,
                }));
                continue;
            }
            for pub_dep_id in publishers {
                let pub_name = self
                    .deployables_by_id
                    .get(&pub_dep_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                let recurse = if visited.insert(pub_dep_id.clone()) {
                    self.dependencies_of_inner(&pub_dep_id, depth_remaining - 1, visited)
                } else {
                    Vec::new()
                };
                out.push(json!({
                    "service_id": svc_id,
                    "service_name": svc_name,
                    "deployable_id": pub_dep_id,
                    "deployable_name": pub_name,
                    "external": false,
                    "depends_on": recurse,
                }));
            }
        }
        out
    }

    /// Topological sort: collect every deployable transitively required by
    /// `deployable_id`, then order them dependencies-first via Kahn's algorithm.
    /// Services without a publishing deployable are surfaced as external
    /// prerequisites. Cycles return an `error` field.
    pub fn deployment_plan(&self, deployable_id: &str) -> Value {
        let dep_name = self
            .deployables_by_id
            .get(deployable_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();

        // 1. Walk forward, collecting deployables and external services.
        let mut required: BTreeSet<String> = BTreeSet::new();
        let mut externals: BTreeMap<String, String> = BTreeMap::new(); // svc_id → svc_name
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(deployable_id.to_string());
        required.insert(deployable_id.to_string());

        while let Some(d) = queue.pop_front() {
            let services = self
                .services_for_deployable
                .get(&d)
                .cloned()
                .unwrap_or_default();
            for svc_id in services {
                let publishers = self
                    .deployables_for_service
                    .get(&svc_id)
                    .cloned()
                    .unwrap_or_default();
                if publishers.is_empty() {
                    externals.insert(svc_id.clone(), self.service_name(&svc_id));
                    continue;
                }
                for p in publishers {
                    if required.insert(p.clone()) {
                        queue.push_back(p);
                    }
                }
            }
        }

        // 2. Build edges among the required set: edge from `consumer → producer`
        //    means consumer DEPENDS ON producer, so producer must come first.
        //    For Kahn's we need in-degree of each node = number of producers
        //    it consumes. Producers with in-degree 0 emit first.
        let mut in_degree: BTreeMap<String, usize> =
            required.iter().map(|d| (d.clone(), 0)).collect();
        let mut producers_to_consumers: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for d in &required {
            let services = self
                .services_for_deployable
                .get(d)
                .cloned()
                .unwrap_or_default();
            for svc_id in services {
                let publishers = self
                    .deployables_for_service
                    .get(&svc_id)
                    .cloned()
                    .unwrap_or_default();
                for p in publishers {
                    if !required.contains(&p) || p == *d {
                        continue;
                    }
                    *in_degree.entry(d.clone()).or_insert(0) += 1;
                    producers_to_consumers.entry(p).or_default().push(d.clone());
                }
            }
        }

        let mut ready: BTreeSet<String> = in_degree
            .iter()
            .filter(|(_, &v)| v == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let mut ordered: Vec<String> = Vec::new();
        while let Some(next) = ready.iter().next().cloned() {
            ready.remove(&next);
            ordered.push(next.clone());
            if let Some(consumers) = producers_to_consumers.get(&next) {
                for c in consumers {
                    if let Some(d) = in_degree.get_mut(c) {
                        *d = d.saturating_sub(1);
                        if *d == 0 {
                            ready.insert(c.clone());
                        }
                    }
                }
            }
        }

        if ordered.len() < required.len() {
            let leftover: Vec<String> = required
                .iter()
                .filter(|k| !ordered.contains(k))
                .cloned()
                .collect();
            return json!({
                "deployable_id": deployable_id,
                "deployable_name": dep_name,
                "error": "cycle detected",
                "cycle_members": leftover,
            });
        }

        let ordered_json: Vec<Value> = ordered
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let name = self
                    .deployables_by_id
                    .get(id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                json!({
                    "order": i,
                    "deployable_id": id,
                    "deployable_name": name,
                })
            })
            .collect();

        let externals_json: Vec<Value> = externals
            .into_iter()
            .map(|(id, name)| json!({"service_id": id, "service_name": name}))
            .collect();

        json!({
            "deployable_id": deployable_id,
            "deployable_name": dep_name,
            "external_prerequisites": externals_json,
            "ordered_deployments": ordered_json,
        })
    }

    fn deployable_exposes(&self, deployable_id: &str) -> Vec<String> {
        self.exposes
            .iter()
            .filter(|e| e.deployable_id == deployable_id)
            .map(|e| e.service_id.clone())
            .collect()
    }

    fn service_name(&self, service_id: &str) -> String {
        self.services_by_id
            .get(service_id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    /// Build a snapshot from raw vecs — handy in tests.
    pub fn synthetic(
        deployables: Vec<Deployable>,
        services: Vec<Service>,
        exposes: Vec<Exposes>,
        dependencies: Vec<Dependency>,
    ) -> Self {
        let mut snap = Snapshot::default();
        for d in deployables {
            snap.deployables_by_id.insert(d.id.clone(), d);
        }
        for s in services {
            snap.services_by_id.insert(s.id.clone(), s);
        }
        for e in &exposes {
            snap.deployables_for_service
                .entry(e.service_id.clone())
                .or_default()
                .push(e.deployable_id.clone());
        }
        for d in &dependencies {
            snap.dependents_of_service
                .entry(d.service_id.clone())
                .or_default()
                .push(d.deployable_id.clone());
            snap.services_for_deployable
                .entry(d.deployable_id.clone())
                .or_default()
                .push(d.service_id.clone());
        }
        snap.exposes = exposes;
        snap.dependencies = dependencies;
        snap
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlanInputs {
    pub deployable_id: String,
}

fn string_field(env: &Value, key: &str) -> String {
    env.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Extract the rows under `data.getAll` from a `MeshqlClient::gql` response.
/// `gql` already unwrapped the outer `{ data: ... }`, so we just look up
/// the `getAll` key on the returned JSON value. Empty / missing → no rows.
fn get_all_rows(data: &Value) -> Vec<&Value> {
    data.get("getAll")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Snapshot {
        Snapshot::synthetic(
            vec![
                Deployable {
                    id: "d-checkout".into(),
                    name: "checkout".into(),
                },
                Deployable {
                    id: "d-auth".into(),
                    name: "auth".into(),
                },
            ],
            vec![
                Service {
                    id: "s-auth-api".into(),
                    name: "auth-api".into(),
                },
                Service {
                    id: "s-stripe".into(),
                    name: "stripe".into(),
                },
            ],
            vec![Exposes {
                deployable_id: "d-auth".into(),
                service_id: "s-auth-api".into(),
            }],
            vec![
                Dependency {
                    deployable_id: "d-checkout".into(),
                    service_id: "s-auth-api".into(),
                },
                Dependency {
                    deployable_id: "d-checkout".into(),
                    service_id: "s-stripe".into(),
                },
            ],
        )
    }

    #[test]
    fn deployment_plan_orders_deps_first() {
        let snap = fixture();
        let plan = snap.deployment_plan("d-checkout");
        let ord = plan
            .get("ordered_deployments")
            .and_then(|v| v.as_array())
            .unwrap();
        let names: Vec<&str> = ord
            .iter()
            .filter_map(|s| s.get("deployable_name").and_then(|n| n.as_str()))
            .collect();
        assert_eq!(names, vec!["auth", "checkout"]);
    }

    #[test]
    fn deployment_plan_surfaces_external_prerequisites() {
        let snap = fixture();
        let plan = snap.deployment_plan("d-checkout");
        let ext = plan
            .get("external_prerequisites")
            .and_then(|v| v.as_array())
            .unwrap();
        let names: Vec<&str> = ext
            .iter()
            .filter_map(|s| s.get("service_name").and_then(|n| n.as_str()))
            .collect();
        assert_eq!(names, vec!["stripe"]);
    }

    #[test]
    fn blast_radius_finds_dependents() {
        let snap = fixture();
        let blast = snap.blast_radius("s-auth-api", 5);
        let direct = blast
            .get("direct_dependents")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(direct.len(), 1);
        assert_eq!(
            direct[0].get("deployable_name").and_then(|v| v.as_str()),
            Some("checkout")
        );
    }

    #[test]
    fn dependencies_of_walks_forward() {
        let snap = fixture();
        let tree = snap.dependencies_of("d-checkout", 5);
        let arr = tree.get("depends_on").and_then(|v| v.as_array()).unwrap();
        // checkout -> auth-api (resolves to auth) and stripe (external)
        assert_eq!(arr.len(), 2);
        let ext_count = arr
            .iter()
            .filter(|n| n.get("external").and_then(|b| b.as_bool()) == Some(true))
            .count();
        assert_eq!(ext_count, 1);
    }
}
