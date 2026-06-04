//! manifold-ingest HTTP service — one entity (`Ingestion`) recording the
//! provenance of every primary-domain write made by an adapter or
//! LLM-driven import.

use axum::{
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use manifold_edge::{with_header_identity, HeaderConfig};
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

const INGESTION_GRAPHQL: &str = include_str!("../config/graph/ingestion.graphql");
const AUTH_MODEL: &str = include_str!("../config/auth/model.conf");
const AUTH_POLICY: &str = include_str!("../config/auth/policy.csv");

async fn health_check() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"ok"}"#,
    )
        .into_response()
}

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    std::fs::create_dir_all(&data_dir)?;

    let ingestion = make_entity(&data_dir, "ingestion").await;

    let ingestion_schema_json: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/ingestion.schema.json"))
            .expect("invalid ingestion schema JSON");

    let ingestion_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByExternalSystem",
            r#"{"payload.external_system": "{{external_system}}"}"#,
        )
        .vector(
            "getByCanonicalId",
            r#"{"payload.canonical_id": "{{canonical_id}}"}"#,
        )
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![GraphletteConfig {
            path: "/ingestion/graph".into(),
            schema_text: INGESTION_GRAPHQL.into(),
            root_config: ingestion_gql_config,
            searcher: ingestion.searcher,
        }],
        restlettes: vec![],
    };

    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id")).await?,
    );

    let ingestion_restlette = meshql_server::build_restlette_router_ext(
        "/ingestion/api",
        ingestion.repo,
        auth.clone(),
        None,
        Some(make_required_validator(&ingestion_schema_json)),
        None,
        None,
    );

    let extra = Router::new()
        .route("/health", get(health_check))
        .merge(ingestion_restlette);

    let app = meshql_server::build_app_with_auth(config, auth, extra).await?;
    let app = with_header_identity(app, HeaderConfig::from_env());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("manifold-ingest listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}
