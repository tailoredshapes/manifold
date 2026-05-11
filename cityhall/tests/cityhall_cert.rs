mod common;

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use common::stub_union::{self, StubTeam, TeamRegistry};
use cucumber::{given, then, when, World};
use meshql_core::{GraphletteConfig, NoAuth, Repository, RootConfig, ServerConfig, Stash};
use meshql_server::{ValidatorContext, ValidatorFn};
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use reqwest::Client;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;

const ORG_NODE_GRAPHQL: &str = include_str!("../config/graph/org_node.graphql");
const BYLAW_GRAPHQL: &str = include_str!("../config/graph/bylaw.graphql");
const CHANGE_REQUEST_GRAPHQL: &str = include_str!("../config/graph/change_request.graphql");
const DEPLOYMENT_PLAN_GRAPHQL: &str = include_str!("../config/graph/deployment_plan.graphql");
const GANTT_OUTPUT_GRAPHQL: &str = include_str!("../config/graph/gantt_output.graphql");

// ── Stub Groundwork ──────────────────────────────────────────────────────────

#[derive(Default)]
struct StubGroundwork {
    deployables: std::sync::Mutex<HashMap<String, StubDeployable>>,
}

#[derive(Clone)]
struct StubDeployable {
    name: String,
    team_id: Option<String>,
    depends_on: Vec<String>,
}

impl StubGroundwork {
    fn put(&self, id: &str, name: &str, team_id: Option<&str>, depends_on: Vec<String>) {
        self.deployables.lock().unwrap().insert(
            id.to_string(),
            StubDeployable {
                name: name.to_string(),
                team_id: team_id.map(String::from),
                depends_on,
            },
        );
    }
}

#[async_trait::async_trait]
impl cityhall::plan::GroundworkLookup for StubGroundwork {
    async fn get_deployable(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<cityhall::plan::DeployableSummary>> {
        Ok(self.deployables.lock().unwrap().get(id).map(|d| {
            cityhall::plan::DeployableSummary {
                id: id.to_string(),
                name: d.name.clone(),
                team_id: d.team_id.clone(),
                depends_on: d.depends_on.clone(),
            }
        }))
    }
}

// ── World ────────────────────────────────────────────────────────────────────

#[derive(Debug, World)]
pub struct CityhallWorld {
    pub server_addr: Option<String>,
    pub ids: HashMap<String, String>,
    pub last_response_status: Option<u16>,
    pub last_response_body: Option<String>,
    pub last_response_content_type: Option<String>,
    pub client: Client,
    pub last_two_bodies: Vec<String>,
    pub union_teams: Option<TeamRegistry>,
}

impl Default for CityhallWorld {
    fn default() -> Self {
        Self {
            server_addr: None,
            ids: HashMap::new(),
            last_response_status: None,
            last_response_body: None,
            last_response_content_type: None,
            client: Client::new(),
            last_two_bodies: Vec::new(),
            union_teams: None,
        }
    }
}

impl CityhallWorld {
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

#[derive(Clone)]
struct TestEntity {
    repo: Arc<dyn Repository>,
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

fn org_node_validator_test(schema_str: &str) -> ValidatorFn {
    let base = validator_for(schema_str);
    Arc::new(move |payload: &Stash, ctx: &ValidatorContext| {
        base(payload, ctx)?;
        let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let parent_id_present = payload
            .get("parent_id")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        match (kind, parent_id_present) {
            ("enterprise", true) => Err("Enterprise OrgNode must not have a parent_id".to_string()),
            ("enterprise", false) => Ok(()),
            (_, false) => Err(format!("OrgNode kind '{kind}' must have a parent_id")),
            _ => Ok(()),
        }
    })
}

fn bylaw_validator_test(schema_str: &str) -> ValidatorFn {
    let base = validator_for(schema_str);
    Arc::new(move |payload: &Stash, ctx: &ValidatorContext| {
        base(payload, ctx)?;
        let gate_type = payload.get("gate_type").and_then(|v| v.as_str()).unwrap_or("");
        let needs: &[&str] = match gate_type {
            "WindowGate" | "FreezePeriod" => &["window"],
            "QuiesceGate" => &["quiesce_for"],
            "ApprovalGate" => &["approvers"],
            _ => &[],
        };
        for field in needs {
            let present = payload
                .get(*field)
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !present {
                return Err(format!(
                    "Bylaw gate_type '{gate_type}' requires field '{field}'"
                ));
            }
        }
        Ok(())
    })
}

#[derive(Clone)]
struct AppState {
    org_node: TestEntity,
    bylaw: TestEntity,
    change_request: TestEntity,
    deployment_plan: TestEntity,
    gantt_output: TestEntity,
    groundwork: Arc<StubGroundwork>,
}

async fn build_test_server(stub: Arc<StubGroundwork>) -> (String, TeamRegistry) {
    let org_node = make_entity().await;
    let bylaw_e = make_entity().await;
    let change_request = make_entity().await;
    let deployment_plan = make_entity().await;
    let gantt_output = make_entity().await;

    let (union_url, team_registry) = stub_union::spawn().await;

    let org_node_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector("getByParentId", r#"{"payload.parent_id": "{{parent_id}}"}"#)
        .vector("getByTeamId", r#"{"payload.team_id": "{{team_id}}"}"#)
        .singleton_resolver(
            "team",
            Some("team_id"),
            "getById",
            format!("{}/team/graph", union_url),
        )
        .build();
    let bylaw_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByOrgNodeId", r#"{"payload.org_node_id": "{{org_node_id}}"}"#)
        .vector("getByGateType", r#"{"payload.gate_type": "{{gate_type}}"}"#)
        .build();
    let change_request_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByStatus", r#"{"payload.status": "{{status}}"}"#)
        .vector("getByTier", r#"{"payload.tier": "{{tier}}"}"#)
        .build();
    let deployment_plan_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByChangeRequestId",
            r#"{"payload.change_request_id": "{{change_request_id}}"}"#,
        )
        .build();
    let gantt_output_root = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByDeploymentPlanId",
            r#"{"payload.deployment_plan_id": "{{deployment_plan_id}}"}"#,
        )
        .build();

    let server_config = ServerConfig {
        port: 0,
        graphlettes: vec![
            GraphletteConfig {
                path: "/org_node/graph".into(),
                schema_text: ORG_NODE_GRAPHQL.into(),
                root_config: org_node_root,
                searcher: org_node.searcher.clone(),
            },
            GraphletteConfig {
                path: "/bylaw/graph".into(),
                schema_text: BYLAW_GRAPHQL.into(),
                root_config: bylaw_root,
                searcher: bylaw_e.searcher.clone(),
            },
            GraphletteConfig {
                path: "/change_request/graph".into(),
                schema_text: CHANGE_REQUEST_GRAPHQL.into(),
                root_config: change_request_root,
                searcher: change_request.searcher.clone(),
            },
            GraphletteConfig {
                path: "/deployment_plan/graph".into(),
                schema_text: DEPLOYMENT_PLAN_GRAPHQL.into(),
                root_config: deployment_plan_root,
                searcher: deployment_plan.searcher.clone(),
            },
            GraphletteConfig {
                path: "/gantt_output/graph".into(),
                schema_text: GANTT_OUTPUT_GRAPHQL.into(),
                root_config: gantt_output_root,
                searcher: gantt_output.searcher.clone(),
            },
        ],
        restlettes: vec![],
    };

    let auth = Arc::new(NoAuth);

    let org_node_restlette = meshql_server::build_restlette_router_ext(
        "/org_node/api",
        org_node.repo.clone(),
        auth.clone(),
        None,
        Some(org_node_validator_test(include_str!("../config/json/org_node.schema.json"))),
        None,
        None,
    );
    let bylaw_restlette = meshql_server::build_restlette_router_ext(
        "/bylaw/api",
        bylaw_e.repo.clone(),
        auth.clone(),
        None,
        Some(bylaw_validator_test(include_str!("../config/json/bylaw.schema.json"))),
        None,
        None,
    );
    let change_request_restlette = meshql_server::build_restlette_router_ext(
        "/change_request/api",
        change_request.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/change_request.schema.json"))),
        None,
        None,
    );
    let deployment_plan_restlette = meshql_server::build_restlette_router_ext(
        "/deployment_plan/api",
        deployment_plan.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/deployment_plan.schema.json"))),
        None,
        None,
    );
    let gantt_output_restlette = meshql_server::build_restlette_router_ext(
        "/gantt_output/api",
        gantt_output.repo.clone(),
        auth.clone(),
        None,
        Some(validator_for(include_str!("../config/json/gantt_output.schema.json"))),
        None,
        None,
    );

    let app_state = AppState {
        org_node: org_node.clone(),
        bylaw: bylaw_e.clone(),
        change_request: change_request.clone(),
        deployment_plan: deployment_plan.clone(),
        gantt_output: gantt_output.clone(),
        groundwork: stub,
    };

    let custom_routes = Router::new()
        .route("/org_node/:id/ancestors", get(get_ancestors))
        .route("/org_node/:id/effective_bylaws", get(get_effective_bylaws))
        .route("/change_request/:id/plan", post(post_change_request_plan))
        .route("/deployment_plan/:id/gantt", post(post_deployment_plan_gantt))
        .with_state(app_state);

    let extra = Router::new()
        .route("/health", get(|| async {
            ([(header::CONTENT_TYPE, "application/json")], r#"{"status":"ok"}"#).into_response()
        }))
        .merge(org_node_restlette)
        .merge(bylaw_restlette)
        .merge(change_request_restlette)
        .merge(deployment_plan_restlette)
        .merge(gantt_output_restlette)
        .merge(custom_routes);

    let app = meshql_server::build_app_ext(server_config, extra)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://127.0.0.1:{}", addr.port()), team_registry)
}

// ── Custom-route handlers ───────────────────────────────────────────────────

async fn get_ancestors(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match cityhall::bylaw::ancestors_of(&state.org_node.repo, &id).await {
        Ok(chain) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&chain).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("ancestors error: {e}"),
        )
            .into_response(),
    }
}

async fn get_effective_bylaws(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match cityhall::bylaw::effective_bylaws_for(&state.org_node.repo, &state.bylaw.repo, &id).await
    {
        Ok(list) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&list).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("effective_bylaws error: {e}"),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize, Default)]
struct PlanRequest {
    #[serde(default)]
    tier: Option<String>,
}

async fn post_change_request_plan(
    State(state): State<AppState>,
    Path(cr_id): Path<String>,
    Json(req): Json<PlanRequest>,
) -> Response {
    let env = match state.change_request.repo.read(&cr_id, &[], None).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("change_request {cr_id} not found"),
            )
                .into_response()
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("read change_request: {e}"),
            )
                .into_response()
        }
    };
    let summary = env
        .payload
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tier = req
        .tier
        .or_else(|| env.payload.get("tier").and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_else(|| "dev".into());
    let target_str = env
        .payload
        .get("target_deployables")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let target_deployable_ids: Vec<String> = if target_str.trim().is_empty() {
        Vec::new()
    } else if let Ok(v) = serde_json::from_str::<Vec<String>>(target_str) {
        v
    } else {
        target_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    let inputs = cityhall::plan::PlanInputs {
        change_request_id: cr_id.clone(),
        change_request_summary: summary.clone(),
        tier: tier.clone(),
        target_deployable_ids,
        org_node_repo: &state.org_node.repo,
        bylaw_repo: &state.bylaw.repo,
        groundwork: state.groundwork.as_ref(),
    };
    let computed = match cityhall::plan::compute_plan(inputs).await {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("compute_plan: {e}"),
            )
                .into_response()
        }
    };

    let mut payload = Stash::new();
    payload.insert("change_request_id".into(), Value::String(computed.change_request_id.clone()));
    payload.insert("tier".into(), Value::String(computed.tier.clone()));
    payload.insert(
        "steps".into(),
        Value::String(serde_json::to_string(&computed.steps).unwrap_or_default()),
    );
    payload.insert(
        "blockers".into(),
        Value::String(serde_json::to_string(&computed.blockers).unwrap_or_default()),
    );
    payload.insert("computed_at".into(), Value::String(computed.computed_at.clone()));
    payload.insert("summary".into(), Value::String(computed.change_request_summary.clone()));
    let envelope = meshql_core::Envelope::new(synthetic_id(&cr_id), payload, vec![]);

    match state.deployment_plan.repo.create(envelope, &[]).await {
        Ok(saved) => (
            axum::http::StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&saved).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("persist plan: {e}"),
        )
            .into_response(),
    }
}

async fn post_deployment_plan_gantt(
    State(state): State<AppState>,
    Path(plan_id): Path<String>,
) -> Response {
    let env = match state.deployment_plan.repo.read(&plan_id, &[], None).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("deployment_plan {plan_id} not found"),
            )
                .into_response()
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("read plan: {e}"),
            )
                .into_response()
        }
    };
    let steps_str = env.payload.get("steps").and_then(|v| v.as_str()).unwrap_or("[]");
    let steps: Vec<cityhall::plan::PlanStep> =
        serde_json::from_str(steps_str).unwrap_or_default();
    let blockers_str = env.payload.get("blockers").and_then(|v| v.as_str()).unwrap_or("[]");
    let blockers: Vec<cityhall::plan::Blocker> = serde_json::from_str(blockers_str).unwrap_or_default();
    let computed = cityhall::plan::ComputedPlan {
        change_request_id: env.payload.get("change_request_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        change_request_summary: env.payload.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        tier: env.payload.get("tier").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        steps,
        blockers,
        computed_at: env.payload.get("computed_at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    };
    let mermaid = cityhall::gantt::render_gantt(&computed);

    let mut payload = Stash::new();
    payload.insert("deployment_plan_id".into(), Value::String(plan_id.clone()));
    payload.insert("tier".into(), Value::String(computed.tier.clone()));
    payload.insert("mermaid".into(), Value::String(mermaid));
    let envelope = meshql_core::Envelope::new(synthetic_id(&plan_id), payload, vec![]);
    match state.gantt_output.repo.create(envelope, &[]).await {
        Ok(saved) => (
            axum::http::StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&saved).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("persist gantt: {e}"),
        )
            .into_response(),
    }
}

fn synthetic_id(seed: &str) -> String {
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    format!("{}-{:x}", seed, nanos)
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

async fn do_request(world: &mut CityhallWorld, method: &str, path: &str, body: Option<Value>) {
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
    world.last_two_bodies.push(body_text.clone());
    if world.last_two_bodies.len() > 2 {
        world.last_two_bodies.remove(0);
    }
    world.store_response(status, body_text, ct);
}

async fn post_for_id(world: &mut CityhallWorld, path: &str, payload: Value) -> String {
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

// ── Step definitions ─────────────────────────────────────────────────────────

#[given("a Cityhall server is running")]
async fn start_server(world: &mut CityhallWorld) {
    let stub = Arc::new(StubGroundwork::default());
    stub.put("dep-checkout", "checkout", Some("team-checkout"), vec!["dep-auth".into()]);
    stub.put("dep-auth", "auth", Some("team-auth"), vec![]);
    stub.put("dep-orphan", "orphan", None, vec![]);
    let (addr, team_registry) = build_test_server(stub).await;
    world.server_addr = Some(addr);
    world.union_teams = Some(team_registry);
    world.ids.clear();
    world.last_two_bodies.clear();
}

#[given(regex = r#"^the Union stub knows team "(.+)" as "(.+)" of kind "(.+)"$"#)]
async fn union_stub_knows_team(
    world: &mut CityhallWorld,
    team_id: String,
    name: String,
    kind: String,
) {
    let reg = world.union_teams.as_ref().expect("Union stub not started");
    reg.insert(StubTeam {
        id: team_id,
        name,
        kind,
        description: None,
    });
}

#[given("I have built the standard hierarchy")]
async fn standard_hierarchy(world: &mut CityhallWorld) {
    let acme = post_for_id(world, "/org_node/api", serde_json::json!({"name":"Acme","kind":"enterprise"})).await;
    let eng = post_for_id(world, "/org_node/api", serde_json::json!({"name":"Engineering","kind":"division","parent_id":acme.clone()})).await;
    let payments = post_for_id(world, "/org_node/api", serde_json::json!({"name":"Payments","kind":"domain","parent_id":eng.clone()})).await;
    let checkout = post_for_id(world, "/org_node/api", serde_json::json!({"name":"Checkout Team","kind":"team","parent_id":payments.clone(),"team_id":"team-checkout"})).await;
    let auth = post_for_id(world, "/org_node/api", serde_json::json!({"name":"Auth Team","kind":"team","parent_id":payments.clone(),"team_id":"team-auth"})).await;
    world.ids.insert("acme".into(), acme);
    world.ids.insert("eng".into(), eng);
    world.ids.insert("payments".into(), payments);
    world.ids.insert("checkout".into(), checkout);
    world.ids.insert("auth".into(), auth);
}

#[given(regex = r#"^I have submitted change request "(.+)"$"#)]
async fn submit_change_request(world: &mut CityhallWorld, label: String) {
    let id = post_for_id(world, "/change_request/api", serde_json::json!({"summary": label.clone()})).await;
    world.ids.insert(label, id);
}

#[given(regex = r#"^I have a change request "(.+)" with target deployables \[(.+)\]$"#)]
async fn change_request_with_targets(world: &mut CityhallWorld, label: String, targets: String) {
    let parsed: Vec<String> = targets
        .split(',')
        .map(|t| t.trim().trim_matches('"').to_string())
        .collect();
    let target_json = serde_json::to_string(&parsed).unwrap();
    let id = post_for_id(
        world,
        "/change_request/api",
        serde_json::json!({"summary": label.clone(), "target_deployables": target_json}),
    )
    .await;
    world.ids.insert(label, id);
}

#[given(regex = r#"^enterprise "<ids\.(.+)>" has a "(.+)" bylaw with window "(.+)"$"#)]
async fn enterprise_freeze_with_window(world: &mut CityhallWorld, node_label: String, gate_type: String, window: String) {
    let node_id = world.ids.get(&node_label).cloned().expect("node not registered");
    post_for_id(
        world,
        "/bylaw/api",
        serde_json::json!({
            "org_node_id": node_id,
            "gate_type": gate_type,
            "window": window,
            "priority": "100",
        }),
    )
    .await;
}

#[given("I have computed a deployment plan with 2 sequential steps and one ApprovalGate")]
async fn compute_two_step_plan(world: &mut CityhallWorld) {
    standard_hierarchy(world).await;
    let auth_node_id = world.ids.get("auth").cloned().unwrap();
    post_for_id(
        world,
        "/bylaw/api",
        serde_json::json!({
            "org_node_id": auth_node_id,
            "gate_type": "ApprovalGate",
            "approvers": "person-abc",
            "priority": "50",
        }),
    )
    .await;
    let cr_id = post_for_id(
        world,
        "/change_request/api",
        serde_json::json!({
            "summary": "deploy-checkout",
            "target_deployables": serde_json::to_string(&vec!["dep-checkout".to_string()]).unwrap(),
        }),
    )
    .await;
    world.ids.insert("cr".into(), cr_id.clone());

    let url = format!("{}/change_request/{}/plan", world.base_url(), cr_id);
    let resp = world
        .client
        .post(&url)
        .json(&serde_json::json!({"tier": "prod"}))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();
    let parsed: Value = serde_json::from_str(&body).expect("plan not JSON");
    let plan_id = parsed.get("id").and_then(|v| v.as_str()).map(String::from).expect("no id");
    world.ids.insert("plan".into(), plan_id);
}

#[when(regex = r#"^I (GET|DELETE) "(.+)"$"#)]
async fn http_get_delete(world: &mut CityhallWorld, method: String, path: String) {
    let resolved = world.resolve(&path);
    do_request(world, &method, &resolved, None).await;
}

#[when(regex = r#"^I POST to "(.+)" with body (.+)$"#)]
async fn http_post(world: &mut CityhallWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let resolved_body = world.resolve(&body_str);
    let body: Value = serde_json::from_str(&resolved_body).expect("invalid JSON body");
    do_request(world, "POST", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I PUT "(.+)" with body (.+)$"#)]
async fn http_put(world: &mut CityhallWorld, path: String, body_str: String) {
    let resolved_path = world.resolve(&path);
    let resolved_body = world.resolve(&body_str);
    let body: Value = serde_json::from_str(&resolved_body).expect("invalid JSON body");
    do_request(world, "PUT", &resolved_path, Some(body)).await;
}

#[when(regex = r#"^I query the "(.+)" graph with: (.+)$"#)]
async fn graphql_query(world: &mut CityhallWorld, entity: String, query_str: String) {
    let resolved = world.resolve(&query_str);
    let path = format!("/{entity}/graph");
    let body = serde_json::json!({ "query": resolved });
    do_request(world, "POST", &path, Some(body)).await;
}

#[then(regex = r"^the response status should be (\d+)$")]
async fn check_status(world: &mut CityhallWorld, expected: u16) {
    let actual = world.last_response_status.expect("no response recorded");
    assert_eq!(
        actual, expected,
        "Expected status {expected}, got {actual}. Body: {:?}",
        world.last_response_body
    );
}

#[then(regex = r#"^the response body should contain "(.+)"$"#)]
async fn body_contains(world: &mut CityhallWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(body.contains(&resolved), "Expected body to contain {resolved:?}\nGot: {body}");
}

#[then(r#"the response body should have an "id" field"#)]
async fn body_has_id(world: &mut CityhallWorld) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("response not JSON");
    assert!(parsed.get("id").map(|v| !v.is_null()).unwrap_or(false), "No 'id' field in: {body}");
}

#[then("there should be no GraphQL errors")]
async fn no_graphql_errors(world: &mut CityhallWorld) {
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
async fn response_data_contains(world: &mut CityhallWorld, expected: String) {
    let resolved = world.resolve(&expected);
    let body = world.last_response_body.as_deref().unwrap_or("");
    assert!(body.contains(&resolved), "Expected response data to contain {resolved:?}\nGot: {body}");
}

#[then(regex = r"^the plan should have (\d+) step$")]
#[then(regex = r"^the plan should have (\d+) steps$")]
async fn plan_step_count(world: &mut CityhallWorld, expected: usize) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let steps_str = parsed.pointer("/payload/steps").and_then(|v| v.as_str()).unwrap_or("[]");
    let steps: Value = serde_json::from_str(steps_str).expect("steps not JSON array");
    let actual = steps.as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(actual, expected, "Expected {expected} steps, got {actual}. Body: {body}");
}

#[then(regex = r#"^the plan step (\d+) should be "deploy (.+)"$"#)]
async fn plan_step_is(world: &mut CityhallWorld, index: usize, deployable_name: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let steps_str = parsed.pointer("/payload/steps").and_then(|v| v.as_str()).unwrap_or("[]");
    let steps: Value = serde_json::from_str(steps_str).unwrap();
    let step = steps.get(index).expect("step index out of range");
    let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let name = step.get("deployable_name").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(action, "deploy");
    assert_eq!(name, deployable_name);
}

#[then(regex = r#"^the plan step (\d+) should have a "(.+)" gate$"#)]
async fn plan_step_has_gate(world: &mut CityhallWorld, index: usize, gate_type: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let steps_str = parsed.pointer("/payload/steps").and_then(|v| v.as_str()).unwrap_or("[]");
    let steps: Value = serde_json::from_str(steps_str).unwrap();
    let step = steps.get(index).expect("step index out of range");
    let gates = step.get("gates").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let found = gates.iter().any(|g| g.get("gate_type").and_then(|v| v.as_str()) == Some(gate_type.as_str()));
    assert!(found, "no {gate_type} gate on step {index}: {step}");
}

#[then(regex = r#"^the plan blockers should contain "(.+)"$"#)]
async fn plan_blockers_contain(world: &mut CityhallWorld, expected: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let parsed: Value = serde_json::from_str(body).expect("not JSON");
    let blockers_str = parsed.pointer("/payload/blockers").and_then(|v| v.as_str()).unwrap_or("[]");
    // Blockers are now structured ({kind, message, mermaid?}); match against
    // the message field so feature scenarios stay readable.
    let blockers: Vec<Value> = serde_json::from_str(blockers_str).unwrap_or_default();
    let messages: Vec<String> = blockers
        .iter()
        .filter_map(|b| b.get("message").and_then(|m| m.as_str()).map(String::from))
        .collect();
    assert!(
        messages.iter().any(|m| m.contains(&expected)),
        "Blocker messages {messages:?} do not contain {expected:?}. Body: {body}"
    );
}

#[then("both responses should be byte-equal")]
async fn both_byte_equal(world: &mut CityhallWorld) {
    assert_eq!(world.last_two_bodies.len(), 2, "need 2 responses");
    let strip = |s: &str| -> String {
        let v: Value = serde_json::from_str(s).unwrap_or(Value::Null);
        v.pointer("/payload/mermaid").and_then(|m| m.as_str()).map(String::from).unwrap_or_default()
    };
    let a = strip(&world.last_two_bodies[0]);
    let b = strip(&world.last_two_bodies[1]);
    assert_eq!(a, b, "Mermaid output differs");
    assert!(!a.is_empty(), "mermaid string is empty");
}

#[tokio::main]
async fn main() {
    CityhallWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features")
        .await;
}
