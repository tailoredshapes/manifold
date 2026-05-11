//! BDD harness for the `groundwork-mcp` binary.
//!
//! Spawns the binary as a subprocess (built via `CARGO_BIN_EXE_groundwork-mcp`)
//! and exchanges line-delimited JSON-RPC frames over its stdin/stdout. The
//! `GROUNDWORK_URL` env var points the MCP child at an in-process Groundwork
//! HTTP server (REST + GraphQL) bound to a random port. Each entity's
//! restlette repository and graphlette searcher share a single SQLite pool
//! so reads via `/graph` see the same rows the test wrote via `/api`.

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

const DEPLOYABLE_GRAPHQL: &str = include_str!("../config/graph/deployable.graphql");
const SERVICE_GRAPHQL: &str = include_str!("../config/graph/service.graphql");
const DEPENDENCY_GRAPHQL: &str = include_str!("../config/graph/dependency.graphql");
const EXPOSES_GRAPHQL: &str = include_str!("../config/graph/exposes.graphql");
const CONTRACT_GRAPHQL: &str = include_str!("../config/graph/contract.graphql");
const SLA_GRAPHQL: &str = include_str!("../config/graph/sla.graphql");

// ── In-process Groundwork HTTP server (REST + GraphQL) ───────────────────────

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
    let deployable = make_entity().await;
    let service = make_entity().await;
    let dependency = make_entity().await;
    let exposes = make_entity().await;
    let contract = make_entity().await;
    let sla = make_entity().await;

    let auth = Arc::new(NoAuth);

    let deployable_restlette = meshql_server::build_restlette_router_ext(
        "/deployable/api",
        deployable.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/deployable.schema.json"))),
        None,
        None,
    );
    let service_restlette = meshql_server::build_restlette_router_ext(
        "/service/api",
        service.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/service.schema.json"))),
        None,
        None,
    );
    let dependency_restlette = meshql_server::build_restlette_router_ext(
        "/dependency/api",
        dependency.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/dependency.schema.json"))),
        None,
        None,
    );
    let exposes_restlette = meshql_server::build_restlette_router_ext(
        "/exposes/api",
        exposes.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/exposes.schema.json"))),
        None,
        None,
    );
    let contract_restlette = meshql_server::build_restlette_router_ext(
        "/contract/api",
        contract.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/contract.schema.json"))),
        None,
        None,
    );
    let sla_restlette = meshql_server::build_restlette_router_ext(
        "/sla/api",
        sla.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/sla.schema.json"))),
        None,
        None,
    );

    let extra = axum::Router::new()
        .merge(deployable_restlette)
        .merge(service_restlette)
        .merge(dependency_restlette)
        .merge(exposes_restlette)
        .merge(contract_restlette)
        .merge(sla_restlette);

    // GraphQL surfaces for the six entities. Templates mirror groundwork's
    // production main.rs so the auto-derived MCP capabilities see the same
    // query shape. No federated singleton/vector resolvers — the harness
    // doesn't run a Union sidecar; capabilities that ask for the federated
    // `team` field will get null, which is fine for the scenarios here.
    let deployable_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let service_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let dependency_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
        .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
        .build();
    let exposes_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
        .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
        .build();
    let contract_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
        .build();
    let sla_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByContractId", r#"{"payload.contract_id": "{{contract_id}}"}"#)
        .build();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/deployable/graph".into(),
                schema_text: DEPLOYABLE_GRAPHQL.into(),
                root_config: deployable_gql,
                searcher: deployable.searcher,
            },
            GraphletteConfig {
                path: "/service/graph".into(),
                schema_text: SERVICE_GRAPHQL.into(),
                root_config: service_gql,
                searcher: service.searcher,
            },
            GraphletteConfig {
                path: "/dependency/graph".into(),
                schema_text: DEPENDENCY_GRAPHQL.into(),
                root_config: dependency_gql,
                searcher: dependency.searcher,
            },
            GraphletteConfig {
                path: "/exposes/graph".into(),
                schema_text: EXPOSES_GRAPHQL.into(),
                root_config: exposes_gql,
                searcher: exposes.searcher,
            },
            GraphletteConfig {
                path: "/contract/graph".into(),
                schema_text: CONTRACT_GRAPHQL.into(),
                root_config: contract_gql,
                searcher: contract.searcher,
            },
            GraphletteConfig {
                path: "/sla/graph".into(),
                schema_text: SLA_GRAPHQL.into(),
                root_config: sla_gql,
                searcher: sla.searcher,
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_groundwork-mcp"))
            .env("GROUNDWORK_URL", server_url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn groundwork-mcp");
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

// Cucumber requires Default; ChildStdin doesn't implement it, so we hand-roll.
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
}

// ── Step definitions ─────────────────────────────────────────────────────────

#[given("a Groundwork server is running")]
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

#[given(regex = r#"^I have registered deployable "([^"]+)"$"#)]
async fn register_deployable(world: &mut McpWorld, name: String) {
    let url = format!("{}/deployable/api", world.server_url());
    let resp = world
        .http
        .post(&url)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .unwrap();
    let env: Value = resp.json().await.unwrap();
    let id = env.get("id").and_then(|v| v.as_str()).unwrap().to_string();
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have registered service "([^"]+)"$"#)]
async fn register_service(world: &mut McpWorld, name: String) {
    let url = format!("{}/service/api", world.server_url());
    let resp = world
        .http
        .post(&url)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .unwrap();
    let env: Value = resp.json().await.unwrap();
    let id = env.get("id").and_then(|v| v.as_str()).unwrap().to_string();
    world.ids.insert(name, id);
}

#[given(regex = r#"^I have recorded that "([^"]+)" depends on "([^"]+)"$"#)]
async fn record_dependency(world: &mut McpWorld, dep_name: String, svc_name: String) {
    let dep_id = world.ids.get(&dep_name).cloned().expect("deployable id");
    let svc_id = world.ids.get(&svc_name).cloned().expect("service id");
    let url = format!("{}/dependency/api", world.server_url());
    world
        .http
        .post(&url)
        .json(&serde_json::json!({
            "deployable_id": dep_id,
            "service_id": svc_id,
            "protocol": "http",
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[given(regex = r#"^I have recorded that "([^"]+)" exposes "([^"]+)"$"#)]
async fn record_exposes(world: &mut McpWorld, dep_name: String, svc_name: String) {
    let dep_id = world.ids.get(&dep_name).cloned().expect("deployable id");
    let svc_id = world.ids.get(&svc_name).cloned().expect("service id");
    let url = format!("{}/exposes/api", world.server_url());
    world
        .http
        .post(&url)
        .json(&serde_json::json!({
            "deployable_id": dep_id,
            "service_id": svc_id,
            "protocol": "http",
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
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
    let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(|n| n.as_str())).collect();
    assert!(
        names.iter().any(|n| *n == name),
        "tool {name} not in {names:?}"
    );
}

impl McpWorld {
    /// Pull the array of GraphQL rows from the structured tool result.
    /// Auto-derived capabilities wrap their data under the operation name
    /// (`{ getAll: [...] }` or `{ getByName: [...] }`); fall back to a raw
    /// array (when scenarios call a custom Custom-handler capability that
    /// returns one) and to the `result`-wrapped envelope.
    fn gql_rows(&self) -> Vec<Value> {
        let r = self.structured_result();
        for key in ["getAll", "getByName", "getById"] {
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
        panic!("expected GraphQL rows under getAll/getByName/getById, got: {r}");
    }

    /// Pull the single GraphQL row from a `getById`-style result.
    fn gql_row(&self) -> Value {
        let r = self.structured_result();
        for key in ["getById"] {
            if let Some(row) = r.get(key) {
                return row.clone();
            }
        }
        r.clone()
    }
}

#[then("the tool result should be a JSON array")]
async fn tool_result_is_array(world: &mut McpWorld) {
    // GraphQL responses wrap rows under the operation name; the helper
    // unwraps to a Vec<Value> for assertions.
    let _ = world.gql_rows();
}

#[then(regex = r#"^the tool result should contain a record named "([^"]+)"$"#)]
async fn tool_result_contains_named(world: &mut McpWorld, name: String) {
    let arr = world.gql_rows();
    let found = arr
        .iter()
        .any(|row| row.get("name").and_then(|v| v.as_str()) == Some(name.as_str()));
    assert!(found, "no record named {name} in {arr:?}");
}

#[then(regex = r#"^the tool result name should be "([^"]+)"$"#)]
async fn flat_name_equals(world: &mut McpWorld, expected: String) {
    let row = world.gql_row();
    let actual = row.get("name").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(actual, expected, "row: {row}");
}

#[then(regex = r#"^the tool result should describe "([^"]+)" as a dependent$"#)]
async fn result_describes_dependent(world: &mut McpWorld, name: String) {
    let r = world.structured_result();
    let direct = r
        .get("direct_dependents")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let found = direct.iter().any(|e| {
        e.get("deployable_name").and_then(|v| v.as_str()) == Some(name.as_str())
    });
    assert!(found, "no direct dependent named {name} in {r}");
}

#[then(regex = r#"^the deployment plan should list "([^"]+)" before "([^"]+)"$"#)]
async fn plan_orders(world: &mut McpWorld, first: String, second: String) {
    let r = world.structured_result();
    let ord = r
        .get("ordered_deployments")
        .and_then(|v| v.as_array())
        .expect("ordered_deployments");
    let pos = |name: &str| {
        ord.iter()
            .position(|s| s.get("deployable_name").and_then(|n| n.as_str()) == Some(name))
    };
    let p_first = pos(&first).unwrap_or_else(|| panic!("{first} missing in {r}"));
    let p_second = pos(&second).unwrap_or_else(|| panic!("{second} missing in {r}"));
    assert!(
        p_first < p_second,
        "{first} (pos {p_first}) should come before {second} (pos {p_second}). plan: {r}"
    );
}

#[then(regex = r#"^the deployment plan should list "([^"]+)" as an external prerequisite$"#)]
async fn plan_lists_external(world: &mut McpWorld, name: String) {
    let r = world.structured_result();
    let ext = r
        .get("external_prerequisites")
        .and_then(|v| v.as_array())
        .expect("external_prerequisites");
    let found = ext.iter().any(|s| {
        s.get("service_name").and_then(|v| v.as_str()) == Some(name.as_str())
    });
    assert!(found, "{name} not an external prereq in {r}");
}

#[then(regex = r#"^the tool result should describe "([^"]+)" as a dependency$"#)]
async fn result_describes_dependency(world: &mut McpWorld, name: String) {
    let r = world.structured_result();
    let arr = r
        .get("depends_on")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let found = arr.iter().any(|e| {
        e.get("deployable_name").and_then(|v| v.as_str()) == Some(name.as_str())
    });
    assert!(found, "no dependency named {name} in {r}");
}

#[tokio::main]
async fn main() {
    McpWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features/mcp_tools.feature")
        .await;
}
