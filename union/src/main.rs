//! Union — people, teams, and work orders for the Manifold suite.

use axum::{
    http::header,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use manifold_edge::{with_header_identity, with_response_cache, HeaderConfig};
use meshql_casbin::CasbinAuth;
use meshql_core::{Auth, GraphletteConfig, RootConfig, ServerConfig, Stash, StashKeyAuth};
use meshql_server::{ValidatorContext, ValidatorFn};
#[cfg(feature = "sqlite")]
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
#[cfg(feature = "mongo")]
use meshql_mongo::{MongoRepository, MongoSearcher};
use std::collections::BTreeMap;
#[cfg(feature = "sqlite")]
use std::str::FromStr;
use std::sync::Arc;

const PERSON_GRAPHQL: &str = include_str!("../config/graph/person.graphql");
const TEAM_GRAPHQL: &str = include_str!("../config/graph/team.graphql");
const TEAM_MEMBER_GRAPHQL: &str = include_str!("../config/graph/team_member.graphql");
const WORK_ORDER_GRAPHQL: &str = include_str!("../config/graph/work_order.graphql");
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

// ── Validation ────────────────────────────────────────────────────────────────

/// Validator that enforces required fields *and* JSON-Schema string enums.
/// Required-field semantics match Groundwork's: missing or empty-string ⇒ reject.
fn make_schema_validator(schema: &serde_json::Value) -> ValidatorFn {
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

    // Map of field-name → list of allowed string values.
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
                    return Err(format!("Required field '{}' cannot be empty", field));
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

// ── Storage bootstrap ─────────────────────────────────────────────────────────

struct Entity {
    repo: Arc<dyn meshql_core::Repository>,
    searcher: Arc<dyn meshql_core::Searcher>,
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

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    let groundwork_url =
        std::env::var("GROUNDWORK_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let cityhall_url =
        std::env::var("CITYHALL_URL").unwrap_or_else(|_| "http://localhost:3002".into());

    // Cross-app public URLs — published via /config.json for the frontend.
    let groundwork_public_url = std::env::var("GROUNDWORK_PUBLIC_URL")
        .unwrap_or_else(|_| "https://groundwork.tildarc.com".into());
    let cityhall_public_url = std::env::var("CITYHALL_PUBLIC_URL")
        .unwrap_or_else(|_| "https://cityhall.tildarc.com".into());
    let lobby_public_url =
        std::env::var("LOBBY_PUBLIC_URL").unwrap_or_else(|_| "https://lobby.tildarc.com".into());
    let manifold_public_url = std::env::var("MANIFOLD_PUBLIC_URL").unwrap_or_else(|_| "/".into());

    let config_body = serde_json::json!({
        "groundwork_public_url": groundwork_public_url,
        "cityhall_public_url":   cityhall_public_url,
        "lobby_public_url":      lobby_public_url,
        "manifold_public_url":   manifold_public_url,
    })
    .to_string();

    // SQLite-only: the mongo build talks to Atlas and must not write to the
    // (read-only, on Lambda) filesystem.
    #[cfg(feature = "sqlite")]
    std::fs::create_dir_all(&data_dir)?;

    let person = make_entity(&data_dir, "person").await;
    let team = make_entity(&data_dir, "team").await;
    let team_member = make_entity(&data_dir, "team_member").await;
    let work_order = make_entity(&data_dir, "work_order").await;

    let person_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/person.schema.json"))
            .expect("invalid person schema JSON");
    let team_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/team.schema.json"))
            .expect("invalid team schema JSON");
    let team_member_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/team_member.schema.json"))
            .expect("invalid team_member schema JSON");
    let work_order_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/work_order.schema.json"))
            .expect("invalid work_order schema JSON");

    let person_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let team_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .build();

    let team_member_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByPersonId", r#"{"payload.person_id": "{{person_id}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .build();

    let work_order_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .vector(
            "getByDeployableId",
            r#"{"payload.deployable_id": "{{deployable_id}}"}"#,
        )
        .vector(
            "getByChangeRequestId",
            r#"{"payload.change_request_id": "{{change_request_id}}"}"#,
        )
        .vector("getByStatus", r#"{"payload.status": "{{status}}"}"#)
        .singleton_resolver(
            "deployable",
            Some("deployable_id"),
            "getById",
            format!("{}/deployable/graph", groundwork_url),
        )
        .singleton_resolver(
            "change_request",
            Some("change_request_id"),
            "getById",
            format!("{}/change_request/graph", cityhall_url),
        )
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![
            GraphletteConfig {
                path: "/person/graph".into(),
                schema_text: PERSON_GRAPHQL.into(),
                root_config: person_gql_config,
                searcher: person.searcher,
            },
            GraphletteConfig {
                path: "/team/graph".into(),
                schema_text: TEAM_GRAPHQL.into(),
                root_config: team_gql_config,
                searcher: team.searcher,
            },
            GraphletteConfig {
                path: "/team_member/graph".into(),
                schema_text: TEAM_MEMBER_GRAPHQL.into(),
                root_config: team_member_gql_config,
                searcher: team_member.searcher,
            },
            GraphletteConfig {
                path: "/work_order/graph".into(),
                schema_text: WORK_ORDER_GRAPHQL.into(),
                root_config: work_order_gql_config,
                searcher: work_order.searcher,
            },
        ],
        restlettes: vec![],
    };

    // Edge-header auth — see specs/2026-05-12-trusted-header-auth-design.md.
    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id")).await?,
    );

    let person_restlette = meshql_server::build_restlette_router_ext(
        "/person/api",
        person.repo,
        auth.clone(),
        None,
        Some(make_schema_validator(&person_schema_json)),
        None,
        None,
    );
    let team_restlette = meshql_server::build_restlette_router_ext(
        "/team/api",
        team.repo,
        auth.clone(),
        None,
        Some(make_schema_validator(&team_schema_json)),
        None,
        None,
    );
    let team_member_restlette = meshql_server::build_restlette_router_ext(
        "/team_member/api",
        team_member.repo,
        auth.clone(),
        None,
        Some(make_schema_validator(&team_member_schema_json)),
        None,
        None,
    );
    let work_order_restlette = meshql_server::build_restlette_router_ext(
        "/work_order/api",
        work_order.repo,
        auth.clone(),
        None,
        Some(make_schema_validator(&work_order_schema_json)),
        None,
        None,
    );

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
        .merge(person_restlette)
        .merge(team_restlette)
        .merge(team_member_restlette)
        .merge(work_order_restlette);

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
        println!("union listening on port {port}");
        axum::serve(listener, app).await?;
    }
    Ok(())
}
