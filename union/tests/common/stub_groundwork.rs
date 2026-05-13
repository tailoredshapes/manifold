//! Stub Groundwork HTTP server used by Union's federation tests for the
//! `WorkOrder.deployable` resolver.

use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct StubDeployable {
    pub id: String,
    pub name: String,
    pub team_id: Option<String>,
    pub repo_url: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Default, Debug)]
pub struct DeployableRegistry {
    inner: Arc<Mutex<HashMap<String, StubDeployable>>>,
}

impl DeployableRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&self, dep: StubDeployable) {
        self.inner.lock().unwrap().insert(dep.id.clone(), dep);
    }
    pub fn forget(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }
    pub fn get(&self, id: &str) -> Option<StubDeployable> {
        self.inner.lock().unwrap().get(id).cloned()
    }
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

async fn handle_graph(State(reg): State<DeployableRegistry>, Json(body): Json<Value>) -> Response {
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

    let resp = json!({ "data": { "getById": payload }, "errors": null });
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&resp).unwrap_or_default(),
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
        .route("/deployable/graph", post(handle_graph))
        .with_state(registry.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{}", addr.port()), registry)
}
