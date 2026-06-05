//! Cityhall — governance, change planning, and deployment Gantt output.

use axum::{
    extract::{Path, State},
    http::header,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use cityhall::{bylaw, gantt, groundwork_client, plan};
use manifold_edge::{with_header_identity, with_response_cache, HeaderConfig};
use meshql_casbin::CasbinAuth;
use meshql_core::{Auth, GraphletteConfig, Repository, RootConfig, ServerConfig, StashKeyAuth};
#[cfg(feature = "sqlite")]
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
#[cfg(feature = "mongo")]
use meshql_mongo::{MongoRepository, MongoSearcher};
#[cfg(feature = "sqlite")]
use std::str::FromStr;
use std::sync::Arc;

const ORG_NODE_GRAPHQL: &str = include_str!("../config/graph/org_node.graphql");
const BYLAW_GRAPHQL: &str = include_str!("../config/graph/bylaw.graphql");
const CHANGE_REQUEST_GRAPHQL: &str = include_str!("../config/graph/change_request.graphql");
const DEPLOYMENT_PLAN_GRAPHQL: &str = include_str!("../config/graph/deployment_plan.graphql");
const GANTT_OUTPUT_GRAPHQL: &str = include_str!("../config/graph/gantt_output.graphql");
const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const AUTH_MODEL: &str = include_str!("../config/auth/model.conf");
const AUTH_POLICY: &str = include_str!("../config/auth/policy.csv");

// ── Static handlers ───────────────────────────────────────────────────────────

async fn serve_index() -> Html<String> {
    Html(manifold_ui::index_html(INDEX_HTML))
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

async fn serve_favicon() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        manifold_ui::FAVICON,
    )
        .into_response()
}

async fn serve_manifold_ui_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache, must-revalidate"),
        ],
        manifold_ui::CSS,
    )
        .into_response()
}

async fn serve_manifold_ui_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache, must-revalidate"),
        ],
        manifold_ui::JS,
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

// ── Storage bootstrap ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Entity {
    pub repo: Arc<dyn Repository>,
    pub searcher: Arc<dyn meshql_core::Searcher>,
}

// SQLite backend (default; local / dev / single-box).
#[cfg(feature = "sqlite")]
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

// MongoDB backend (Atlas; serverless / Lambda / multi-instance). One collection
// per entity in the MONGO_DB database. The Mongo repo/searcher enforce auth at
// the store, so we build the same CasbinAuth from the embedded policy here.
#[cfg(feature = "mongo")]
async fn make_entity(_dir: &str, name: &str) -> Entity {
    let uri = std::env::var("MONGO_URL").expect("MONGO_URL is required for the mongo build");
    let db = std::env::var("MONGO_DB").unwrap_or_else(|_| "manifold".into());
    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id"))
            .await
            .expect("auth policy"),
    );
    let repo: Arc<dyn meshql_core::Repository> =
        Arc::new(MongoRepository::new(&uri, &db, name, auth.clone()).await.unwrap());
    let searcher: Arc<dyn meshql_core::Searcher> =
        Arc::new(MongoSearcher::new(&uri, &db, name, auth).await.unwrap());
    Entity { repo, searcher }
}

#[derive(Clone)]
pub struct AppState {
    pub org_node: Entity,
    pub bylaw: Entity,
    pub change_request: Entity,
    pub deployment_plan: Entity,
    pub gantt_output: Entity,
    pub groundwork: Arc<dyn plan::GroundworkLookup>,
}

// ── Custom routes ─────────────────────────────────────────────────────────────

async fn get_ancestors(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match bylaw::ancestors_of(&state.org_node.repo, &id).await {
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
    match bylaw::effective_bylaws_for(&state.org_node.repo, &state.bylaw.repo, &id).await {
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
    let target_deployable_ids = parse_target_deployables(
        env.payload
            .get("target_deployables")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );

    let inputs = plan::PlanInputs {
        change_request_id: cr_id.clone(),
        change_request_summary: summary,
        tier,
        target_deployable_ids,
        org_node_repo: &state.org_node.repo,
        bylaw_repo: &state.bylaw.repo,
        groundwork: state.groundwork.as_ref(),
    };
    let computed = match plan::compute_plan(inputs).await {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("compute_plan: {e}"),
            )
                .into_response()
        }
    };

    let envelope = build_plan_envelope(&computed);
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

    let computed: plan::ComputedPlan = match parse_plan_payload(&env.payload) {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("malformed plan payload: {e}"),
            )
                .into_response()
        }
    };

    let mermaid = gantt::render_gantt(&computed);
    let envelope = build_gantt_envelope(&plan_id, &computed.tier, &mermaid);
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

// ── Helpers for plan envelopes ──────────────────────────────────────────────

fn parse_target_deployables(s: &str) -> Vec<String> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    if let Ok(v) = serde_json::from_str::<Vec<String>>(s) {
        return v;
    }
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn build_plan_envelope(p: &plan::ComputedPlan) -> meshql_core::Envelope {
    let mut payload = meshql_core::Stash::new();
    payload.insert(
        "change_request_id".into(),
        serde_json::Value::String(p.change_request_id.clone()),
    );
    payload.insert("tier".into(), serde_json::Value::String(p.tier.clone()));
    payload.insert(
        "steps".into(),
        serde_json::Value::String(serde_json::to_string(&p.steps).unwrap_or_default()),
    );
    payload.insert(
        "blockers".into(),
        serde_json::Value::String(serde_json::to_string(&p.blockers).unwrap_or_default()),
    );
    payload.insert(
        "computed_at".into(),
        serde_json::Value::String(p.computed_at.clone()),
    );
    payload.insert(
        "summary".into(),
        serde_json::Value::String(p.change_request_summary.clone()),
    );
    meshql_core::Envelope::new(uuid_like(), payload, vec![])
}

fn parse_plan_payload(payload: &meshql_core::Stash) -> anyhow::Result<plan::ComputedPlan> {
    let steps_str = payload
        .get("steps")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");
    let steps: Vec<plan::PlanStep> = serde_json::from_str(steps_str)?;
    let blockers_str = payload
        .get("blockers")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");
    let blockers: Vec<plan::Blocker> = serde_json::from_str(blockers_str).unwrap_or_default();
    Ok(plan::ComputedPlan {
        change_request_id: payload
            .get("change_request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        change_request_summary: payload
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tier: payload
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        steps,
        blockers,
        computed_at: payload
            .get("computed_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

fn build_gantt_envelope(plan_id: &str, tier: &str, mermaid: &str) -> meshql_core::Envelope {
    let mut payload = meshql_core::Stash::new();
    payload.insert(
        "deployment_plan_id".into(),
        serde_json::Value::String(plan_id.to_string()),
    );
    payload.insert("tier".into(), serde_json::Value::String(tier.to_string()));
    payload.insert(
        "mermaid".into(),
        serde_json::Value::String(mermaid.to_string()),
    );
    meshql_core::Envelope::new(uuid_like(), payload, vec![])
}

/// UUID-shaped string without pulling in the `uuid` crate. Good enough for
/// primary keys; collisions are astronomically unlikely.
fn uuid_like() -> String {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let r = (now as u128).wrapping_mul(2_862_933_555_777_941_757_u128);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (r >> 96) as u32,
        ((r >> 80) & 0xffff) as u16,
        ((r >> 64) & 0xffff) as u16,
        ((r >> 48) & 0xffff) as u16,
        (r & 0xffffffffffff) as u64,
    )
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3002".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    let union_url = std::env::var("UNION_URL").unwrap_or_else(|_| "http://localhost:3001".into());

    // Cross-app public URLs — published via /config.json for the frontend.
    let groundwork_public_url = std::env::var("GROUNDWORK_PUBLIC_URL")
        .unwrap_or_else(|_| "https://groundwork.tildarc.com".into());
    let union_public_url =
        std::env::var("UNION_PUBLIC_URL").unwrap_or_else(|_| "https://union.tildarc.com".into());
    let lobby_public_url =
        std::env::var("LOBBY_PUBLIC_URL").unwrap_or_else(|_| "https://lobby.tildarc.com".into());
    let manifold_public_url = std::env::var("MANIFOLD_PUBLIC_URL").unwrap_or_else(|_| "/".into());

    let config_body = serde_json::json!({
        "groundwork_public_url": groundwork_public_url,
        "union_public_url":      union_public_url,
        "lobby_public_url":      lobby_public_url,
        "manifold_public_url":   manifold_public_url,
    })
    .to_string();

    // SQLite-only: the mongo build talks to Atlas and must not write to the
    // (read-only, on Lambda) filesystem.
    #[cfg(feature = "sqlite")]
    std::fs::create_dir_all(&data_dir)?;

    let org_node = make_entity(&data_dir, "org_node").await;
    let bylaw_e = make_entity(&data_dir, "bylaw").await;
    let change_request = make_entity(&data_dir, "change_request").await;
    let deployment_plan = make_entity(&data_dir, "deployment_plan").await;
    let gantt_output = make_entity(&data_dir, "gantt_output").await;

    let groundwork: Arc<dyn plan::GroundworkLookup> =
        Arc::new(groundwork_client::HttpGroundworkClient::from_env());

    let app_state = AppState {
        org_node: org_node.clone(),
        bylaw: bylaw_e.clone(),
        change_request: change_request.clone(),
        deployment_plan: deployment_plan.clone(),
        gantt_output: gantt_output.clone(),
        groundwork,
    };

    let org_node_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/org_node.schema.json"))
            .expect("invalid org_node schema JSON");
    let bylaw_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/bylaw.schema.json"))
            .expect("invalid bylaw schema JSON");
    let change_request_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/change_request.schema.json"))
            .expect("invalid change_request schema JSON");
    let deployment_plan_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/deployment_plan.schema.json"))
            .expect("invalid deployment_plan schema JSON");
    let gantt_output_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/gantt_output.schema.json"))
            .expect("invalid gantt_output schema JSON");

    let org_node_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector("getByParentId", r#"{"payload.parent_id": "{{parent_id}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .singleton_resolver(
            "team",
            Some("team_id"),
            "getById",
            format!("{}/team/graph", union_url),
        )
        .build();

    let bylaw_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByOrgNodeId",
            r#"{"payload.org_node_id": "{{org_node_id}}"}"#,
        )
        .vector("getByGateType", r#"{"payload.gate_type": "{{gate_type}}"}"#)
        .build();

    let change_request_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByStatus", r#"{"payload.status": "{{status}}"}"#)
        .vector("getByTier", r#"{"payload.tier": "{{tier}}"}"#)
        .build();

    let deployment_plan_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByChangeRequestId",
            r#"{"payload.change_request_id": "{{change_request_id}}"}"#,
        )
        .build();

    let gantt_output_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByDeploymentPlanId",
            r#"{"payload.deployment_plan_id": "{{deployment_plan_id}}"}"#,
        )
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![
            GraphletteConfig {
                path: "/org_node/graph".into(),
                schema_text: ORG_NODE_GRAPHQL.into(),
                root_config: org_node_root,
                searcher: org_node.searcher.clone(),
            },
            GraphletteConfig {
                path: "/bylaw/graph".into(),
                schema_text: BYLAW_GRAPHQL.into(),
                root_config: bylaw_root,
                searcher: bylaw_e.searcher.clone(),
            },
            GraphletteConfig {
                path: "/change_request/graph".into(),
                schema_text: CHANGE_REQUEST_GRAPHQL.into(),
                root_config: change_request_root,
                searcher: change_request.searcher.clone(),
            },
            GraphletteConfig {
                path: "/deployment_plan/graph".into(),
                schema_text: DEPLOYMENT_PLAN_GRAPHQL.into(),
                root_config: deployment_plan_root,
                searcher: deployment_plan.searcher.clone(),
            },
            GraphletteConfig {
                path: "/gantt_output/graph".into(),
                schema_text: GANTT_OUTPUT_GRAPHQL.into(),
                root_config: gantt_output_root,
                searcher: gantt_output.searcher.clone(),
            },
        ],
        restlettes: vec![],
    };

    // Edge-header auth — see specs/2026-05-12-trusted-header-auth-design.md.
    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id")).await?,
    );

    let org_node_restlette = meshql_server::build_restlette_router_ext(
        "/org_node/api",
        org_node.repo.clone(),
        auth.clone(),
        None,
        Some(bylaw::org_node_validator(&org_node_schema_json)),
        None,
        None,
    );
    let bylaw_restlette = meshql_server::build_restlette_router_ext(
        "/bylaw/api",
        bylaw_e.repo.clone(),
        auth.clone(),
        None,
        Some(bylaw::bylaw_validator(&bylaw_schema_json)),
        None,
        None,
    );
    let change_request_restlette = meshql_server::build_restlette_router_ext(
        "/change_request/api",
        change_request.repo.clone(),
        auth.clone(),
        None,
        Some(bylaw::base_schema_validator(&change_request_schema_json)),
        None,
        None,
    );
    let deployment_plan_restlette = meshql_server::build_restlette_router_ext(
        "/deployment_plan/api",
        deployment_plan.repo.clone(),
        auth.clone(),
        None,
        Some(bylaw::base_schema_validator(&deployment_plan_schema_json)),
        None,
        None,
    );
    let gantt_output_restlette = meshql_server::build_restlette_router_ext(
        "/gantt_output/api",
        gantt_output.repo.clone(),
        auth.clone(),
        None,
        Some(bylaw::base_schema_validator(&gantt_output_schema_json)),
        None,
        None,
    );

    let custom_routes = Router::new()
        .route("/org_node/:id/ancestors", get(get_ancestors))
        .route("/org_node/:id/effective_bylaws", get(get_effective_bylaws))
        .route("/change_request/:id/plan", post(post_change_request_plan))
        .route(
            "/deployment_plan/:id/gantt",
            post(post_deployment_plan_gantt),
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
        .route("/static/manifold-ui.css", get(serve_manifold_ui_css))
        .route("/static/favicon.png", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        .route("/static/manifold-ui.js", get(serve_manifold_ui_js))
        .route("/health", get(health_check))
        .merge(config_route)
        .merge(org_node_restlette)
        .merge(bylaw_restlette)
        .merge(change_request_restlette)
        .merge(deployment_plan_restlette)
        .merge(gantt_output_restlette)
        .merge(custom_routes);

    let app = meshql_server::build_app_with_auth(config, auth, extra).await?;
    let app = with_header_identity(app, HeaderConfig::from_env());

    // Read-through response cache (outermost; no-op unless CACHE_TTL_SECS set).
    let app = with_response_cache(app);
    #[cfg(feature = "lambda")]
    {
        let _ = port;
        lambda_http::run(app)
            .await
            .map_err(|e| anyhow::anyhow!("lambda runtime error: {e}"))?;
    }
    #[cfg(not(feature = "lambda"))]
    {
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
        println!("cityhall listening on port {port}");
        axum::serve(listener, app).await?;
    }
    Ok(())
}
