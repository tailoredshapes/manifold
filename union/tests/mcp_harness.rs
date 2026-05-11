//! BDD harness for the `union-mcp` binary.
//!
//! Spawns the binary as a subprocess (built via `CARGO_BIN_EXE_union-mcp`)
//! and exchanges line-delimited JSON-RPC frames over its stdin/stdout. The
//! `UNION_URL` env var points the MCP child at an in-process Union HTTP
//! server (REST + GraphQL) bound to a random port. Each entity's restlette
//! repository and graphlette searcher share a single SQLite pool so reads
//! via `/graph` (used by auto-derived catalog capabilities) see the same
//! rows the test wrote via `/api`.

use cucumber::{given, then, when, World};
use meshql_core::{GraphletteConfig, NoAuth, RootConfig, ServerConfig, Stash};
use meshql_server::{ValidatorContext, ValidatorFn};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use reqwest::Client;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

const PERSON_GRAPHQL: &str = include_str!("../config/graph/person.graphql");
const TEAM_GRAPHQL: &str = include_str!("../config/graph/team.graphql");
const TEAM_MEMBER_GRAPHQL: &str = include_str!("../config/graph/team_member.graphql");
const WORK_ORDER_GRAPHQL: &str = include_str!("../config/graph/work_order.graphql");

// ── In-process Union HTTP server (REST + GraphQL) ────────────────────────────

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

async fn build_server() -> String {
    let person = make_entity().await;
    let team = make_entity().await;
    let team_member = make_entity().await;
    let work_order = make_entity().await;

    let auth = Arc::new(NoAuth);

    let person_restlette = meshql_server::build_restlette_router_ext(
        "/person/api",
        person.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/person.schema.json"))),
        None,
        None,
    );
    let team_restlette = meshql_server::build_restlette_router_ext(
        "/team/api",
        team.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/team.schema.json"))),
        None,
        None,
    );
    let team_member_restlette = meshql_server::build_restlette_router_ext(
        "/team_member/api",
        team_member.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/team_member.schema.json"))),
        None,
        None,
    );
    let work_order_restlette = meshql_server::build_restlette_router_ext(
        "/work_order/api",
        work_order.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/work_order.schema.json"))),
        None,
        None,
    );

    let extra = axum::Router::new()
        .merge(person_restlette)
        .merge(team_restlette)
        .merge(team_member_restlette)
        .merge(work_order_restlette);

    // GraphQL surfaces for the four entities. Templates mirror union's
    // production main.rs (minus the federated singleton resolvers on work_order,
    // which would require running Groundwork/Cityhall sidecars in the harness).
    let person_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let team_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .build();
    let team_member_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByPersonId", r#"{"payload.person_id": "{{person_id}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .build();
    let work_order_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
        .vector(
            "getByChangeRequestId",
            r#"{"payload.change_request_id": "{{change_request_id}}"}"#,
        )
        .vector("getByStatus", r#"{"payload.status": "{{status}}"}"#)
        .build();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/person/graph".into(),
                schema_text: PERSON_GRAPHQL.into(),
                root_config: person_gql,
                searcher: person.searcher,
            },
            GraphletteConfig {
                path: "/team/graph".into(),
                schema_text: TEAM_GRAPHQL.into(),
                root_config: team_gql,
                searcher: team.searcher,
            },
            GraphletteConfig {
                path: "/team_member/graph".into(),
                schema_text: TEAM_MEMBER_GRAPHQL.into(),
                root_config: team_member_gql,
                searcher: team_member.searcher,
            },
            GraphletteConfig {
                path: "/work_order/graph".into(),
                schema_text: WORK_ORDER_GRAPHQL.into(),
                root_config: work_order_gql,
                searcher: work_order.searcher,
            },
        ],
        restlettes: vec![],
    };

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

// ── MCP subprocess client ────────────────────────────────────────────────────

struct McpClient {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient").finish()
    }
}

impl McpClient {
    async fn spawn(server_url: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_union-mcp"))
            .env("UNION_URL", server_url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn union-mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            _child: child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&frame).unwrap();
        self.stdin.write_all(line.as_bytes()).await.unwrap();
        self.stdin.write_all(b"\n").await.unwrap();
        self.stdin.flush().await.unwrap();

        let mut buf = String::new();
        self.stdout.read_line(&mut buf).await.unwrap();
        let resp: Value = serde_json::from_str(&buf)
            .unwrap_or_else(|e| panic!("MCP response was not JSON: {e}\nbody: {buf}"));
        resp
    }
}

// ── World ────────────────────────────────────────────────────────────────────

#[derive(Debug, World)]
#[world(init = Self::default_world)]
struct McpWorld {
    server_url: Option<String>,
    mcp: Option<McpClient>,
    ids: HashMap<String, String>,
    last_response: Option<Value>,
    last_tool_result: Option<Value>,
    http: Client,
}

impl std::default::Default for McpWorld {
    fn default() -> Self {
        Self {
            server_url: None,
            mcp: None,
            ids: HashMap::new(),
            last_response: None,
            last_tool_result: None,
            http: Client::new(),
        }
    }
}

impl McpWorld {
    fn default_world() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self, std::convert::Infallible>> + Send>,
    > {
        Box::pin(async { Ok(Self::default()) })
    }

    fn server_url(&self) -> &str {
        self.server_url.as_deref().expect("server not started")
    }

    fn mcp_mut(&mut self) -> &mut McpClient {
        self.mcp.as_mut().expect("MCP client not started")
    }

    fn resolve(&self, s: &str) -> String {
        let mut out = s.to_string();
        for (k, v) in &self.ids {
            out = out.replace(&format!("<ids.{k}>"), v);
        }
        out
    }

    fn structured_result(&self) -> &Value {
        self.last_tool_result
            .as_ref()
            .expect("no tool result captured")
    }

    fn structured_array(&self) -> Vec<Value> {
        let r = self.structured_result();
        for key in ["getAll", "getByName", "getByKind", "getByTeamId", "getByPersonId"] {
            if let Some(arr) = r.get(key).and_then(|v| v.as_array()) {
                return arr.clone();
            }
        }
        if let Some(arr) = r.as_array() {
            return arr.clone();
        }
        if let Some(arr) = r.get("result").and_then(|v| v.as_array()) {
            return arr.clone();
        }
        panic!("expected array (raw or GraphQL-wrapped under getAll/etc), got: {r}");
    }
}

// ── Step definitions ─────────────────────────────────────────────────────────

#[given("a Union server is running")]
async fn start_server(world: &mut McpWorld) {
    world.server_url = Some(build_server().await);
}

#[given("the MCP binary is started against the server")]
async fn start_mcp(world: &mut McpWorld) {
    let url = world.server_url().to_string();
    let mut mcp = McpClient::spawn(&url).await;
    let _ = mcp
        .call(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" },
            }),
        )
        .await;
    world.mcp = Some(mcp);
}

#[given(regex = r#"^I have registered team "([^"]+)" of kind "([^"]+)"$"#)]
async fn register_team(world: &mut McpWorld, name: String, kind: String) {
    let url = format!("{}/team/api", world.server_url());
    let resp = world
        .http
        .post(&url)
        .json(&serde_json::json!({ "name": name, "kind": kind }))
        .send()
        .await
        .unwrap();
    let env: Value = resp.json().await.unwrap();
    let id = env.get("id").and_then(|v| v.as_str()).unwrap().to_string();
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered person "([^"]+)" with role "([^"]+)"$"#)]
async fn register_person(world: &mut McpWorld, name: String, role: String) {
    let url = format!("{}/person/api", world.server_url());
    let resp = world
        .http
        .post(&url)
        .json(&serde_json::json!({ "name": name, "role": role }))
        .send()
        .await
        .unwrap();
    let env: Value = resp.json().await.unwrap();
    let id = env.get("id").and_then(|v| v.as_str()).unwrap().to_string();
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have placed "([^"]+)" on team "([^"]+)" as "([^"]+)"$"#)]
async fn place_on_team(world: &mut McpWorld, person: String, team: String, role: String) {
    let person_id = world.ids.get(&person).cloned().expect("person id");
    let team_id = world.ids.get(&team).cloned().expect("team id");
    let url = format!("{}/team_member/api", world.server_url());
    world
        .http
        .post(&url)
        .json(&serde_json::json!({
            "person_id": person_id,
            "team_id": team_id,
            "role": role,
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[given(
    regex = r#"^I have filed work order "([^"]+)" for team "([^"]+)" worth (\d+) points with status "([^"]+)"$"#
)]
async fn file_work_order(
    world: &mut McpWorld,
    summary: String,
    team: String,
    points: u64,
    status: String,
) {
    let team_id = world.ids.get(&team).cloned().expect("team id");
    let url = format!("{}/work_order/api", world.server_url());
    let resp = world
        .http
        .post(&url)
        .json(&serde_json::json!({
            "team_id": team_id,
            "summary": summary,
            "status": status,
            "story_points": points,
        }))
        .send()
        .await
        .unwrap();
    let env: Value = resp.json().await.unwrap();
    let id = env.get("id").and_then(|v| v.as_str()).unwrap().to_string();
    world.ids.insert(summary, id);
}

#[when(regex = r#"^I send MCP request "([^"]+)"$"#)]
async fn send_mcp_method(world: &mut McpWorld, method: String) {
    let resp = world.mcp_mut().call(&method, serde_json::json!({})).await;
    world.last_response = Some(resp);
}

#[when(regex = r#"^I call MCP tool "([^"]+)" with arguments (.+)$"#)]
async fn call_mcp_tool(world: &mut McpWorld, name: String, args_str: String) {
    let resolved = world.resolve(&args_str);
    let args: Value = serde_json::from_str(&resolved).expect("invalid JSON arguments");
    let resp = world
        .mcp_mut()
        .call(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": args }),
        )
        .await;
    if let Some(err) = resp.get("error") {
        panic!("tool call returned JSON-RPC error: {err}");
    }
    let result = resp.get("result").cloned().unwrap_or(Value::Null);
    let structured = result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| result.clone());
    world.last_tool_result = Some(structured);
    world.last_response = Some(result);
}

#[then(regex = r#"^the response should include tool "([^"]+)"$"#)]
async fn response_includes_tool(world: &mut McpWorld, name: String) {
    let resp = world.last_response.as_ref().expect("no response");
    let tools = resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .or_else(|| resp.get("tools"))
        .and_then(|t| t.as_array())
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(
        names.iter().any(|n| *n == name),
        "tool {name} not in {names:?}"
    );
}

#[then(regex = r#"^the tool result should be a JSON array of at least (\d+) records$"#)]
async fn tool_result_at_least(world: &mut McpWorld, n: usize) {
    let arr = world.structured_array();
    assert!(
        arr.len() >= n,
        "expected at least {n} records, got {} in {:?}",
        arr.len(),
        arr
    );
}

#[then(regex = r#"^the tool result should report (\w+) (-?\d+)$"#)]
async fn report_field_eq(world: &mut McpWorld, field: String, expected: i64) {
    let r = world.structured_result();
    let actual = r
        .get(&field)
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| panic!("field {field} missing or not an int in {r}"));
    assert_eq!(actual, expected, "{field}: {r}");
}

#[tokio::main]
async fn main() {
    McpWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features/union_mcp.feature")
        .await;
}
