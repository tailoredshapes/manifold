use axum::{http::header, response::IntoResponse, routing::get, Router};
use cucumber::{World, given, then, when};
use meshql_core::{GraphletteConfig, NoAuth, RootConfig, ServerConfig, Stash};
use meshql_server::{ValidatorContext, ValidatorFn};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use reqwest::Client;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

const DEPLOYABLE_GRAPHQL: &str = include_str!("../config/graph/deployable.graphql");

#[derive(Debug, World)]
pub struct GroundworkWorld {
    pub server_addr: Option<String>,
    pub ids: HashMap<String, String>,
    pub timestamps: HashMap<String, f64>,
    pub last_response_status: Option<u16>,
    pub last_response_body: Option<String>,
    pub last_response_content_type: Option<String>,
    pub client: Client,
}

impl Default for GroundworkWorld {
    fn default() -> Self {
        Self {
            server_addr: None,
            ids: HashMap::new(),
            timestamps: HashMap::new(),
            last_response_status: None,
            last_response_body: None,
            last_response_content_type: None,
            client: Client::new(),
        }
    }
}

impl GroundworkWorld {
    pub fn base_url(&self) -> &str {
        self.server_addr.as_deref().expect("server not started")
    }

    pub fn resolve(&self, s: &str) -> String {
        let mut result = s.to_string();
        for (k, v) in &self.ids {
            result = result.replace(&format!("<ids.{k}>"), v);
        }
        for (k, v) in &self.timestamps {
            result = result.replace(&format!("<timestamps.{k}>"), &v.to_string());
        }
        result
    }

    fn store_response(&mut self, status: u16, body: String, content_type: Option<String>) {
        self.last_response_status = Some(status);
        self.last_response_body = Some(body);
        self.last_response_content_type = content_type;
    }
}

// ── Server bootstrap ─────────────────────────────────────────────────────────

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

async fn build_test_server() -> String {
    let pool = make_pool().await;
    let repo = Arc::new(SqliteRepository::new_with_pool(pool.clone()).await.unwrap());
    let searcher: Arc<dyn meshql_core::Searcher> =
        Arc::new(SqliteSearcher::new_with_pool(pool).await.unwrap());

    let root_config = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let schema_json: Value =
        serde_json::from_str(include_str!("../config/json/deployable.schema.json")).unwrap();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![GraphletteConfig {
            path: "/deployable/graph".into(),
            schema_text: DEPLOYABLE_GRAPHQL.into(),
            root_config,
            searcher,
        }],
        restlettes: vec![], // restlette with validation is in extra
    };

    // JSON Schema validator (same as main.rs)
    let schema_for_validator = schema_json.clone();
    let required: Vec<String> = schema_for_validator
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_default();
    let validator: ValidatorFn = Arc::new(move |payload: &Stash, _ctx: &ValidatorContext| {
        for field in &required {
            match payload.get(field.as_str()) {
                None => return Err(format!("Required field '{}' is missing", field)),
                Some(v) if v.as_str().map(|s| s.trim().is_empty()).unwrap_or(false) => {
                    return Err(format!("Required field '{}' cannot be empty", field))
                }
                _ => {}
            }
        }
        Ok(())
    });

    let restlette = meshql_server::build_restlette_router_ext(
        "/deployable/api",
        repo,
        Arc::new(NoAuth),
        None,
        Some(validator),
        None,
        None,
    );

    let extra = Router::new()
        .route("/", get(|| async { axum::response::Html(include_str!("../static/index.html")) }))
        .route("/static/app.js", get(|| async {
            ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], include_str!("../static/app.js")).into_response()
        }))
        .route("/health", get(|| async {
            ([(header::CONTENT_TYPE, "application/json")], r#"{"status":"ok"}"#).into_response()
        }))
        .merge(restlette);

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

// ── HTTP helpers ──────────────────────────────────────────────────────────────

async fn do_request(world: &mut GroundworkWorld, method: &str, path: &str, body: Option<Value>) {
    let url = format!("{}{}", world.base_url(), path);
    let builder = match method {
        "GET" => world.client.get(&url),
        "DELETE" => world.client.delete(&url),
        "POST" => world.client.post(&url).json(body.as_ref().unwrap_or(&Value::Null)),
        "PUT" => world.client.put(&url).json(body.as_ref().unwrap_or(&Value::Null)),
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

async fn register_deployable_raw(world: &mut GroundworkWorld, name: &str) -> String {
    let url = format!("{}/deployable/api", world.base_url());
    let resp = world
        .client
        .post(&url)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    assert!(
        status == 200 || status == 201,
        "Register '{name}' failed ({status}): {text}"
    );
    let parsed: Value = serde_json::from_str(&text).expect("response not JSON");
    parsed
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .expect("no id in response")
}

// ── Step definitions ──────────────────────────────────────────────────────────

#[given("a Groundwork server is running")]
async fn start_server(world: &mut GroundworkWorld) {
    let addr = build_test_server().await;
    world.server_addr = Some(addr);
    world.ids.clear();
    world.timestamps.clear();
}

#[given(regex = r#"^I have registered deployable "(.+)"$"#)]
async fn register_one_deployable(world: &mut GroundworkWorld, name: String) {
    let id = register_deployable_raw(world, &name).await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered deployables:$"#)]
async fn register_many_deployables(world: &mut GroundworkWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("expected a table");
    let mut pairs: Vec<(String, String)> = Vec::new();
    for row in table.rows.iter().skip(1) {
        let name = row[0].trim().to_string();
        let id = register_deployable_raw(world, &name).await;
        pairs.push((name, id));
    }
    for (name, id) in pairs {
        world.ids.insert(name, id);
    }
}

#[given(regex = r#"^I capture the current timestamp as "(.+)"$"#)]
async fn capture_timestamp(world: &mut GroundworkWorld, key: String) {
    // Sleep to ensure any prior operations have a strictly earlier timestamp
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    let ms = chrono::Utc::now().timestamp_millis() as f64;
    world.timestamps.insert(key, ms);
    // Sleep to ensure subsequent operations have a strictly later timestamp
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
}

#[given(regex = r#"^I update deployable "(.+)" with body (.+)$"#)]
async fn update_deployable_given(world: &mut GroundworkWorld, name: String, body_str: String) {
    let id = world.ids.get(&name).cloned().expect("deployable not registered");
    let path = format!("/deployable/api/{id}");
    let body: Value = serde_json::from_str(&body_str).expect("invalid JSON body");
    do_request(world, "PUT", &path, Some(body)).await;
}

#[when(regex = r#"^I (GET|DELETE) "(.+)"$"#)]
async fn http_get_delete(world: &mut GroundworkWorld, method: String, path: String) {
    let resolved = world.resolve(&path);
    do_request(world, &method, &resolved, None).await;
}

#[when(regex = r#"^I POST to "(.+)" with body (.+)$"#)]
async fn http_post(world: &mut GroundworkWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let body: Value = serde_json::from_str(&body_str).expect("invalid JSON body");
    do_request(world, "POST", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I PUT "(.+)" with body (.+)$"#)]
async fn http_put(world: &mut GroundworkWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let body: Value = serde_json::from_str(&body_str).expect("invalid JSON body");
    do_request(world, "PUT", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I query the "(.+)" graph with: (.+)$"#)]
async fn graphql_query(world: &mut GroundworkWorld, entity: String, query_str: String) {
    let resolved = world.resolve(&query_str);
    let path = format!("/{entity}/graph");
    let body = serde_json::json!({ "query": resolved });
    do_request(world, "POST", &path, Some(body)).await;
}

#[then(regex = r"^the response status should be (\d+)$")]
async fn check_status(world: &mut GroundworkWorld, expected: u16) {
    let actual = world.last_response_status.expect("no response recorded");
    assert_eq!(
        actual, expected,
        "Expected status {expected}, got {actual}. Body: {:?}",
        world.last_response_body
    );
}

#[then(regex = r#"^the response body should contain "(.+)"$"#)]
async fn body_contains(world: &mut GroundworkWorld, expected: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&expected),
        "Expected body to contain {expected:?}\nGot: {body}"
    );
}

#[then(r#"the response body should have an "id" field"#)]
async fn body_has_id(world: &mut GroundworkWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    assert!(
        parsed.get("id").map(|v| !v.is_null()).unwrap_or(false),
        "No 'id' field in: {body}"
    );
}

#[then("the response body should be a JSON array")]
async fn body_is_array(world: &mut GroundworkWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    assert!(parsed.is_array(), "Expected JSON array, got: {body}");
}

#[then(regex = r"^the response array should have (\d+) items$")]
async fn array_has_items(world: &mut GroundworkWorld, expected: usize) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let arr = serde_json::from_str::<Value>(body)
        .expect("not JSON")
        .as_array()
        .expect("not an array")
        .clone();
    assert_eq!(arr.len(), expected, "Expected {expected} items, got {}", arr.len());
}

#[then(regex = r#"^the response content-type should contain "(.+)"$"#)]
async fn check_content_type(world: &mut GroundworkWorld, expected: String) {
    let ct = world.last_response_content_type.as_deref().unwrap_or("");
    assert!(
        ct.contains(&expected),
        "Expected content-type to contain {expected:?}, got: {ct:?}"
    );
}

#[then("there should be no GraphQL errors")]
async fn no_graphql_errors(world: &mut GroundworkWorld) {
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
async fn response_data_contains(world: &mut GroundworkWorld, expected: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&expected),
        "Expected response data to contain {expected:?}\nGot: {body}"
    );
}

#[then("the response data description should be null")]
async fn response_data_description_null(world: &mut GroundworkWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    let desc = parsed
        .get("data")
        .and_then(|d| d.get("getById"))
        .and_then(|g| g.get("description"));
    assert!(
        desc.map(|v| v.is_null()).unwrap_or(true),
        "Expected description null, got: {desc:?}"
    );
}

#[tokio::main]
async fn main() {
    GroundworkWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features")
        .await;
}
