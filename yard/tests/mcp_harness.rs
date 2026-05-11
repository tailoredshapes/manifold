//! BDD harness for the `yard-mcp` binary.
//!
//! Spawns the binary as a subprocess (built via `CARGO_BIN_EXE_yard-mcp`) and
//! exchanges line-delimited JSON-RPC frames over its stdin/stdout. The
//! `YARD_URL` env var points the MCP child at an in-process Yard HTTP server
//! (REST + GraphQL) bound to a random port. The in-process server registers
//! REST routes for the seven catalog entities, graphlettes for the same
//! entities (so auto-derived `list_*` capabilities can resolve via /graph),
//! plus the two custom GET/POST endpoints the MCP custom capabilities wrap
//! (`/test_environment/:id/history`, `/data_sync/recommend`). The
//! `estimate_for_change_request` route is omitted: the corresponding
//! scenarios exercise `tools/list` only, not the round-trip call.

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

const TEST_ENVIRONMENT_GRAPHQL: &str = include_str!("../config/graph/test_environment.graphql");
const TEST_INFRASTRUCTURE_GRAPHQL: &str =
    include_str!("../config/graph/test_infrastructure.graphql");
const MOCK_SOURCE_GRAPHQL: &str = include_str!("../config/graph/mock_source.graphql");
const DATA_SOURCE_GRAPHQL: &str = include_str!("../config/graph/data_source.graphql");
const DATA_SYNC_GRAPHQL: &str = include_str!("../config/graph/data_sync.graphql");
const TEST_RUN_GRAPHQL: &str = include_str!("../config/graph/test_run.graphql");
const TEST_SUITE_GRAPHQL: &str = include_str!("../config/graph/test_suite.graphql");

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
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
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
                            arr.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
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
    test_environment: TestEntity,
    test_run: TestEntity,
}

// ── Custom-route handlers (mirror yard::main) ────────────────────────────────

async fn get_test_environment_history(
    State(state): State<AppState>,
    Path(env_id): Path<String>,
) -> Response {
    match yard::history::history_for_env(&state.test_run.repo, &env_id).await {
        Ok(h) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&h).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("history: {e}"),
        )
            .into_response(),
    }
}

async fn get_test_environment_availability(
    State(state): State<AppState>,
    Path(env_id): Path<String>,
) -> Response {
    match yard::history::availability_for_env(
        &state.test_environment.repo,
        &state.test_run.repo,
        &env_id,
    )
    .await
    {
        Ok(a) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&a).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("availability: {e}"),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
struct RecommendBody {
    edge: String,
}

async fn post_data_sync_recommend(Json(body): Json<RecommendBody>) -> Response {
    let Some(edge) = yard::sync::DependencyEdge::parse(&body.edge) else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown dependency edge: {}", body.edge),
        )
            .into_response();
    };
    let rec = yard::sync::recommend_sync(edge);
    (
        axum::http::StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&rec).unwrap_or_default(),
    )
        .into_response()
}

// ── In-process Yard REST server ──────────────────────────────────────────────

async fn build_server() -> String {
    let test_environment = make_entity().await;
    let test_infrastructure = make_entity().await;
    let mock_source = make_entity().await;
    let data_source = make_entity().await;
    let data_sync = make_entity().await;
    let test_run = make_entity().await;
    let test_suite = make_entity().await;

    let auth = Arc::new(NoAuth);

    let test_environment_restlette = meshql_server::build_restlette_router_ext(
        "/test_environment/api",
        test_environment.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/test_environment.schema.json"
        ))),
        None,
        None,
    );
    let test_infrastructure_restlette = meshql_server::build_restlette_router_ext(
        "/test_infrastructure/api",
        test_infrastructure.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/test_infrastructure.schema.json"
        ))),
        None,
        None,
    );
    let mock_source_restlette = meshql_server::build_restlette_router_ext(
        "/mock_source/api",
        mock_source.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/mock_source.schema.json"
        ))),
        None,
        None,
    );
    let data_source_restlette = meshql_server::build_restlette_router_ext(
        "/data_source/api",
        data_source.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/data_source.schema.json"
        ))),
        None,
        None,
    );
    let data_sync_restlette = meshql_server::build_restlette_router_ext(
        "/data_sync/api",
        data_sync.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/data_sync.schema.json"
        ))),
        None,
        None,
    );
    let test_run_restlette = meshql_server::build_restlette_router_ext(
        "/test_run/api",
        test_run.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/test_run.schema.json"
        ))),
        None,
        None,
    );
    let test_suite_restlette = meshql_server::build_restlette_router_ext(
        "/test_suite/api",
        test_suite.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!(
            "../config/json/test_suite.schema.json"
        ))),
        None,
        None,
    );

    let app_state = AppState {
        test_environment: test_environment.clone(),
        test_run: test_run.clone(),
    };

    let custom_routes = Router::new()
        .route(
            "/test_environment/:id/history",
            get(get_test_environment_history),
        )
        .route(
            "/test_environment/:id/availability",
            get(get_test_environment_availability),
        )
        .route("/data_sync/recommend", post(post_data_sync_recommend))
        .with_state(app_state);

    let extra = Router::new()
        .merge(test_environment_restlette)
        .merge(test_infrastructure_restlette)
        .merge(mock_source_restlette)
        .merge(data_source_restlette)
        .merge(data_sync_restlette)
        .merge(test_run_restlette)
        .merge(test_suite_restlette)
        .merge(custom_routes);

    let test_environment_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector(
            "getByDeployableId",
            r#"{"payload.deployable_id": "{{deployable_id}}"}"#,
        )
        .vector(
            "getByServiceId",
            r#"{"payload.service_id": "{{service_id}}"}"#,
        )
        .vector(
            "getByInfrastructureId",
            r#"{"payload.infrastructure_id": "{{infrastructure_id}}"}"#,
        )
        .build();
    let test_infrastructure_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByProvider", r#"{"payload.provider": "{{provider}}"}"#)
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let mock_source_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector("getByLanguage", r#"{"payload.language": "{{language}}"}"#)
        .build();
    let data_source_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let data_sync_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector(
            "getByTargetEnvId",
            r#"{"payload.target_env_id": "{{target_env_id}}"}"#,
        )
        .vector(
            "getBySourceEnvId",
            r#"{"payload.source_env_id": "{{source_env_id}}"}"#,
        )
        .build();
    let test_run_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByTestEnvironmentId",
            r#"{"payload.test_environment_id": "{{test_environment_id}}"}"#,
        )
        .vector(
            "getByChangeRequestId",
            r#"{"payload.change_request_id": "{{change_request_id}}"}"#,
        )
        .vector("getByStatus", r#"{"payload.status": "{{status}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .build();
    let test_suite_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector(
            "getByDeployableId",
            r#"{"payload.deployable_id": "{{deployable_id}}"}"#,
        )
        .vector("getByRunner", r#"{"payload.runner": "{{runner}}"}"#)
        .build();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/test_environment/graph".into(),
                schema_text: TEST_ENVIRONMENT_GRAPHQL.into(),
                root_config: test_environment_gql,
                searcher: test_environment.searcher.clone(),
            },
            GraphletteConfig {
                path: "/test_infrastructure/graph".into(),
                schema_text: TEST_INFRASTRUCTURE_GRAPHQL.into(),
                root_config: test_infrastructure_gql,
                searcher: test_infrastructure.searcher.clone(),
            },
            GraphletteConfig {
                path: "/mock_source/graph".into(),
                schema_text: MOCK_SOURCE_GRAPHQL.into(),
                root_config: mock_source_gql,
                searcher: mock_source.searcher.clone(),
            },
            GraphletteConfig {
                path: "/data_source/graph".into(),
                schema_text: DATA_SOURCE_GRAPHQL.into(),
                root_config: data_source_gql,
                searcher: data_source.searcher.clone(),
            },
            GraphletteConfig {
                path: "/data_sync/graph".into(),
                schema_text: DATA_SYNC_GRAPHQL.into(),
                root_config: data_sync_gql,
                searcher: data_sync.searcher.clone(),
            },
            GraphletteConfig {
                path: "/test_run/graph".into(),
                schema_text: TEST_RUN_GRAPHQL.into(),
                root_config: test_run_gql,
                searcher: test_run.searcher.clone(),
            },
            GraphletteConfig {
                path: "/test_suite/graph".into(),
                schema_text: TEST_SUITE_GRAPHQL.into(),
                root_config: test_suite_gql,
                searcher: test_suite.searcher.clone(),
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_yard-mcp"))
            .env("YARD_URL", server_url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn yard-mcp");
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
            "getByStatus",
            "getByTestEnvironmentId",
            "getByDeployableId",
            "getByTargetEnvId",
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

#[given("a Yard server is running")]
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

#[given(regex = r#"^I have registered test environment "([^"]+)" of kind "([^"]+)"$"#)]
async fn register_test_environment(world: &mut McpWorld, name: String, kind: String) {
    let id = post_for_id(
        world,
        "/test_environment/api",
        serde_json::json!({ "name": name.clone(), "kind": kind }),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(
    regex = r#"^I have logged test run for "([^"]+)" with status "([^"]+)" lasting (\d+) minutes$"#
)]
async fn log_test_run(
    world: &mut McpWorld,
    env_label: String,
    status: String,
    minutes: u64,
) {
    let env_id = world
        .ids
        .get(&env_label)
        .cloned()
        .unwrap_or_else(|| panic!("env {env_label} not registered"));
    let _ = post_for_id(
        world,
        "/test_run/api",
        serde_json::json!({
            "test_environment_id": env_id,
            "status": status,
            "duration_minutes": minutes.to_string(),
        }),
    )
    .await;
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

#[then(regex = r#"^the tool result run_count should be at least (\d+)$"#)]
async fn tool_result_run_count_at_least(world: &mut McpWorld, n: u64) {
    let r = world.structured_result();
    let actual = r
        .get("run_count")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| panic!("no integer run_count in {r}"));
    assert!(
        actual >= n,
        "expected run_count >= {n}, got {actual} in {r}"
    );
}

#[then(regex = r#"^the tool result kind should equal "([^"]+)"$"#)]
async fn tool_result_kind_eq(world: &mut McpWorld, expected: String) {
    let r = world.structured_result();
    let actual = r
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no kind in {r}"));
    assert_eq!(actual, expected, "kind mismatch in {r}");
}

#[tokio::main]
async fn main() {
    McpWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features/yard_mcp.feature")
        .await;
}
