//! BDD harness for the `cityhall-mcp` binary.
//!
//! Spawns the binary as a subprocess (built via `CARGO_BIN_EXE_cityhall-mcp`)
//! and exchanges line-delimited JSON-RPC frames over its stdin/stdout. The
//! `CITYHALL_URL` env var points the MCP child at an in-process Cityhall
//! HTTP server (REST + GraphQL) bound to a random port. The in-process
//! server registers REST routes plus the four custom endpoints
//! (`/org_node/:id/ancestors`, `/org_node/:id/effective_bylaws`,
//! `/change_request/:id/plan`, `/deployment_plan/:id/gantt`) that the MCP
//! custom capabilities wrap, plus graphlettes for the five entities so the
//! auto-derived catalog capabilities can resolve via /graph.

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use cucumber::{given, then, when, World};
use meshql_core::{GraphletteConfig, NoAuth, Repository, RootConfig, ServerConfig, Stash};
use meshql_server::{ValidatorContext, ValidatorFn};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use reqwest::Client;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const ORG_NODE_GRAPHQL: &str = include_str!("../config/graph/org_node.graphql");
const BYLAW_GRAPHQL: &str = include_str!("../config/graph/bylaw.graphql");
const CHANGE_REQUEST_GRAPHQL: &str = include_str!("../config/graph/change_request.graphql");
const DEPLOYMENT_PLAN_GRAPHQL: &str = include_str!("../config/graph/deployment_plan.graphql");
const GANTT_OUTPUT_GRAPHQL: &str = include_str!("../config/graph/gantt_output.graphql");

// ── Stub Groundwork ──────────────────────────────────────────────────────────

#[derive(Default)]
struct StubGroundwork {
    deployables: std::sync::Mutex<HashMap<String, StubDeployable>>,
}

#[derive(Clone)]
struct StubDeployable {
    name: String,
    team_id: Option<String>,
    depends_on: Vec<String>,
}

impl StubGroundwork {
    fn put(&self, id: &str, name: &str, team_id: Option<&str>, depends_on: Vec<String>) {
        self.deployables.lock().unwrap().insert(
            id.to_string(),
            StubDeployable {
                name: name.to_string(),
                team_id: team_id.map(String::from),
                depends_on,
            },
        );
    }
}

#[async_trait::async_trait]
impl cityhall::plan::GroundworkLookup for StubGroundwork {
    async fn get_deployable(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<cityhall::plan::DeployableSummary>> {
        Ok(self
            .deployables
            .lock()
            .unwrap()
            .get(id)
            .map(|d| cityhall::plan::DeployableSummary {
                id: id.to_string(),
                name: d.name.clone(),
                team_id: d.team_id.clone(),
                depends_on: d.depends_on.clone(),
            }))
    }
}

// ── Storage ──────────────────────────────────────────────────────────────────

async fn make_pool() -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::from_str("sqlite::memory:")
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap()
}

#[derive(Clone)]
struct TestEntity {
    repo: Arc<dyn Repository>,
    searcher: Arc<dyn meshql_core::Searcher>,
}

async fn make_entity() -> TestEntity {
    let pool = make_pool().await;
    let repo = Arc::new(SqliteRepository::new_with_pool(pool.clone()).await.unwrap());
    let searcher: Arc<dyn meshql_core::Searcher> =
        Arc::new(SqliteSearcher::new_with_pool(pool).await.unwrap());
    TestEntity { repo, searcher }
}

fn validator_for(schema_str: &str) -> ValidatorFn {
    let schema: Value = serde_json::from_str(schema_str).expect("invalid schema");
    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let enums: BTreeMap<String, Vec<String>> = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| {
            props
                .iter()
                .filter_map(|(k, v)| {
                    v.get("enum").and_then(|e| e.as_array()).map(|arr| {
                        (
                            k.clone(),
                            arr.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect(),
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Arc::new(move |payload: &Stash, _ctx: &ValidatorContext| {
        for field in &required {
            match payload.get(field.as_str()) {
                None => return Err(format!("Required field '{}' is missing", field)),
                Some(v) if v.as_str().map(|s| s.trim().is_empty()).unwrap_or(false) => {
                    return Err(format!("Required field '{}' cannot be empty", field))
                }
                _ => {}
            }
        }
        for (field, allowed) in &enums {
            if let Some(v) = payload.get(field.as_str()).and_then(|x| x.as_str()) {
                if !allowed.iter().any(|a| a == v) {
                    return Err(format!(
                        "Field '{}' must be one of {:?}, got {:?}",
                        field, allowed, v
                    ));
                }
            }
        }
        Ok(())
    })
}

#[derive(Clone)]
struct AppState {
    org_node: TestEntity,
    bylaw: TestEntity,
    change_request: TestEntity,
    deployment_plan: TestEntity,
    gantt_output: TestEntity,
    groundwork: Arc<StubGroundwork>,
}

// ── Custom-route handlers (mirror cityhall::main / cityhall_cert) ────────────

async fn get_ancestors(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match cityhall::bylaw::ancestors_of(&state.org_node.repo, &id).await {
        Ok(chain) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&chain).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("ancestors error: {e}"),
        )
            .into_response(),
    }
}

async fn get_effective_bylaws(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match cityhall::bylaw::effective_bylaws_for(&state.org_node.repo, &state.bylaw.repo, &id).await
    {
        Ok(list) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&list).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("effective_bylaws error: {e}"),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize, Default)]
struct PlanRequest {
    #[serde(default)]
    tier: Option<String>,
}

async fn post_change_request_plan(
    State(state): State<AppState>,
    Path(cr_id): Path<String>,
    Json(req): Json<PlanRequest>,
) -> Response {
    let env = match state.change_request.repo.read(&cr_id, &[], None).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("change_request {cr_id} not found"),
            )
                .into_response()
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("read change_request: {e}"),
            )
                .into_response()
        }
    };
    let summary = env
        .payload
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tier = req
        .tier
        .or_else(|| {
            env.payload
                .get("tier")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "dev".into());
    let target_str = env
        .payload
        .get("target_deployables")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let target_deployable_ids: Vec<String> = if target_str.trim().is_empty() {
        Vec::new()
    } else if let Ok(v) = serde_json::from_str::<Vec<String>>(target_str) {
        v
    } else {
        target_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let inputs = cityhall::plan::PlanInputs {
        change_request_id: cr_id.clone(),
        change_request_summary: summary.clone(),
        tier: tier.clone(),
        target_deployable_ids,
        org_node_repo: &state.org_node.repo,
        bylaw_repo: &state.bylaw.repo,
        groundwork: state.groundwork.as_ref(),
    };
    let computed = match cityhall::plan::compute_plan(inputs).await {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("compute_plan: {e}"),
            )
                .into_response()
        }
    };

    let mut payload = Stash::new();
    payload.insert(
        "change_request_id".into(),
        Value::String(computed.change_request_id.clone()),
    );
    payload.insert("tier".into(), Value::String(computed.tier.clone()));
    payload.insert(
        "steps".into(),
        Value::String(serde_json::to_string(&computed.steps).unwrap_or_default()),
    );
    payload.insert(
        "blockers".into(),
        Value::String(serde_json::to_string(&computed.blockers).unwrap_or_default()),
    );
    payload.insert(
        "computed_at".into(),
        Value::String(computed.computed_at.clone()),
    );
    payload.insert(
        "summary".into(),
        Value::String(computed.change_request_summary.clone()),
    );
    let envelope = meshql_core::Envelope::new(synthetic_id(&cr_id), payload, vec![]);

    match state.deployment_plan.repo.create(envelope, &[]).await {
        Ok(saved) => (
            axum::http::StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&saved).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("persist plan: {e}"),
        )
            .into_response(),
    }
}

async fn post_deployment_plan_gantt(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
) -> Response {
    let env = match state.deployment_plan.repo.read(&plan_id, &[], None).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("deployment_plan {plan_id} not found"),
            )
                .into_response()
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("read plan: {e}"),
            )
                .into_response()
        }
    };
    let steps_str = env
        .payload
        .get("steps")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");
    let steps: Vec<cityhall::plan::PlanStep> = serde_json::from_str(steps_str).unwrap_or_default();
    let blockers_str = env
        .payload
        .get("blockers")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");
    let blockers: Vec<cityhall::plan::Blocker> =
        serde_json::from_str(blockers_str).unwrap_or_default();
    let computed = cityhall::plan::ComputedPlan {
        change_request_id: env
            .payload
            .get("change_request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        change_request_summary: env
            .payload
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tier: env
            .payload
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        steps,
        blockers,
        computed_at: env
            .payload
            .get("computed_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    let mermaid = cityhall::gantt::render_gantt(&computed);

    let mut payload = Stash::new();
    payload.insert("deployment_plan_id".into(), Value::String(plan_id.clone()));
    payload.insert("tier".into(), Value::String(computed.tier.clone()));
    payload.insert("mermaid".into(), Value::String(mermaid));
    let envelope = meshql_core::Envelope::new(synthetic_id(&plan_id), payload, vec![]);
    match state.gantt_output.repo.create(envelope, &[]).await {
        Ok(saved) => (
            axum::http::StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&saved).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("persist gantt: {e}"),
        )
            .into_response(),
    }
}

fn synthetic_id(seed: &str) -> String {
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    format!("{}-{:x}", seed, nanos)
}

// ── In-process Cityhall REST server ──────────────────────────────────────────

async fn build_server() -> String {
    let stub = Arc::new(StubGroundwork::default());
    // The standard hierarchy below targets `dep-checkout` which depends on
    // `dep-auth`. Seed both so plan compilation has something to chew on.
    stub.put(
        "dep-checkout",
        "checkout",
        Some("team-checkout"),
        vec!["dep-auth".into()],
    );
    stub.put("dep-auth", "auth", Some("team-auth"), vec![]);

    let org_node = make_entity().await;
    let bylaw_e = make_entity().await;
    let change_request = make_entity().await;
    let deployment_plan = make_entity().await;
    let gantt_output = make_entity().await;

    let auth = Arc::new(NoAuth);

    let org_node_restlette = meshql_server::build_restlette_router_ext(
        "/org_node/api",
        org_node.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/org_node.schema.json"
        ))),
        None,
        None,
    );
    let bylaw_restlette = meshql_server::build_restlette_router_ext(
        "/bylaw/api",
        bylaw_e.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/bylaw.schema.json"
        ))),
        None,
        None,
    );
    let change_request_restlette = meshql_server::build_restlette_router_ext(
        "/change_request/api",
        change_request.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/change_request.schema.json"
        ))),
        None,
        None,
    );
    let deployment_plan_restlette = meshql_server::build_restlette_router_ext(
        "/deployment_plan/api",
        deployment_plan.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/deployment_plan.schema.json"
        ))),
        None,
        None,
    );
    let gantt_output_restlette = meshql_server::build_restlette_router_ext(
        "/gantt_output/api",
        gantt_output.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/gantt_output.schema.json"
        ))),
        None,
        None,
    );

    let app_state = AppState {
        org_node: org_node.clone(),
        bylaw: bylaw_e.clone(),
        change_request: change_request.clone(),
        deployment_plan: deployment_plan.clone(),
        gantt_output: gantt_output.clone(),
        groundwork: stub,
    };

    let custom_routes = Router::new()
        .route("/org_node/:id/ancestors", get(get_ancestors))
        .route("/org_node/:id/effective_bylaws", get(get_effective_bylaws))
        .route("/change_request/:id/plan", post(post_change_request_plan))
        .route(
            "/deployment_plan/:id/gantt",
            post(post_deployment_plan_gantt),
        )
        .with_state(app_state);

    let extra = Router::new()
        .merge(org_node_restlette)
        .merge(bylaw_restlette)
        .merge(change_request_restlette)
        .merge(deployment_plan_restlette)
        .merge(gantt_output_restlette)
        .merge(custom_routes);

    let org_node_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector("getByParentId", r#"{"payload.parent_id": "{{parent_id}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .build();
    let bylaw_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByOrgNodeId",
            r#"{"payload.org_node_id": "{{org_node_id}}"}"#,
        )
        .vector("getByGateType", r#"{"payload.gate_type": "{{gate_type}}"}"#)
        .build();
    let change_request_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByStatus", r#"{"payload.status": "{{status}}"}"#)
        .vector("getByTier", r#"{"payload.tier": "{{tier}}"}"#)
        .build();
    let deployment_plan_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByChangeRequestId",
            r#"{"payload.change_request_id": "{{change_request_id}}"}"#,
        )
        .build();
    let gantt_output_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByDeploymentPlanId",
            r#"{"payload.deployment_plan_id": "{{deployment_plan_id}}"}"#,
        )
        .build();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/org_node/graph".into(),
                schema_text: ORG_NODE_GRAPHQL.into(),
                root_config: org_node_gql,
                searcher: org_node.searcher.clone(),
            },
            GraphletteConfig {
                path: "/bylaw/graph".into(),
                schema_text: BYLAW_GRAPHQL.into(),
                root_config: bylaw_gql,
                searcher: bylaw_e.searcher.clone(),
            },
            GraphletteConfig {
                path: "/change_request/graph".into(),
                schema_text: CHANGE_REQUEST_GRAPHQL.into(),
                root_config: change_request_gql,
                searcher: change_request.searcher.clone(),
            },
            GraphletteConfig {
                path: "/deployment_plan/graph".into(),
                schema_text: DEPLOYMENT_PLAN_GRAPHQL.into(),
                root_config: deployment_plan_gql,
                searcher: deployment_plan.searcher.clone(),
            },
            GraphletteConfig {
                path: "/gantt_output/graph".into(),
                schema_text: GANTT_OUTPUT_GRAPHQL.into(),
                root_config: gantt_output_gql,
                searcher: gantt_output.searcher.clone(),
            },
        ],
        restlettes: vec![],
    };

    let app = meshql_server::build_app_ext(server_config, extra)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{}", addr.port())
}

// ── MCP subprocess client ────────────────────────────────────────────────────

struct McpClient {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient").finish()
    }
}

impl McpClient {
    async fn spawn(server_url: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cityhall-mcp"))
            .env("CITYHALL_URL", server_url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn cityhall-mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            _child: child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&frame).unwrap();
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        self.stdin.write_all(b"\n").await.unwrap();
        self.stdin.flush().await.unwrap();

        let mut buf = String::new();
        self.stdout.read_line(&mut buf).await.unwrap();
        serde_json::from_str(&buf)
            .unwrap_or_else(|e| panic!("MCP response was not JSON: {e}\nbody: {buf}"))
    }
}

// ── World ────────────────────────────────────────────────────────────────────

#[derive(Debug, World)]
#[world(init = Self::default_world)]
struct McpWorld {
    server_url: Option<String>,
    mcp: Option<McpClient>,
    ids: HashMap<String, String>,
    last_response: Option<Value>,
    last_tool_result: Option<Value>,
    http: Client,
}

impl std::default::Default for McpWorld {
    fn default() -> Self {
        Self {
            server_url: None,
            mcp: None,
            ids: HashMap::new(),
            last_response: None,
            last_tool_result: None,
            http: Client::new(),
        }
    }
}

impl McpWorld {
    fn default_world() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self, std::convert::Infallible>> + Send>,
    > {
        Box::pin(async { Ok(Self::default()) })
    }

    fn server_url(&self) -> &str {
        self.server_url.as_deref().expect("server not started")
    }

    fn mcp_mut(&mut self) -> &mut McpClient {
        self.mcp.as_mut().expect("MCP client not started")
    }

    fn resolve(&self, s: &str) -> String {
        let mut out = s.to_string();
        for (k, v) in &self.ids {
            out = out.replace(&format!("<ids.{k}>"), v);
        }
        out
    }

    fn structured_result(&self) -> &Value {
        self.last_tool_result
            .as_ref()
            .expect("no tool result captured")
    }

    fn structured_array(&self) -> Vec<Value> {
        let r = self.structured_result();
        for key in [
            "getAll",
            "getByName",
            "getByKind",
            "getByOrgNodeId",
            "getByGateType",
            "getByStatus",
            "getByTier",
        ] {
            if let Some(arr) = r.get(key).and_then(|v| v.as_array()) {
                return arr.clone();
            }
        }
        if let Some(arr) = r.as_array() {
            return arr.clone();
        }
        if let Some(arr) = r.get("result").and_then(|v| v.as_array()) {
            return arr.clone();
        }
        panic!("expected array (raw or GraphQL-wrapped under getAll/etc), got: {r}");
    }
}

// ── HTTP helper ──────────────────────────────────────────────────────────────

async fn post_for_id(world: &mut McpWorld, path: &str, body: Value) -> String {
    let url = format!("{}{}", world.server_url(), path);
    let resp = world.http.post(&url).json(&body).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    assert!(
        status == 200 || status == 201,
        "POST {path} failed ({status}): {text}\nbody={body}"
    );
    let parsed: Value = serde_json::from_str(&text).expect("response not JSON");
    parsed
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .expect("no id in response")
}

// ── Step definitions ─────────────────────────────────────────────────────────

#[given("a Cityhall server is running")]
async fn start_server(world: &mut McpWorld) {
    world.server_url = Some(build_server().await);
}

#[given("the MCP binary is started against the server")]
async fn start_mcp(world: &mut McpWorld) {
    let url = world.server_url().to_string();
    let mut mcp = McpClient::spawn(&url).await;
    let _ = mcp
        .call(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" },
            }),
        )
        .await;
    world.mcp = Some(mcp);
}

#[given("I have built the standard hierarchy")]
async fn standard_hierarchy(world: &mut McpWorld) {
    let acme = post_for_id(
        world,
        "/org_node/api",
        serde_json::json!({ "name": "Acme", "kind": "enterprise" }),
    )
    .await;
    let eng = post_for_id(
        world,
        "/org_node/api",
        serde_json::json!({ "name": "Engineering", "kind": "division", "parent_id": acme.clone() }),
    )
    .await;
    let payments = post_for_id(
        world,
        "/org_node/api",
        serde_json::json!({ "name": "Payments", "kind": "domain", "parent_id": eng.clone() }),
    )
    .await;
    let checkout = post_for_id(
        world,
        "/org_node/api",
        serde_json::json!({
            "name": "Checkout Team",
            "kind": "team",
            "parent_id": payments.clone(),
            "team_id": "team-checkout",
        }),
    )
    .await;
    let auth = post_for_id(
        world,
        "/org_node/api",
        serde_json::json!({
            "name": "Auth Team",
            "kind": "team",
            "parent_id": payments.clone(),
            "team_id": "team-auth",
        }),
    )
    .await;
    world.ids.insert("acme".into(), acme);
    world.ids.insert("eng".into(), eng);
    world.ids.insert("payments".into(), payments);
    world.ids.insert("checkout".into(), checkout);
    world.ids.insert("auth".into(), auth);
}

#[given(
    regex = r#"^I have registered enterprise bylaw "([^"]+)" of type "([^"]+)"(?: with (?:window|approvers|quiesce_for) "([^"]+)")?$"#
)]
async fn enterprise_bylaw(world: &mut McpWorld, label: String, gate_type: String, value: String) {
    let node_id = world.ids.get("acme").cloned().expect("acme not registered");
    register_bylaw(world, &label, &node_id, &gate_type, &value).await;
}

#[given(
    regex = r#"^I have registered division bylaw "([^"]+)" of type "([^"]+)"(?: with (?:window|approvers|quiesce_for) "([^"]+)")?$"#
)]
async fn division_bylaw(world: &mut McpWorld, label: String, gate_type: String, value: String) {
    let node_id = world.ids.get("eng").cloned().expect("eng not registered");
    register_bylaw(world, &label, &node_id, &gate_type, &value).await;
}

#[given(
    regex = r#"^I have registered domain bylaw "([^"]+)" of type "([^"]+)"(?: with (?:window|approvers|quiesce_for) "([^"]+)")?$"#
)]
async fn domain_bylaw(world: &mut McpWorld, label: String, gate_type: String, value: String) {
    let node_id = world
        .ids
        .get("payments")
        .cloned()
        .expect("payments not registered");
    register_bylaw(world, &label, &node_id, &gate_type, &value).await;
}

#[given(
    regex = r#"^I have registered team bylaw "([^"]+)" of type "([^"]+)"(?: with (?:window|approvers|quiesce_for) "([^"]+)")?$"#
)]
async fn team_bylaw(world: &mut McpWorld, label: String, gate_type: String, value: String) {
    let node_id = world
        .ids
        .get("checkout")
        .cloned()
        .expect("checkout not registered");
    register_bylaw(world, &label, &node_id, &gate_type, &value).await;
}

/// Common bylaw POST. The optional `value` is the field implied by the gate
/// type ("window" / "approvers" / "quiesce_for"); for AutoGate it's empty
/// and we skip the field.
async fn register_bylaw(
    world: &mut McpWorld,
    label: &str,
    node_id: &str,
    gate_type: &str,
    value: &str,
) {
    let mut body = serde_json::json!({
        "org_node_id": node_id,
        "gate_type": gate_type,
        "priority": "10",
    });
    let field = match gate_type {
        "WindowGate" | "FreezePeriod" => Some("window"),
        "ApprovalGate" => Some("approvers"),
        "QuiesceGate" => Some("quiesce_for"),
        _ => None,
    };
    if let (Some(f), false) = (field, value.is_empty()) {
        body.as_object_mut()
            .unwrap()
            .insert(f.into(), Value::String(value.to_string()));
    }
    let id = post_for_id(world, "/bylaw/api", body).await;
    world.ids.insert(label.to_string(), id);
}

#[given(regex = r#"^I have submitted change request "([^"]+)" targeting "([^"]+)"$"#)]
async fn submit_change_request(world: &mut McpWorld, label: String, target: String) {
    let targets = serde_json::to_string(&vec![target]).unwrap();
    let id = post_for_id(
        world,
        "/change_request/api",
        serde_json::json!({
            "summary": label.clone(),
            "target_deployables": targets,
        }),
    )
    .await;
    world.ids.insert(label, id);
}

#[when(regex = r#"^I send MCP request "([^"]+)"$"#)]
async fn send_mcp_method(world: &mut McpWorld, method: String) {
    let resp = world.mcp_mut().call(&method, serde_json::json!({})).await;
    world.last_response = Some(resp);
}

#[when(regex = r#"^I call MCP tool "([^"]+)" with arguments (.+)$"#)]
async fn call_mcp_tool(world: &mut McpWorld, name: String, args_str: String) {
    let resolved = world.resolve(&args_str);
    let args: Value = serde_json::from_str(&resolved).expect("invalid JSON arguments");
    let resp = world
        .mcp_mut()
        .call(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": args }),
        )
        .await;
    if let Some(err) = resp.get("error") {
        panic!("tool call returned JSON-RPC error: {err}");
    }
    let result = resp.get("result").cloned().unwrap_or(Value::Null);
    let structured = result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| result.clone());
    world.last_tool_result = Some(structured);
    world.last_response = Some(result);
}

#[then(regex = r#"^the response should include tool "([^"]+)"$"#)]
async fn response_includes_tool(world: &mut McpWorld, name: String) {
    let resp = world.last_response.as_ref().expect("no response");
    let tools = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .or_else(|| resp.get("tools"))
        .and_then(|t| t.as_array())
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(
        names.iter().any(|n| *n == name),
        "tool {name} not in {names:?}"
    );
}

#[then(regex = r#"^the tool result should be a JSON array of at least (\d+) records$"#)]
async fn tool_result_at_least(world: &mut McpWorld, n: usize) {
    let arr = world.structured_array();
    assert!(
        arr.len() >= n,
        "expected at least {n} records, got {} in {:?}",
        arr.len(),
        arr
    );
}

#[then(regex = r#"^the tool result names should be in order "([^"]+)"$"#)]
async fn names_in_order(world: &mut McpWorld, csv: String) {
    let expected: Vec<String> = csv.split(',').map(|s| s.trim().to_string()).collect();
    let arr = world.structured_array();
    // `ancestors_of` returns Vec<(id, name)>; serde renders that as an array
    // of two-element arrays. Accept either that or an envelope shape (in case
    // the response shape ever flips to envelopes-with-payload).
    let actual: Vec<String> = arr
        .iter()
        .map(|entry| {
            if let Some(pair) = entry.as_array() {
                if let Some(name) = pair.get(1).and_then(|v| v.as_str()) {
                    return name.to_string();
                }
            }
            entry
                .get("payload")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| entry.get("name").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert_eq!(actual, expected, "ancestor name chain mismatch");
}

#[then("the tool result should have plan steps and blockers populated")]
async fn plan_has_steps_and_blockers(world: &mut McpWorld) {
    let r = world.structured_result();
    let steps_str = r
        .pointer("/payload/steps")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no payload.steps in {r}"));
    let steps: Value = serde_json::from_str(steps_str).expect("steps not JSON");
    let step_count = steps.as_array().map(|a| a.len()).unwrap_or(0);
    assert!(
        step_count >= 1,
        "expected at least one step, got: {steps_str}"
    );

    // blockers should at least be present (the seeded FreezePeriod bylaw
    // surfaces as a blocker for prod deployments).
    let blockers_str = r
        .pointer("/payload/blockers")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no payload.blockers in {r}"));
    let blockers: Value = serde_json::from_str(blockers_str).expect("blockers not JSON");
    assert!(
        blockers.is_array(),
        "blockers should be an array, got: {blockers_str}"
    );
}

#[tokio::main]
async fn main() {
    McpWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features/cityhall_mcp.feature")
        .await;
}
