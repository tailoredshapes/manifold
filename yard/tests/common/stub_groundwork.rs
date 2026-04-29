//! Stub Groundwork server. Serves both:
//!   - GET /deployable/api/<id>            (used by yard's HTTP groundwork client)
//!   - POST /deployable/graph              (used by federated GraphQL resolvers)
//!   - POST /service/graph                 (federated service resolver)
//!   - POST /dependency/graph              (used to walk dep edges)
//!   - POST /exposes/graph                 (used to find publishers for services)

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct StubDeployable {
    pub id: String,
    pub name: String,
    pub team_id: Option<String>,
    pub repo_url: Option<String>,
    pub description: Option<String>,
    /// service IDs this deployable consumes
    pub depends_on_service_ids: Vec<String>,
    /// service IDs this deployable exposes
    pub exposes_service_ids: Vec<String>,
}

/// Service-id → publisher-deployable-id. Used by /exposes/graph stub.
#[derive(Clone, Default, Debug)]
pub struct DeployableRegistry {
    inner: Arc<Mutex<HashMap<String, StubDeployable>>>,
}

impl DeployableRegistry {
    pub fn new() -> Self { Self::default() }
    pub fn insert(&self, dep: StubDeployable) {
        self.inner.lock().unwrap().insert(dep.id.clone(), dep);
    }
    pub fn get(&self, id: &str) -> Option<StubDeployable> {
        self.inner.lock().unwrap().get(id).cloned()
    }
    pub fn clear(&self) { self.inner.lock().unwrap().clear(); }

    /// Convenience: register `id`+`name` plus a list of upstream deployable IDs.
    /// Internally we synthesise a per-upstream service ID `svc-<dep>` so the
    /// dependency-walk can resolve dependents back to upstream deployables.
    /// Merges with any existing record so multiple registrations layer cleanly.
    pub fn register_with_deps(&self, id: &str, name: &str, depends_on: &[&str]) {
        let mut consumed: Vec<String> = Vec::new();
        for dep in depends_on {
            let svc_id = format!("svc-{dep}");
            consumed.push(svc_id.clone());
            let mut inner = self.inner.lock().unwrap();
            inner
                .entry(dep.to_string())
                .and_modify(|d| {
                    if !d.exposes_service_ids.contains(&svc_id) {
                        d.exposes_service_ids.push(svc_id.clone());
                    }
                })
                .or_insert_with(|| StubDeployable {
                    id: dep.to_string(),
                    name: dep.to_string(),
                    exposes_service_ids: vec![svc_id.clone()],
                    ..Default::default()
                });
        }
        let mut inner = self.inner.lock().unwrap();
        inner
            .entry(id.to_string())
            .and_modify(|d| {
                d.name = name.to_string();
                for s in &consumed {
                    if !d.depends_on_service_ids.contains(s) {
                        d.depends_on_service_ids.push(s.clone());
                    }
                }
            })
            .or_insert_with(|| StubDeployable {
                id: id.to_string(),
                name: name.to_string(),
                depends_on_service_ids: consumed.clone(),
                ..Default::default()
            });
    }
}

async fn get_deployable(
    State(reg): State<DeployableRegistry>,
    Path(id): Path<String>,
) -> Response {
    let Some(dep) = reg.get(&id) else {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    };
    let env = json!({
        "id": dep.id,
        "payload": {
            "name": dep.name,
            "team_id": dep.team_id,
            "repo_url": dep.repo_url,
            "description": dep.description,
        }
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&env).unwrap_or_default(),
    )
        .into_response()
}

async fn deployable_graph(
    State(reg): State<DeployableRegistry>,
    Json(body): Json<Value>,
) -> Response {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let id = extract_string_arg(query, "id").unwrap_or_default();
    let payload = match reg.get(&id) {
        Some(dep) => json!({
            "id": dep.id,
            "name": dep.name,
            "team_id": dep.team_id,
            "repo_url": dep.repo_url,
            "description": dep.description,
        }),
        None => Value::Null,
    };
    json_resp(json!({ "data": { "getById": payload }, "errors": null }))
}

async fn service_graph(Json(body): Json<Value>) -> Response {
    // Just echo a synthetic service envelope keyed off the requested id so
    // federated resolvers don't error out.
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let id = extract_string_arg(query, "id").unwrap_or_default();
    let payload = if id.is_empty() {
        Value::Null
    } else {
        json!({
            "id": id,
            "name": id,
            "type": "rest",
            "description": "stub service",
            "endpoint": "stub",
        })
    };
    json_resp(json!({ "data": { "getById": payload }, "errors": null }))
}

async fn dependency_graph(
    State(reg): State<DeployableRegistry>,
    Json(body): Json<Value>,
) -> Response {
    // Yard's groundwork client calls getByDeployableId(deployable_id: "<id>")
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let dep_id = extract_string_arg(query, "deployable_id").unwrap_or_default();
    let arr: Vec<Value> = reg
        .get(&dep_id)
        .map(|d| {
            d.depends_on_service_ids
                .into_iter()
                .map(|sid| json!({ "service_id": sid }))
                .collect()
        })
        .unwrap_or_default();
    json_resp(json!({ "data": { "getByDeployableId": arr }, "errors": null }))
}

async fn exposes_graph(
    State(reg): State<DeployableRegistry>,
    Json(body): Json<Value>,
) -> Response {
    // getByServiceId(service_id: "<id>") → [{ deployable_id }]
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let svc_id = extract_string_arg(query, "service_id").unwrap_or_default();
    let inner = reg.inner.lock().unwrap();
    let arr: Vec<Value> = inner
        .values()
        .filter(|d| d.exposes_service_ids.iter().any(|s| s == &svc_id))
        .map(|d| json!({ "deployable_id": d.id }))
        .collect();
    json_resp(json!({ "data": { "getByServiceId": arr }, "errors": null }))
}

fn json_resp(v: Value) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&v).unwrap_or_default(),
    )
        .into_response()
}

fn extract_string_arg(query: &str, arg_name: &str) -> Option<String> {
    let needle = format!("{arg_name}:");
    let start = query.find(&needle)?;
    let after = &query[start + needle.len()..];
    let q1 = after.find('"')?;
    let after_q1 = &after[q1 + 1..];
    let q2 = after_q1.find('"')?;
    Some(after_q1[..q2].to_string())
}

pub async fn spawn() -> (String, DeployableRegistry) {
    let registry = DeployableRegistry::new();
    let app = Router::new()
        .route("/deployable/api/:id", get(get_deployable))
        .route("/deployable/graph", post(deployable_graph))
        .route("/service/graph", post(service_graph))
        .route("/dependency/graph", post(dependency_graph))
        .route("/exposes/graph", post(exposes_graph))
        .with_state(registry.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{}", addr.port()), registry)
}
