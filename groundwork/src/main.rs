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

const DEPLOYABLE_GRAPHQL: &str = include_str!("../config/graph/deployable.graphql");
const SERVICE_GRAPHQL: &str = include_str!("../config/graph/service.graphql");
const DEPENDENCY_GRAPHQL: &str = include_str!("../config/graph/dependency.graphql");
const CONTRACT_GRAPHQL: &str = include_str!("../config/graph/contract.graphql");
const SLA_GRAPHQL: &str = include_str!("../config/graph/sla.graphql");
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

    let deployable = make_entity(&data_dir, "deployable").await;
    let service = make_entity(&data_dir, "service").await;
    let dependency = make_entity(&data_dir, "dependency").await;
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
        .build();

    let service_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let dependency_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
        .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
        .build();

    let contract_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
        .build();

    let sla_gql_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByContractId", r#"{"payload.contract_id": "{{contract_id}}"}"#)
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

    let auth = Arc::new(NoAuth);

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

    let extra = Router::new()
        .route("/", get(serve_index))
        .route("/static/app.js", get(serve_app_js))
        .route("/health", get(health_check))
        .merge(deployable_restlette)
        .merge(service_restlette)
        .merge(dependency_restlette)
        .merge(contract_restlette)
        .merge(sla_restlette);

    meshql_server::run_ext(config, extra).await
}
