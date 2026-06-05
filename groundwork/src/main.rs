//! Groundwork — service catalog for the Manifold suite.

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
#[cfg(feature = "sqlite")]
use std::str::FromStr;
use std::sync::Arc;

const DEPLOYABLE_GRAPHQL: &str = include_str!("../config/graph/deployable.graphql");
const SERVICE_GRAPHQL: &str = include_str!("../config/graph/service.graphql");
const DEPENDENCY_GRAPHQL: &str = include_str!("../config/graph/dependency.graphql");
const EXPOSES_GRAPHQL: &str = include_str!("../config/graph/exposes.graphql");
const CONTRACT_GRAPHQL: &str = include_str!("../config/graph/contract.graphql");
const SLA_GRAPHQL: &str = include_str!("../config/graph/sla.graphql");
const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const AUTH_MODEL: &str = include_str!("../config/auth/model.conf");
const AUTH_POLICY: &str = include_str!("../config/auth/policy.csv");
// Vendored library, version-pinned at vendor time (cytoscape@3.30.2). Served
// with immutable cache headers — the URL only changes when we bump the file.
const CYTOSCAPE_JS: &str = include_str!("../static/vendor/cytoscape.min.js");

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

async fn serve_cytoscape_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        CYTOSCAPE_JS,
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

fn make_required_validator(schema: &serde_json::Value) -> ValidatorFn {
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
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    let union_url = std::env::var("UNION_URL").unwrap_or_else(|_| "http://localhost:3001".into());

    // Cross-app public URLs — published via /config.json for the frontend to
    // build cross-app <a href>s. These are *config*, not data. See memory
    // feedback_use_the_graph.md.
    let union_public_url =
        std::env::var("UNION_PUBLIC_URL").unwrap_or_else(|_| "https://union.tildarc.com".into());
    let cityhall_public_url = std::env::var("CITYHALL_PUBLIC_URL")
        .unwrap_or_else(|_| "https://cityhall.tildarc.com".into());
    let yard_public_url =
        std::env::var("YARD_PUBLIC_URL").unwrap_or_else(|_| "https://yard.tildarc.com".into());
    let lobby_public_url =
        std::env::var("LOBBY_PUBLIC_URL").unwrap_or_else(|_| "https://lobby.tildarc.com".into());
    let manifold_public_url = std::env::var("MANIFOLD_PUBLIC_URL").unwrap_or_else(|_| "/".into());

    let config_body = serde_json::json!({
        "union_public_url":     union_public_url,
        "cityhall_public_url":  cityhall_public_url,
        "yard_public_url":      yard_public_url,
        "lobby_public_url":     lobby_public_url,
        "manifold_public_url":  manifold_public_url,
    })
    .to_string();

    // SQLite-only: the mongo build talks to Atlas and must not write to the
    // (read-only, on Lambda) filesystem.
    #[cfg(feature = "sqlite")]
    std::fs::create_dir_all(&data_dir)?;

    let deployable = make_entity(&data_dir, "deployable").await;
    let service = make_entity(&data_dir, "service").await;
    let dependency = make_entity(&data_dir, "dependency").await;
    let exposes = make_entity(&data_dir, "exposes").await;
    let contract = make_entity(&data_dir, "contract").await;
    let sla = make_entity(&data_dir, "sla").await;

    let deployable_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/deployable.schema.json"))
            .expect("invalid deployable schema JSON");
    let service_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/service.schema.json"))
            .expect("invalid service schema JSON");
    let dependency_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/dependency.schema.json"))
            .expect("invalid dependency schema JSON");
    let exposes_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/exposes.schema.json"))
            .expect("invalid exposes schema JSON");
    let contract_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/contract.schema.json"))
            .expect("invalid contract schema JSON");
    let sla_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/sla.schema.json"))
            .expect("invalid sla schema JSON");

    let deployable_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .singleton_resolver(
            "team",
            Some("team_id"),
            "getById",
            format!("{}/team/graph", union_url),
        )
        .build();

    let service_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let dependency_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByDeployableId",
            r#"{"payload.deployable_id": "{{deployable_id}}"}"#,
        )
        .vector(
            "getByServiceId",
            r#"{"payload.service_id": "{{service_id}}"}"#,
        )
        .build();

    let exposes_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByDeployableId",
            r#"{"payload.deployable_id": "{{deployable_id}}"}"#,
        )
        .vector(
            "getByServiceId",
            r#"{"payload.service_id": "{{service_id}}"}"#,
        )
        .build();

    let contract_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByServiceId",
            r#"{"payload.service_id": "{{service_id}}"}"#,
        )
        .build();

    let sla_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByContractId",
            r#"{"payload.contract_id": "{{contract_id}}"}"#,
        )
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![
            GraphletteConfig {
                path: "/deployable/graph".into(),
                schema_text: DEPLOYABLE_GRAPHQL.into(),
                root_config: deployable_gql_config,
                searcher: deployable.searcher,
            },
            GraphletteConfig {
                path: "/service/graph".into(),
                schema_text: SERVICE_GRAPHQL.into(),
                root_config: service_gql_config,
                searcher: service.searcher,
            },
            GraphletteConfig {
                path: "/dependency/graph".into(),
                schema_text: DEPENDENCY_GRAPHQL.into(),
                root_config: dependency_gql_config,
                searcher: dependency.searcher,
            },
            GraphletteConfig {
                path: "/exposes/graph".into(),
                schema_text: EXPOSES_GRAPHQL.into(),
                root_config: exposes_gql_config,
                searcher: exposes.searcher,
            },
            GraphletteConfig {
                path: "/contract/graph".into(),
                schema_text: CONTRACT_GRAPHQL.into(),
                root_config: contract_gql_config,
                searcher: contract.searcher,
            },
            GraphletteConfig {
                path: "/sla/graph".into(),
                schema_text: SLA_GRAPHQL.into(),
                root_config: sla_gql_config,
                searcher: sla.searcher,
            },
        ],
        restlettes: vec![],
    };

    // Edge-header auth: Caddy injects trusted identity headers, manifold-edge
    // middleware lifts them into the request Stash, and CasbinAuth resolves
    // roles via the embedded policy. See specs/2026-05-12-trusted-header-auth-design.md.
    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id")).await?,
    );

    let deployable_restlette = meshql_server::build_restlette_router_ext(
        "/deployable/api",
        deployable.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&deployable_schema_json)),
        None,
        None,
    );
    let service_restlette = meshql_server::build_restlette_router_ext(
        "/service/api",
        service.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&service_schema_json)),
        None,
        None,
    );
    let dependency_restlette = meshql_server::build_restlette_router_ext(
        "/dependency/api",
        dependency.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&dependency_schema_json)),
        None,
        None,
    );
    let exposes_restlette = meshql_server::build_restlette_router_ext(
        "/exposes/api",
        exposes.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&exposes_schema_json)),
        None,
        None,
    );
    let contract_restlette = meshql_server::build_restlette_router_ext(
        "/contract/api",
        contract.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&contract_schema_json)),
        None,
        None,
    );
    let sla_restlette = meshql_server::build_restlette_router_ext(
        "/sla/api",
        sla.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&sla_schema_json)),
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
        .route("/static/vendor/cytoscape.min.js", get(serve_cytoscape_js))
        .route("/static/manifold-ui.css", get(serve_manifold_ui_css))
        .route("/static/favicon.png", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        .route("/static/manifold-ui.js", get(serve_manifold_ui_js))
        .route("/health", get(health_check))
        .merge(config_route)
        .merge(deployable_restlette)
        .merge(service_restlette)
        .merge(dependency_restlette)
        .merge(exposes_restlette)
        .merge(contract_restlette)
        .merge(sla_restlette);

    let app = meshql_server::build_app_with_auth(config, auth, extra).await?;
    // Apply the header-identity middleware to the FULL app so it covers
    // graphlette + restlette routes alike.
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
        println!("groundwork listening on port {port}");
        axum::serve(listener, app).await?;
    }
    Ok(())
}
