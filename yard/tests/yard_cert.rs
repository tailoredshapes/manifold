mod common;

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use common::stub_cityhall::{self, ChangeRequestRegistry, StubChangeRequest};
use common::stub_groundwork::{self, DeployableRegistry};
use common::stub_union::{self, StubTeam, TeamRegistry};
use cucumber::{given, then, when, World};
use meshql_core::{GraphletteConfig, NoAuth, Repository, RootConfig, ServerConfig};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use reqwest::Client;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

const TEST_ENVIRONMENT_GRAPHQL: &str = include_str!("../config/graph/test_environment.graphql");
const TEST_INFRASTRUCTURE_GRAPHQL: &str =
    include_str!("../config/graph/test_infrastructure.graphql");
const MOCK_SOURCE_GRAPHQL: &str = include_str!("../config/graph/mock_source.graphql");
const DATA_SOURCE_GRAPHQL: &str = include_str!("../config/graph/data_source.graphql");
const DATA_SYNC_GRAPHQL: &str = include_str!("../config/graph/data_sync.graphql");
const TEST_RUN_GRAPHQL: &str = include_str!("../config/graph/test_run.graphql");
const TEST_SUITE_GRAPHQL: &str = include_str!("../config/graph/test_suite.graphql");

// ── World ────────────────────────────────────────────────────────────────────

#[derive(Debug, World)]
pub struct YardWorld {
    pub server_addr: Option<String>,
    pub ids: HashMap<String, String>,
    pub last_response_status: Option<u16>,
    pub last_response_body: Option<String>,
    pub last_response_content_type: Option<String>,
    pub client: Client,
    pub deployables: Option<DeployableRegistry>,
    pub change_requests: Option<ChangeRequestRegistry>,
    pub teams: Option<TeamRegistry>,
}

impl Default for YardWorld {
    fn default() -> Self {
        Self {
            server_addr: None,
            ids: HashMap::new(),
            last_response_status: None,
            last_response_body: None,
            last_response_content_type: None,
            client: Client::new(),
            deployables: None,
            change_requests: None,
            teams: None,
        }
    }
}

impl YardWorld {
    pub fn base_url(&self) -> &str {
        self.server_addr.as_deref().expect("server not started")
    }

    pub fn resolve(&self, s: &str) -> String {
        let mut result = s.to_string();
        for (k, v) in &self.ids {
            result = result.replace(&format!("<ids.{k}>"), v);
        }
        result
    }

    fn store_response(&mut self, status: u16, body: String, content_type: Option<String>) {
        self.last_response_status = Some(status);
        self.last_response_body = Some(body);
        self.last_response_content_type = content_type;
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

// ── Federation client adapters that hit our in-process stubs ────────────────

struct StubGroundworkClient {
    base_url: String,
    http: reqwest::Client,
}

#[async_trait::async_trait]
impl yard::estimator::GroundworkLookup for StubGroundworkClient {
    async fn get_deployable(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<yard::estimator::DeployableSummary>> {
        // Mirror the production client's REST + GraphQL walk, against the stub.
        let inner_client =
            yard::groundwork_client::HttpGroundworkClient::new(self.base_url.clone());
        // We only need the trait method, not the struct — call through it.
        // (HttpGroundworkClient implements the same trait we are implementing.)
        let _ = &self.http; // keep field used
        <yard::groundwork_client::HttpGroundworkClient as yard::estimator::GroundworkLookup>::get_deployable(&inner_client, id).await
    }
}

struct StubCityhallClient {
    base_url: String,
}

#[async_trait::async_trait]
impl yard::estimator::ChangeRequestLookup for StubCityhallClient {
    async fn get_change_request(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<yard::estimator::ChangeRequestSummary>> {
        let inner = yard::cityhall_client::HttpCityhallClient::new(self.base_url.clone());
        <yard::cityhall_client::HttpCityhallClient as yard::estimator::ChangeRequestLookup>::get_change_request(&inner, id).await
    }
}

#[derive(Clone)]
struct AppState {
    test_environment: TestEntity,
    test_infrastructure: TestEntity,
    data_sync: TestEntity,
    test_run: TestEntity,
    groundwork: Arc<dyn yard::estimator::GroundworkLookup>,
    cityhall: Arc<dyn yard::estimator::ChangeRequestLookup>,
}

async fn build_test_server() -> (
    String,
    DeployableRegistry,
    ChangeRequestRegistry,
    TeamRegistry,
) {
    let test_environment = make_entity().await;
    let test_infrastructure = make_entity().await;
    let mock_source = make_entity().await;
    let data_source = make_entity().await;
    let data_sync = make_entity().await;
    let test_run = make_entity().await;
    let test_suite = make_entity().await;

    let (groundwork_url, deployables) = stub_groundwork::spawn().await;
    let (cityhall_url, change_requests) = stub_cityhall::spawn().await;
    let (union_url, teams) = stub_union::spawn().await;

    let groundwork: Arc<dyn yard::estimator::GroundworkLookup> = Arc::new(StubGroundworkClient {
        base_url: groundwork_url.clone(),
        http: reqwest::Client::new(),
    });
    let cityhall: Arc<dyn yard::estimator::ChangeRequestLookup> = Arc::new(StubCityhallClient {
        base_url: cityhall_url.clone(),
    });

    let test_environment_root = RootConfig::builder()
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
        .singleton_resolver(
            "deployable",
            Some("deployable_id"),
            "getById",
            format!("{}/deployable/graph", groundwork_url),
        )
        .singleton_resolver(
            "service",
            Some("service_id"),
            "getById",
            format!("{}/service/graph", groundwork_url),
        )
        .build();
    let test_infrastructure_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByProvider", r#"{"payload.provider": "{{provider}}"}"#)
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let mock_source_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector("getByLanguage", r#"{"payload.language": "{{language}}"}"#)
        .build();
    let data_source_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let data_sync_root = RootConfig::builder()
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
    let test_run_root = RootConfig::builder()
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
        .singleton_resolver(
            "change_request",
            Some("change_request_id"),
            "getById",
            format!("{}/change_request/graph", cityhall_url),
        )
        .singleton_resolver(
            "team",
            Some("team_id"),
            "getById",
            format!("{}/team/graph", union_url),
        )
        .build();
    let test_suite_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector(
            "getByDeployableId",
            r#"{"payload.deployable_id": "{{deployable_id}}"}"#,
        )
        .vector("getByRunner", r#"{"payload.runner": "{{runner}}"}"#)
        .singleton_resolver(
            "deployable",
            Some("deployable_id"),
            "getById",
            format!("{}/deployable/graph", groundwork_url),
        )
        .build();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/test_environment/graph".into(),
                schema_text: TEST_ENVIRONMENT_GRAPHQL.into(),
                root_config: test_environment_root,
                searcher: test_environment.searcher.clone(),
            },
            GraphletteConfig {
                path: "/test_infrastructure/graph".into(),
                schema_text: TEST_INFRASTRUCTURE_GRAPHQL.into(),
                root_config: test_infrastructure_root,
                searcher: test_infrastructure.searcher.clone(),
            },
            GraphletteConfig {
                path: "/mock_source/graph".into(),
                schema_text: MOCK_SOURCE_GRAPHQL.into(),
                root_config: mock_source_root,
                searcher: mock_source.searcher.clone(),
            },
            GraphletteConfig {
                path: "/data_source/graph".into(),
                schema_text: DATA_SOURCE_GRAPHQL.into(),
                root_config: data_source_root,
                searcher: data_source.searcher.clone(),
            },
            GraphletteConfig {
                path: "/data_sync/graph".into(),
                schema_text: DATA_SYNC_GRAPHQL.into(),
                root_config: data_sync_root,
                searcher: data_sync.searcher.clone(),
            },
            GraphletteConfig {
                path: "/test_run/graph".into(),
                schema_text: TEST_RUN_GRAPHQL.into(),
                root_config: test_run_root,
                searcher: test_run.searcher.clone(),
            },
            GraphletteConfig {
                path: "/test_suite/graph".into(),
                schema_text: TEST_SUITE_GRAPHQL.into(),
                root_config: test_suite_root,
                searcher: test_suite.searcher.clone(),
            },
        ],
        restlettes: vec![],
    };

    let auth = Arc::new(NoAuth);

    let test_environment_schema_json: Value =
        serde_json::from_str(include_str!("../config/json/test_environment.schema.json")).unwrap();
    let test_infrastructure_schema_json: Value = serde_json::from_str(include_str!(
        "../config/json/test_infrastructure.schema.json"
    ))
    .unwrap();
    let mock_source_schema_json: Value =
        serde_json::from_str(include_str!("../config/json/mock_source.schema.json")).unwrap();
    let data_source_schema_json: Value =
        serde_json::from_str(include_str!("../config/json/data_source.schema.json")).unwrap();
    let data_sync_schema_json: Value =
        serde_json::from_str(include_str!("../config/json/data_sync.schema.json")).unwrap();
    let test_run_schema_json: Value =
        serde_json::from_str(include_str!("../config/json/test_run.schema.json")).unwrap();
    let test_suite_schema_json: Value =
        serde_json::from_str(include_str!("../config/json/test_suite.schema.json")).unwrap();

    let test_environment_restlette = meshql_server::build_restlette_router_ext(
        "/test_environment/api",
        test_environment.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::test_environment_validator(
            &test_environment_schema_json,
        )),
        None,
        None,
    );
    let test_infrastructure_restlette = meshql_server::build_restlette_router_ext(
        "/test_infrastructure/api",
        test_infrastructure.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::base_schema_validator(
            &test_infrastructure_schema_json,
        )),
        None,
        None,
    );
    let mock_source_restlette = meshql_server::build_restlette_router_ext(
        "/mock_source/api",
        mock_source.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::base_schema_validator(
            &mock_source_schema_json,
        )),
        None,
        None,
    );
    let data_source_restlette = meshql_server::build_restlette_router_ext(
        "/data_source/api",
        data_source.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::base_schema_validator(
            &data_source_schema_json,
        )),
        None,
        None,
    );
    let data_sync_restlette = meshql_server::build_restlette_router_ext(
        "/data_sync/api",
        data_sync.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::data_sync_validator(
            &data_sync_schema_json,
        )),
        None,
        None,
    );
    let test_run_restlette = meshql_server::build_restlette_router_ext(
        "/test_run/api",
        test_run.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::base_schema_validator(
            &test_run_schema_json,
        )),
        None,
        None,
    );
    let test_suite_restlette = meshql_server::build_restlette_router_ext(
        "/test_suite/api",
        test_suite.repo.clone(),
        auth.clone(),
        None,
        Some(yard::validators::base_schema_validator(
            &test_suite_schema_json,
        )),
        None,
        None,
    );

    let app_state = AppState {
        test_environment: test_environment.clone(),
        test_infrastructure: test_infrastructure.clone(),
        data_sync: data_sync.clone(),
        test_run: test_run.clone(),
        groundwork,
        cityhall,
    };

    let custom_routes = Router::new()
        .route(
            "/change_request/:id/estimate",
            post(post_change_request_estimate),
        )
        .route("/data_sync/recommend", post(post_data_sync_recommend))
        .route(
            "/test_environment/:id/history",
            get(get_test_environment_history),
        )
        .route(
            "/test_environment/:id/availability",
            get(get_test_environment_availability),
        )
        .with_state(app_state);

    let extra = Router::new()
        .route(
            "/health",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    r#"{"status":"ok"}"#,
                )
                    .into_response()
            }),
        )
        .merge(test_environment_restlette)
        .merge(test_infrastructure_restlette)
        .merge(mock_source_restlette)
        .merge(data_source_restlette)
        .merge(data_sync_restlette)
        .merge(test_run_restlette)
        .merge(test_suite_restlette)
        .merge(custom_routes);

    let app = meshql_server::build_app_ext(server_config, extra)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        format!("http://127.0.0.1:{}", addr.port()),
        deployables,
        change_requests,
        teams,
    )
}

// ── Custom-route handlers ────────────────────────────────────────────────────

#[derive(serde::Deserialize, Default)]
struct EstimateRequest {
    #[serde(default)]
    tier: Option<String>,
}

async fn post_change_request_estimate(
    State(state): State<AppState>,
    Path(cr_id): Path<String>,
    Json(req): Json<EstimateRequest>,
) -> Response {
    let cr = match state.cityhall.get_change_request(&cr_id).await {
        Ok(Some(c)) => c,
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
                format!("cityhall: {e}"),
            )
                .into_response()
        }
    };

    let tier = req.tier.or(cr.tier).unwrap_or_else(|| "dev".into());

    let inputs = yard::estimator::EstimateInputs {
        change_request_id: cr.id.clone(),
        change_request_summary: cr.summary,
        tier,
        target_deployable_ids: cr.target_deployables,
        test_environment_repo: &state.test_environment.repo,
        test_infrastructure_repo: &state.test_infrastructure.repo,
        data_sync_repo: &state.data_sync.repo,
        groundwork: state.groundwork.as_ref(),
    };

    match yard::estimator::compute_estimate(inputs).await {
        Ok(estimate) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&estimate).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("compute_estimate: {e}"),
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
    Query(_q): Query<HashMap<String, String>>,
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

// ── HTTP helpers ─────────────────────────────────────────────────────────────

async fn do_request(world: &mut YardWorld, method: &str, path: &str, body: Option<Value>) {
    let url = format!("{}{}", world.base_url(), path);
    let builder = match method {
        "GET" => world.client.get(&url),
        "DELETE" => world.client.delete(&url),
        "POST" => world
            .client
            .post(&url)
            .json(body.as_ref().unwrap_or(&Value::Null)),
        "PUT" => world
            .client
            .put(&url)
            .json(body.as_ref().unwrap_or(&Value::Null)),
        _ => panic!("Unknown method: {method}"),
    };
    let resp = builder.send().await.expect("request failed");
    let status = resp.status().as_u16();
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let body_text = resp.text().await.unwrap_or_default();
    world.store_response(status, body_text, ct);
}

async fn post_for_id(world: &mut YardWorld, path: &str, payload: Value) -> String {
    let url = format!("{}{}", world.base_url(), path);
    let resp = world.client.post(&url).json(&payload).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    assert!(
        status == 200 || status == 201,
        "POST {path} failed ({status}): {text}\npayload={payload}"
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
async fn start_server(world: &mut YardWorld) {
    let (addr, deployables, change_requests, teams) = build_test_server().await;
    world.server_addr = Some(addr);
    world.deployables = Some(deployables);
    world.change_requests = Some(change_requests);
    world.teams = Some(teams);
    world.ids.clear();
}

#[given(regex = r#"^I capture the last id as "(.+)"$"#)]
async fn capture_last_id(world: &mut YardWorld, label: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    let id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no id in response: {body}"))
        .to_string();
    world.ids.insert(label, id);
}

#[given(regex = r#"^the Groundwork stub knows deployable "([^"]+)" as "([^"]+)"$"#)]
async fn groundwork_stub_knows(world: &mut YardWorld, id: String, name: String) {
    let reg = world
        .deployables
        .as_ref()
        .expect("Groundwork stub not started");
    reg.register_with_deps(&id, &name, &[]);
}

#[given(
    regex = r#"^the Groundwork stub knows deployable "([^"]+)" as "([^"]+)" depending on "([^"]+)"$"#
)]
async fn groundwork_stub_knows_with_dep(
    world: &mut YardWorld,
    id: String,
    name: String,
    dep: String,
) {
    let reg = world
        .deployables
        .as_ref()
        .expect("Groundwork stub not started");
    reg.register_with_deps(&id, &name, &[dep.as_str()]);
}

#[given(
    regex = r#"^the Groundwork stub knows deployable "([^"]+)" as "([^"]+)" with no dependencies$"#
)]
async fn groundwork_stub_knows_no_deps(world: &mut YardWorld, id: String, name: String) {
    let reg = world
        .deployables
        .as_ref()
        .expect("Groundwork stub not started");
    reg.register_with_deps(&id, &name, &[]);
}

#[given(regex = r#"^the Cityhall stub knows change request "([^"]+)" with summary "([^"]+)"$"#)]
async fn cityhall_stub_knows_cr(world: &mut YardWorld, id: String, summary: String) {
    let reg = world
        .change_requests
        .as_ref()
        .expect("Cityhall stub not started");
    reg.insert(StubChangeRequest {
        id,
        summary,
        status: Some("submitted".into()),
        tier: Some("prod".into()),
        target_deployables: vec![],
    });
}

#[given(
    regex = r#"^the Cityhall stub knows change request "([^"]+)" with summary "([^"]+)" targeting "([^"]+)"$"#
)]
async fn cityhall_stub_knows_cr_targeting(
    world: &mut YardWorld,
    id: String,
    summary: String,
    targets: String,
) {
    let reg = world
        .change_requests
        .as_ref()
        .expect("Cityhall stub not started");
    let parsed: Vec<String> = targets
        .split(',')
        .map(|t| t.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    reg.insert(StubChangeRequest {
        id,
        summary,
        status: Some("submitted".into()),
        tier: Some("dev".into()),
        target_deployables: parsed,
    });
}

#[given(regex = r#"^the Union stub knows team "(.+)" as "(.+)" of kind "(.+)"$"#)]
async fn union_stub_knows_team(world: &mut YardWorld, team_id: String, name: String, kind: String) {
    let reg = world.teams.as_ref().expect("Union stub not started");
    reg.insert(StubTeam {
        id: team_id,
        name,
        kind,
        description: None,
    });
}

#[given(regex = r#"^I have registered test_environment "([^"]+)" with kind "([^"]+)"$"#)]
async fn register_env(world: &mut YardWorld, name: String, kind: String) {
    let payload = match kind.as_str() {
        "external" => serde_json::json!({"name": name, "kind": kind, "contractual_limit": "5"}),
        "mock" | "stub" => {
            serde_json::json!({"name": name, "kind": kind, "mock_source_id": "ms-default"})
        }
        _ => serde_json::json!({"name": name, "kind": kind}),
    };
    let id = post_for_id(world, "/test_environment/api", payload).await;
    world.ids.insert(name, id);
}

#[given(
    regex = r#"^I have registered test_environment "([^"]+)" with kind "([^"]+)" for deployable "([^"]+)" with spinup_minutes "([^"]+)" and cost_per_hour "([^"]+)"$"#
)]
async fn register_env_for_dep(
    world: &mut YardWorld,
    name: String,
    kind: String,
    dep_id: String,
    spinup: String,
    cost: String,
) {
    let id = post_for_id(
        world,
        "/test_environment/api",
        serde_json::json!({
            "name": name,
            "kind": kind,
            "deployable_id": dep_id,
            "spinup_minutes": spinup,
            "cost_per_hour": cost,
        }),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(
    regex = r#"^I have registered test_environment "([^"]+)" with kind "([^"]+)" for deployable "([^"]+)" with spinup_minutes "([^"]+)" and contractual_limit "([^"]+)" and rate_limit "([^"]+)"$"#
)]
async fn register_external_env(
    world: &mut YardWorld,
    name: String,
    kind: String,
    dep_id: String,
    spinup: String,
    contractual: String,
    rate: String,
) {
    let id = post_for_id(
        world,
        "/test_environment/api",
        serde_json::json!({
            "name": name,
            "kind": kind,
            "deployable_id": dep_id,
            "spinup_minutes": spinup,
            "contractual_limit": contractual,
            "rate_limit": rate,
        }),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered test_infrastructure "(.+)" with provider "(.+)"$"#)]
async fn register_infra(world: &mut YardWorld, name: String, provider: String) {
    let id = post_for_id(
        world,
        "/test_infrastructure/api",
        serde_json::json!({"name": name, "provider": provider}),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered mock_source "(.+)" with language "(.+)"$"#)]
async fn register_mock_source(world: &mut YardWorld, name: String, language: String) {
    let id = post_for_id(
        world,
        "/mock_source/api",
        serde_json::json!({"name": name, "language": language}),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered data_source "(.+)" with kind "(.+)"$"#)]
async fn register_data_source(world: &mut YardWorld, name: String, kind: String) {
    let id = post_for_id(
        world,
        "/data_source/api",
        serde_json::json!({"name": name, "kind": kind}),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have recorded a (passed|failed) test_run on "(.+)" with duration "(.+)"$"#)]
async fn record_run(world: &mut YardWorld, status: String, env_label: String, duration: String) {
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
            "duration_minutes": duration,
        }),
    )
    .await;
}

#[given(regex = r#"^I have registered test_suite "(.+)" against deployable "(.+)"$"#)]
async fn register_test_suite(world: &mut YardWorld, name: String, dep_id: String) {
    let id = post_for_id(
        world,
        "/test_suite/api",
        serde_json::json!({"name": name, "deployable_id": dep_id}),
    )
    .await;
    world.ids.insert(name, id);
}

#[when(regex = r#"^I (GET|DELETE) "(.+)"$"#)]
async fn http_get_delete(world: &mut YardWorld, method: String, path: String) {
    let resolved = world.resolve(&path);
    do_request(world, &method, &resolved, None).await;
}

#[when(regex = r#"^I POST to "(.+)" with body (.+)$"#)]
async fn http_post(world: &mut YardWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let resolved_body = world.resolve(&body_str);
    let body: Value = serde_json::from_str(&resolved_body).expect("invalid JSON body");
    do_request(world, "POST", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I PUT "(.+)" with body (.+)$"#)]
async fn http_put(world: &mut YardWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let resolved_body = world.resolve(&body_str);
    let body: Value = serde_json::from_str(&resolved_body).expect("invalid JSON body");
    do_request(world, "PUT", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I query the "(.+)" graph with: (.+)$"#)]
async fn graphql_query(world: &mut YardWorld, entity: String, query_str: String) {
    let resolved = world.resolve(&query_str);
    let path = format!("/{entity}/graph");
    let body = serde_json::json!({ "query": resolved });
    do_request(world, "POST", &path, Some(body)).await;
}

#[then(regex = r"^the response status should be (\d+)$")]
async fn check_status(world: &mut YardWorld, expected: u16) {
    let actual = world.last_response_status.expect("no response");
    assert_eq!(
        actual, expected,
        "Expected {expected}, got {actual}. Body: {:?}",
        world.last_response_body
    );
}

#[then(regex = r#"^the response body should contain "(.+)"$"#)]
async fn body_contains(world: &mut YardWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&resolved),
        "expected body to contain {resolved:?}\nGot: {body}"
    );
}

#[then(regex = r#"^the response body should not contain "(.+)"$"#)]
async fn body_not_contains(world: &mut YardWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        !body.contains(&resolved),
        "expected body NOT to contain {resolved:?}\nGot: {body}"
    );
}

#[then(r#"the response body should have an "id" field"#)]
async fn body_has_id(world: &mut YardWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    assert!(
        parsed.get("id").map(|v| !v.is_null()).unwrap_or(false),
        "no id in: {body}"
    );
}

#[then("there should be no GraphQL errors")]
async fn no_graphql_errors(world: &mut YardWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    let has_errors = parsed
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    assert!(!has_errors, "GraphQL errors in response: {body}");
}

#[then(regex = r#"^the response data should contain "(.+)"$"#)]
async fn response_data_contains(world: &mut YardWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&resolved),
        "Expected data to contain {resolved:?}\nGot: {body}"
    );
}

#[then(regex = r#"^the response data should not contain "(.+)"$"#)]
async fn response_data_not_contains(world: &mut YardWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        !body.contains(&resolved),
        "Expected data NOT to contain {resolved:?}\nGot: {body}"
    );
}

#[then(regex = r"^the history pass_rate should be ([0-9.]+)$")]
async fn history_pass_rate(world: &mut YardWorld, expected: f64) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let actual = parsed
        .get("pass_rate")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected pass_rate={expected}, got {actual}. Body: {body}"
    );
}

#[then(regex = r"^the history average_duration_minutes should be ([0-9.]+)$")]
async fn history_avg_duration(world: &mut YardWorld, expected: f64) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let actual = parsed
        .get("average_duration_minutes")
        .and_then(|v| v.as_f64())
        .unwrap_or(-1.0);
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected average_duration_minutes={expected}, got {actual}. Body: {body}"
    );
}

#[then(regex = r"^the estimate total_minutes should be at least (\d+)$")]
async fn estimate_total_minutes_at_least(world: &mut YardWorld, expected: u64) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let actual = parsed
        .get("total_minutes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        actual >= expected,
        "expected total_minutes ≥ {expected}, got {actual}. Body: {body}"
    );
}

#[then(r"the estimate total_cost should be greater than 0")]
async fn estimate_total_cost_positive(world: &mut YardWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let actual = parsed
        .get("total_cost")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    assert!(
        actual > 0.0,
        "expected total_cost > 0, got {actual}. Body: {body}"
    );
}

#[tokio::main]
async fn main() {
    YardWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features")
        .await;
}
