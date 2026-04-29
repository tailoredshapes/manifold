mod common;

use axum::{http::header, response::IntoResponse, routing::get, Router};
use common::stub_cityhall::{self, ChangeRequestRegistry, StubChangeRequest};
use common::stub_groundwork::{self, DeployableRegistry, StubDeployable};
use cucumber::{World, given, then, when};
use meshql_core::{GraphletteConfig, NoAuth, RootConfig, ServerConfig, Stash};
use meshql_server::{ValidatorContext, ValidatorFn};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use reqwest::Client;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;

const PERSON_GRAPHQL: &str = include_str!("../config/graph/person.graphql");
const TEAM_GRAPHQL: &str = include_str!("../config/graph/team.graphql");
const TEAM_MEMBER_GRAPHQL: &str = include_str!("../config/graph/team_member.graphql");
const WORK_ORDER_GRAPHQL: &str = include_str!("../config/graph/work_order.graphql");

#[derive(Debug, World)]
pub struct UnionWorld {
    pub server_addr: Option<String>,
    pub ids: HashMap<String, String>,
    pub last_response_status: Option<u16>,
    pub last_response_body: Option<String>,
    pub last_response_content_type: Option<String>,
    pub client: Client,
    pub deployables: Option<DeployableRegistry>,
    pub change_requests: Option<ChangeRequestRegistry>,
}

impl Default for UnionWorld {
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
        }
    }
}

impl UnionWorld {
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

struct TestEntity {
    repo: Arc<dyn meshql_core::Repository>,
    searcher: Arc<dyn meshql_core::Searcher>,
}

async fn make_entity() -> TestEntity {
    let pool = make_pool().await;
    let repo = Arc::new(SqliteRepository::new_with_pool(pool.clone()).await.unwrap());
    let searcher: Arc<dyn meshql_core::Searcher> =
        Arc::new(SqliteSearcher::new_with_pool(pool).await.unwrap());
    TestEntity { repo, searcher }
}

fn validator_for(schema_str: &str) -> ValidatorFn {
    let schema: Value = serde_json::from_str(schema_str).expect("invalid schema");
    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
        .unwrap_or_default();
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
                            arr.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
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
                    return Err(format!("Required field '{}' cannot be empty", field))
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

async fn build_test_server() -> (String, DeployableRegistry, ChangeRequestRegistry) {
    let person = make_entity().await;
    let team = make_entity().await;
    let team_member = make_entity().await;
    let work_order = make_entity().await;

    let (groundwork_url, deployable_registry) = stub_groundwork::spawn().await;
    let (cityhall_url, change_request_registry) = stub_cityhall::spawn().await;

    let person_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();

    let team_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .build();

    let team_member_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByPersonId", r#"{"payload.person_id": "{{person_id}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .build();

    let work_order_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
        .vector("getByChangeRequestId", r#"{"payload.change_request_id": "{{change_request_id}}"}"#)
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

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/person/graph".into(),
                schema_text: PERSON_GRAPHQL.into(),
                root_config: person_root,
                searcher: person.searcher.clone(),
            },
            GraphletteConfig {
                path: "/team/graph".into(),
                schema_text: TEAM_GRAPHQL.into(),
                root_config: team_root,
                searcher: team.searcher.clone(),
            },
            GraphletteConfig {
                path: "/team_member/graph".into(),
                schema_text: TEAM_MEMBER_GRAPHQL.into(),
                root_config: team_member_root,
                searcher: team_member.searcher.clone(),
            },
            GraphletteConfig {
                path: "/work_order/graph".into(),
                schema_text: WORK_ORDER_GRAPHQL.into(),
                root_config: work_order_root,
                searcher: work_order.searcher.clone(),
            },
        ],
        restlettes: vec![],
    };

    let auth = Arc::new(NoAuth);

    let person_restlette = meshql_server::build_restlette_router_ext(
        "/person/api",
        person.repo,
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/person.schema.json"))),
        None,
        None,
    );
    let team_restlette = meshql_server::build_restlette_router_ext(
        "/team/api",
        team.repo,
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/team.schema.json"))),
        None,
        None,
    );
    let team_member_restlette = meshql_server::build_restlette_router_ext(
        "/team_member/api",
        team_member.repo,
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/team_member.schema.json"))),
        None,
        None,
    );
    let work_order_restlette = meshql_server::build_restlette_router_ext(
        "/work_order/api",
        work_order.repo,
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/work_order.schema.json"))),
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
        .merge(person_restlette)
        .merge(team_restlette)
        .merge(team_member_restlette)
        .merge(work_order_restlette);

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
        deployable_registry,
        change_request_registry,
    )
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

async fn do_request(world: &mut UnionWorld, method: &str, path: &str, body: Option<Value>) {
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

async fn post_for_id(world: &mut UnionWorld, path: &str, payload: serde_json::Value) -> String {
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

// ── Step definitions ──────────────────────────────────────────────────────────

#[given("a Union server is running")]
async fn start_server(world: &mut UnionWorld) {
    let (addr, deployables, change_requests) = build_test_server().await;
    world.server_addr = Some(addr);
    world.deployables = Some(deployables);
    world.change_requests = Some(change_requests);
    world.ids.clear();
}

#[given(regex = r#"^I capture the last id as "(.+)"$"#)]
async fn capture_last_id(world: &mut UnionWorld, label: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    let id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no id in response: {body}"))
        .to_string();
    world.ids.insert(label, id);
}

#[given(regex = r#"^the Groundwork stub knows deployable "(.+)" as "(.+)"$"#)]
async fn groundwork_stub_knows_deployable(
    world: &mut UnionWorld,
    id: String,
    name: String,
) {
    let reg = world.deployables.as_ref().expect("Groundwork stub not started");
    reg.insert(StubDeployable {
        id,
        name,
        team_id: None,
        repo_url: None,
        description: None,
    });
}

#[given(regex = r#"^the Cityhall stub knows change request "(.+)" with summary "(.+)"$"#)]
async fn cityhall_stub_knows_change_request(
    world: &mut UnionWorld,
    id: String,
    summary: String,
) {
    let reg = world.change_requests.as_ref().expect("Cityhall stub not started");
    reg.insert(StubChangeRequest {
        id,
        summary,
        status: Some("submitted".into()),
        tier: Some("prod".into()),
    });
}

#[given(regex = r#"^I have registered person "(.+)"$"#)]
async fn register_one_person(world: &mut UnionWorld, name: String) {
    let id = post_for_id(world, "/person/api", serde_json::json!({"name": name})).await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered team "(.+)" with kind "(.+)"$"#)]
async fn register_team_with_kind(world: &mut UnionWorld, name: String, kind: String) {
    let id = post_for_id(
        world,
        "/team/api",
        serde_json::json!({"name": name, "kind": kind}),
    )
    .await;
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have opened work order "(.+)" against "(.+)"$"#)]
async fn open_work_order(world: &mut UnionWorld, label: String, team_name: String) {
    let team_id = world
        .ids
        .get(&team_name)
        .cloned()
        .unwrap_or_else(|| panic!("team {team_name} not registered"));
    let id = post_for_id(
        world,
        "/work_order/api",
        serde_json::json!({"team_id": team_id, "summary": label.clone()}),
    )
    .await;
    world.ids.insert(label, id);
}

#[when(regex = r#"^I (GET|DELETE) "(.+)"$"#)]
async fn http_get_delete(world: &mut UnionWorld, method: String, path: String) {
    let resolved = world.resolve(&path);
    do_request(world, &method, &resolved, None).await;
}

#[when(regex = r#"^I POST to "(.+)" with body (.+)$"#)]
async fn http_post(world: &mut UnionWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let resolved_body = world.resolve(&body_str);
    let body: Value = serde_json::from_str(&resolved_body).expect("invalid JSON body");
    do_request(world, "POST", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I PUT "(.+)" with body (.+)$"#)]
async fn http_put(world: &mut UnionWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let resolved_body = world.resolve(&body_str);
    let body: Value = serde_json::from_str(&resolved_body).expect("invalid JSON body");
    do_request(world, "PUT", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I query the "(.+)" graph with: (.+)$"#)]
async fn graphql_query(world: &mut UnionWorld, entity: String, query_str: String) {
    let resolved = world.resolve(&query_str);
    let path = format!("/{entity}/graph");
    let body = serde_json::json!({ "query": resolved });
    do_request(world, "POST", &path, Some(body)).await;
}

#[then(regex = r"^the response status should be (\d+)$")]
async fn check_status(world: &mut UnionWorld, expected: u16) {
    let actual = world.last_response_status.expect("no response recorded");
    assert_eq!(
        actual, expected,
        "Expected status {expected}, got {actual}. Body: {:?}",
        world.last_response_body
    );
}

#[then(regex = r#"^the response body should contain "(.+)"$"#)]
async fn body_contains(world: &mut UnionWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&resolved),
        "Expected body to contain {resolved:?}\nGot: {body}"
    );
}

#[then(regex = r#"^the response body should not contain "(.+)"$"#)]
async fn body_does_not_contain(world: &mut UnionWorld, unexpected: String) {
    let resolved = world.resolve(&unexpected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        !body.contains(&resolved),
        "Expected body NOT to contain {resolved:?}\nGot: {body}"
    );
}

#[then(r#"the response body should have an "id" field"#)]
async fn body_has_id(world: &mut UnionWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    assert!(
        parsed.get("id").map(|v| !v.is_null()).unwrap_or(false),
        "No 'id' field in: {body}"
    );
}

#[then("the response body should be a JSON array")]
async fn body_is_array(world: &mut UnionWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    assert!(parsed.is_array(), "Expected JSON array, got: {body}");
}

#[then(regex = r#"^the response content-type should contain "(.+)"$"#)]
async fn check_content_type(world: &mut UnionWorld, expected: String) {
    let ct = world.last_response_content_type.as_deref().unwrap_or("");
    assert!(
        ct.contains(&expected),
        "Expected content-type to contain {expected:?}, got: {ct:?}"
    );
}

#[then("there should be no GraphQL errors")]
async fn no_graphql_errors(world: &mut UnionWorld) {
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
async fn response_data_contains(world: &mut UnionWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        body.contains(&resolved),
        "Expected response data to contain {resolved:?}\nGot: {body}"
    );
}

#[then(regex = r#"^the response data should not contain "(.+)"$"#)]
async fn response_data_does_not_contain(world: &mut UnionWorld, unexpected: String) {
    let resolved = world.resolve(&unexpected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(
        !body.contains(&resolved),
        "Expected response data NOT to contain {resolved:?}\nGot: {body}"
    );
}

#[tokio::main]
async fn main() {
    UnionWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features")
        .await;
}
