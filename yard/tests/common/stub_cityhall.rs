//! Stub Cityhall server. Yard hits both:
//!   - GET /change_request/api/<id>            (HTTP cityhall client)
//!   - POST /change_request/graph              (federated GraphQL resolver)

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

#[derive(Clone, Debug)]
pub struct StubChangeRequest {
    pub id: String,
    pub summary: String,
    pub status: Option<String>,
    pub tier: Option<String>,
    pub target_deployables: Vec<String>,
}

#[derive(Clone, Default, Debug)]
pub struct ChangeRequestRegistry {
    inner: Arc<Mutex<HashMap<String, StubChangeRequest>>>,
}

impl ChangeRequestRegistry {
    pub fn new() -> Self { Self::default() }
    pub fn insert(&self, cr: StubChangeRequest) {
        self.inner.lock().unwrap().insert(cr.id.clone(), cr);
    }
    pub fn get(&self, id: &str) -> Option<StubChangeRequest> {
        self.inner.lock().unwrap().get(id).cloned()
    }
    pub fn clear(&self) { self.inner.lock().unwrap().clear(); }
}

async fn get_change_request(
    State(reg): State<ChangeRequestRegistry>,
    Path(id): Path<String>,
) -> Response {
    let Some(cr) = reg.get(&id) else {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    };
    let target_str = serde_json::to_string(&cr.target_deployables).unwrap_or_default();
    let env = json!({
        "id": cr.id,
        "payload": {
            "summary": cr.summary,
            "status": cr.status,
            "tier": cr.tier,
            "target_deployables": target_str,
        }
    });
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&env).unwrap_or_default(),
    )
        .into_response()
}

async fn change_request_graph(
    State(reg): State<ChangeRequestRegistry>,
    Json(body): Json<Value>,
) -> Response {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let id = extract_string_arg(query, "id").unwrap_or_default();
    let payload = match reg.get(&id) {
        Some(cr) => json!({
            "id": cr.id,
            "summary": cr.summary,
            "status": cr.status,
            "tier": cr.tier,
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

pub async fn spawn() -> (String, ChangeRequestRegistry) {
    let registry = ChangeRequestRegistry::new();
    let app = Router::new()
        .route("/change_request/api/:id", get(get_change_request))
        .route("/change_request/graph", post(change_request_graph))
        .with_state(registry.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{}", addr.port()), registry)
}
