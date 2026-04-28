//! Groundwork — service catalog for the Manifold suite.

use axum::{
    http::header,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use meshql_core::{
    GraphletteConfig, NoAuth, RootConfig, ServerConfig, Stash,
};
use meshql_server::{ValidatorContext, ValidatorFn};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

const APPLICATION_GRAPHQL: &str = include_str!("../config/graph/application.graphql");
const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");

// ── Static handlers ───────────────────────────────────────────────────────────

async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn serve_app_js() -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
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

// ── Validation ────────────────────────────────────────────────────────────────

/// Build a ValidatorFn from a JSON Schema that enforces required fields.
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

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());

    std::fs::create_dir_all(&data_dir)?;

    let application = make_entity(&data_dir, "application").await;

    let application_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/application.schema.json"))
            .expect("invalid application schema JSON");

    // Graphlette via ServerConfig (federation resolver registry handled internally)
    let application_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![GraphletteConfig {
            path: "/application/graph".into(),
            schema_text: APPLICATION_GRAPHQL.into(),
            root_config: application_gql_config,
            searcher: application.searcher,
        }],
        restlettes: vec![], // built manually below to wire in JSON Schema validation
    };

    // Restlette with JSON Schema validation
    let validator = make_required_validator(&application_schema_json);
    let auth = Arc::new(NoAuth);
    let restlette = meshql_server::build_restlette_router_ext(
        "/application/api",
        application.repo,
        auth,
        None,           // no field defaults
        Some(validator),
        None,           // no post-create side effect
        None,
    );

    // Static UI + health check + validated restlette merged into extra
    let extra = Router::new()
        .route("/", get(serve_index))
        .route("/static/app.js", get(serve_app_js))
        .route("/health", get(health_check))
        .merge(restlette);

    meshql_server::run_ext(config, extra).await
}
