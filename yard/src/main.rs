//! Yard — test infrastructure, data sync, and run-history estimation.

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use manifold_edge::{with_header_identity, HeaderConfig};
use meshql_casbin::CasbinAuth;
use meshql_core::{
    Auth, GraphletteConfig, Repository, RootConfig, ServerConfig, Stash, StashKeyAuth,
};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use yard::{cityhall_client, estimator, groundwork_client, history, sync, validators};

const TEST_ENVIRONMENT_GRAPHQL: &str = include_str!("../config/graph/test_environment.graphql");
const TEST_INFRASTRUCTURE_GRAPHQL: &str =
    include_str!("../config/graph/test_infrastructure.graphql");
const MOCK_SOURCE_GRAPHQL: &str = include_str!("../config/graph/mock_source.graphql");
const DATA_SOURCE_GRAPHQL: &str = include_str!("../config/graph/data_source.graphql");
const DATA_SYNC_GRAPHQL: &str = include_str!("../config/graph/data_sync.graphql");
const TEST_RUN_GRAPHQL: &str = include_str!("../config/graph/test_run.graphql");
const TEST_SUITE_GRAPHQL: &str = include_str!("../config/graph/test_suite.graphql");
const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const AUTH_MODEL: &str = include_str!("../config/auth/model.conf");
const AUTH_POLICY: &str = include_str!("../config/auth/policy.csv");

// ── Static handlers ───────────────────────────────────────────────────────────

async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn serve_app_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache, must-revalidate"),
        ],
        APP_JS,
    )
        .into_response()
}

async fn health_check() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"ok"}"#,
    )
        .into_response()
}

// ── Storage ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Entity {
    pub repo: Arc<dyn Repository>,
    pub searcher: Arc<dyn meshql_core::Searcher>,
}

async fn make_entity(dir: &str, name: &str) -> Entity {
    let db_path = format!("{dir}/{name}.db");
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(
            SqliteConnectOptions::from_str(&format!("sqlite:{db_path}"))
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let repo = Arc::new(SqliteRepository::new_with_pool(pool.clone()).await.unwrap());
    let searcher: Arc<dyn meshql_core::Searcher> =
        Arc::new(SqliteSearcher::new_with_pool(pool).await.unwrap());
    Entity { repo, searcher }
}

#[derive(Clone)]
pub struct AppState {
    pub test_environment: Entity,
    pub test_infrastructure: Entity,
    pub mock_source: Entity,
    pub data_source: Entity,
    pub data_sync: Entity,
    pub test_run: Entity,
    pub test_suite: Entity,
    pub groundwork: Arc<dyn estimator::GroundworkLookup>,
    pub cityhall: Arc<dyn estimator::ChangeRequestLookup>,
}

// ── Custom routes ─────────────────────────────────────────────────────────────

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
                format!("cityhall lookup: {e}"),
            )
                .into_response()
        }
    };

    let tier = req.tier.or(cr.tier).unwrap_or_else(|| "dev".into());

    let inputs = estimator::EstimateInputs {
        change_request_id: cr.id.clone(),
        change_request_summary: cr.summary.clone(),
        tier,
        target_deployable_ids: cr.target_deployables,
        test_environment_repo: &state.test_environment.repo,
        test_infrastructure_repo: &state.test_infrastructure.repo,
        data_sync_repo: &state.data_sync.repo,
        groundwork: state.groundwork.as_ref(),
    };

    match estimator::compute_estimate(inputs).await {
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
struct RecommendSyncBody {
    edge: String,
}

async fn post_data_sync_recommend(Json(body): Json<RecommendSyncBody>) -> Response {
    let Some(edge) = sync::DependencyEdge::parse(&body.edge) else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("unknown dependency edge: {}", body.edge),
        )
            .into_response();
    };
    let rec = sync::recommend_sync(edge);
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
    match history::history_for_env(&state.test_run.repo, &env_id).await {
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
    match history::availability_for_env(&state.test_environment.repo, &state.test_run.repo, &env_id)
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

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3003".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    let groundwork_url =
        std::env::var("GROUNDWORK_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let cityhall_url =
        std::env::var("CITYHALL_URL").unwrap_or_else(|_| "http://localhost:3002".into());
    let union_url = std::env::var("UNION_URL").unwrap_or_else(|_| "http://localhost:3001".into());

    // Cross-app public URLs — published via /config.json for the frontend.
    let groundwork_public_url = std::env::var("GROUNDWORK_PUBLIC_URL")
        .unwrap_or_else(|_| "https://groundwork.tildarc.com".into());
    let union_public_url =
        std::env::var("UNION_PUBLIC_URL").unwrap_or_else(|_| "https://union.tildarc.com".into());
    let cityhall_public_url = std::env::var("CITYHALL_PUBLIC_URL")
        .unwrap_or_else(|_| "https://cityhall.tildarc.com".into());

    let config_body = serde_json::json!({
        "groundwork_public_url": groundwork_public_url,
        "union_public_url":      union_public_url,
        "cityhall_public_url":   cityhall_public_url,
    })
    .to_string();

    std::fs::create_dir_all(&data_dir)?;

    let test_environment = make_entity(&data_dir, "test_environment").await;
    let test_infrastructure = make_entity(&data_dir, "test_infrastructure").await;
    let mock_source = make_entity(&data_dir, "mock_source").await;
    let data_source = make_entity(&data_dir, "data_source").await;
    let data_sync = make_entity(&data_dir, "data_sync").await;
    let test_run = make_entity(&data_dir, "test_run").await;
    let test_suite = make_entity(&data_dir, "test_suite").await;

    let groundwork: Arc<dyn estimator::GroundworkLookup> = Arc::new(
        groundwork_client::HttpGroundworkClient::new(groundwork_url.clone()),
    );
    let cityhall: Arc<dyn estimator::ChangeRequestLookup> = Arc::new(
        cityhall_client::HttpCityhallClient::new(cityhall_url.clone()),
    );

    let app_state = AppState {
        test_environment: test_environment.clone(),
        test_infrastructure: test_infrastructure.clone(),
        mock_source: mock_source.clone(),
        data_source: data_source.clone(),
        data_sync: data_sync.clone(),
        test_run: test_run.clone(),
        test_suite: test_suite.clone(),
        groundwork,
        cityhall,
    };

    let test_environment_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/test_environment.schema.json"))
            .expect("invalid test_environment schema JSON");
    let test_infrastructure_schema_json: serde_json::Value = serde_json::from_str(include_str!(
        "../config/json/test_infrastructure.schema.json"
    ))
    .expect("invalid test_infrastructure schema JSON");
    let mock_source_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/mock_source.schema.json"))
            .expect("invalid mock_source schema JSON");
    let data_source_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/data_source.schema.json"))
            .expect("invalid data_source schema JSON");
    let data_sync_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/data_sync.schema.json"))
            .expect("invalid data_sync schema JSON");
    let test_run_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/test_run.schema.json"))
            .expect("invalid test_run schema JSON");
    let test_suite_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/test_suite.schema.json"))
            .expect("invalid test_suite schema JSON");

    // ── Graphlette configs ──────────────────────────────────────────────────
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
        .singleton_resolver(
            "infrastructure",
            Some("infrastructure_id"),
            "getById",
            format!("{}/test_infrastructure/graph", external_self_url(port)),
        )
        .singleton_resolver(
            "mock_source",
            Some("mock_source_id"),
            "getById",
            format!("{}/mock_source/graph", external_self_url(port)),
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
        .singleton_resolver(
            "target_env",
            Some("target_env_id"),
            "getById",
            format!("{}/test_environment/graph", external_self_url(port)),
        )
        .singleton_resolver(
            "source_env",
            Some("source_env_id"),
            "getById",
            format!("{}/test_environment/graph", external_self_url(port)),
        )
        .singleton_resolver(
            "source_data",
            Some("source_data_id"),
            "getById",
            format!("{}/data_source/graph", external_self_url(port)),
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
            "test_environment",
            Some("test_environment_id"),
            "getById",
            format!("{}/test_environment/graph", external_self_url(port)),
        )
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
        .singleton_resolver(
            "test_suite",
            Some("test_suite_id"),
            "getById",
            format!("{}/test_suite/graph", external_self_url(port)),
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

    let config = ServerConfig {
        port,
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

    // Edge-header auth — see specs/2026-05-12-trusted-header-auth-design.md.
    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id")).await?,
    );

    let test_environment_restlette = meshql_server::build_restlette_router_ext(
        "/test_environment/api",
        test_environment.repo.clone(),
        auth.clone(),
        None,
        Some(validators::test_environment_validator(
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
        Some(validators::base_schema_validator(
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
        Some(validators::base_schema_validator(&mock_source_schema_json)),
        None,
        None,
    );
    let data_source_restlette = meshql_server::build_restlette_router_ext(
        "/data_source/api",
        data_source.repo.clone(),
        auth.clone(),
        None,
        Some(validators::base_schema_validator(&data_source_schema_json)),
        None,
        None,
    );
    let data_sync_restlette = meshql_server::build_restlette_router_ext(
        "/data_sync/api",
        data_sync.repo.clone(),
        auth.clone(),
        None,
        Some(validators::data_sync_validator(&data_sync_schema_json)),
        None,
        None,
    );
    let test_run_restlette = meshql_server::build_restlette_router_ext(
        "/test_run/api",
        test_run.repo.clone(),
        auth.clone(),
        None,
        Some(validators::base_schema_validator(&test_run_schema_json)),
        None,
        None,
    );
    let test_suite_restlette = meshql_server::build_restlette_router_ext(
        "/test_suite/api",
        test_suite.repo.clone(),
        auth.clone(),
        None,
        Some(validators::base_schema_validator(&test_suite_schema_json)),
        None,
        None,
    );

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

    let config_route = Router::new().route(
        "/config.json",
        get(move || {
            let body = config_body.clone();
            async move { ([(header::CONTENT_TYPE, "application/json")], body).into_response() }
        }),
    );

    let extra = Router::new()
        .route("/", get(serve_index))
        .route("/static/app.js", get(serve_app_js))
        .route("/health", get(health_check))
        .merge(config_route)
        .merge(test_environment_restlette)
        .merge(test_infrastructure_restlette)
        .merge(mock_source_restlette)
        .merge(data_source_restlette)
        .merge(data_sync_restlette)
        .merge(test_run_restlette)
        .merge(test_suite_restlette)
        .merge(custom_routes);

    let app = meshql_server::build_app_with_auth(config, auth, extra).await?;
    let app = with_header_identity(app, HeaderConfig::from_env());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("yard listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// URL the in-process graphlette resolvers should use when they need to
/// federate against another graphlette inside the SAME yard process. We use
/// the listening port so the resolver hits the public surface, which matches
/// how cityhall/union/groundwork configure their resolvers.
fn external_self_url(port: u16) -> String {
    std::env::var("YARD_SELF_URL").unwrap_or_else(|_| format!("http://localhost:{port}"))
}

// Suppress unused-import lint when nothing in this file uses Stash directly.
#[allow(dead_code)]
fn _stash_marker(_: Stash) {}
