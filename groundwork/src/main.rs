//! Groundwork — service catalog for the Manifold suite.

use meshql_core::{GraphletteConfig, RestletteConfig, RootConfig, ServerConfig};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

const APPLICATION_GRAPHQL: &str = include_str!("../config/graph/application.graphql");

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

    let application_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![GraphletteConfig {
            path: "/application/graph".into(),
            schema_text: APPLICATION_GRAPHQL.into(),
            root_config: application_config,
            searcher: application.searcher,
        }],
        restlettes: vec![RestletteConfig {
            path: "/application/api".into(),
            schema_json: application_schema_json,
            repository: application.repo,
        }],
    };

    meshql_server::run(config).await
}
