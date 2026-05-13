//! Stub Union server. Yard hits POST /team/graph for the federated
//! `TestRun.team` resolver.

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
pub struct StubTeam {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
}

#[derive(Clone, Default, Debug)]
pub struct TeamRegistry {
    inner: Arc<Mutex<HashMap<String, StubTeam>>>,
}

impl TeamRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&self, team: StubTeam) {
        self.inner.lock().unwrap().insert(team.id.clone(), team);
    }
    pub fn get(&self, id: &str) -> Option<StubTeam> {
        self.inner.lock().unwrap().get(id).cloned()
    }
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

async fn team_graph(State(reg): State<TeamRegistry>, Json(body): Json<Value>) -> Response {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let id = extract_string_arg(query, "id").unwrap_or_default();
    let payload = match reg.get(&id) {
        Some(t) => json!({
            "id": t.id,
            "name": t.name,
            "kind": t.kind,
            "description": t.description,
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

pub async fn spawn() -> (String, TeamRegistry) {
    let registry = TeamRegistry::new();
    let app = Router::new()
        .route("/team/graph", post(team_graph))
        .with_state(registry.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{}", addr.port()), registry)
}
