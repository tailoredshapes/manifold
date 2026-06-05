//! Lobby HTTP service. Hosts 6 meshlette entities, six restlettes, six
//! graphlettes, four custom RPC routes (advisory actions), static frontend
//! assets, and the derivation engine as a background task.

use axum::{
    extract::{Path, State},
    http::header,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use manifold_edge::{with_header_identity, with_response_cache, HeaderConfig};
use manifold_lobby::engine::{Engine, UserAction};
use manifold_lobby::sources::SourceClients;
use manifold_lobby::state::{AppState, Entity};
use meshql_casbin::CasbinAuth;
use meshql_core::{
    Auth, AuthContext, GraphletteConfig, RootConfig, ServerConfig, Stash, StashKeyAuth,
};
use meshql_server::{ValidatorContext, ValidatorFn};
#[cfg(feature = "sqlite")]
use meshql_sqlite::{SqliteRepository, SqliteSearcher};
use serde::Deserialize;
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
#[cfg(feature = "mongo")]
use meshql_mongo::{MongoRepository, MongoSearcher};
#[cfg(feature = "sqlite")]
use std::str::FromStr;
use std::sync::Arc;

const ADVISORY_GRAPHQL: &str = include_str!("../config/graph/advisory.graphql");
const PROGRAM_GRAPHQL: &str = include_str!("../config/graph/program.graphql");
const PROGRAM_MEMBERSHIP_GRAPHQL: &str = include_str!("../config/graph/program_membership.graphql");
const LIFECYCLE_GRAPHQL: &str = include_str!("../config/graph/lifecycle_entry.graphql");
const SAVED_VIEW_GRAPHQL: &str = include_str!("../config/graph/saved_view.graphql");
const COMMENT_GRAPHQL: &str = include_str!("../config/graph/comment.graphql");
const AUTH_MODEL: &str = include_str!("../config/auth/model.conf");
const AUTH_POLICY: &str = include_str!("../config/auth/policy.csv");
const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");

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

// ── Custom RPC routes ──────────────────────────────────────────────────────

// `acknowledge` takes no body — the action is meaningful on its own. If
// callers ever need to attach a note, mirror DismissBody and wire it through
// `UserAction::acknowledge`.

#[derive(Deserialize)]
struct DismissBody {
    reason: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct EscalateBody {
    to: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct AssignBody {
    assignee: String,
}

#[derive(Deserialize)]
struct CommentBody {
    body: String,
}

async fn ack_advisory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Extension(ctx): axum::extract::Extension<AuthContext>,
) -> Response {
    let user_id = ctx_user(&ctx);
    let action = UserAction {
        state: &state,
        advisory_id: &id,
        user_id: &user_id,
    };
    match action.acknowledge().await {
        Ok(()) => (axum::http::StatusCode::OK, "{}").into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("ack: {e}"),
        )
            .into_response(),
    }
}

async fn dismiss_advisory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Extension(ctx): axum::extract::Extension<AuthContext>,
    Json(body): Json<DismissBody>,
) -> Response {
    let user_id = ctx_user(&ctx);
    let action = UserAction {
        state: &state,
        advisory_id: &id,
        user_id: &user_id,
    };
    match action.dismiss(&body.reason, body.note.as_deref()).await {
        Ok(()) => (axum::http::StatusCode::OK, "{}").into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("dismiss: {e}"),
        )
            .into_response(),
    }
}

async fn escalate_advisory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Extension(ctx): axum::extract::Extension<AuthContext>,
    Json(body): Json<EscalateBody>,
) -> Response {
    let user_id = ctx_user(&ctx);
    let action = UserAction {
        state: &state,
        advisory_id: &id,
        user_id: &user_id,
    };
    match action.escalate(&body.to, body.note.as_deref()).await {
        Ok(()) => (axum::http::StatusCode::OK, "{}").into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("escalate: {e}"),
        )
            .into_response(),
    }
}

async fn assign_advisory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Extension(ctx): axum::extract::Extension<AuthContext>,
    Json(body): Json<AssignBody>,
) -> Response {
    let user_id = ctx_user(&ctx);
    let action = UserAction {
        state: &state,
        advisory_id: &id,
        user_id: &user_id,
    };
    match action.assign(&body.assignee).await {
        Ok(()) => (axum::http::StatusCode::OK, "{}").into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("assign: {e}"),
        )
            .into_response(),
    }
}

async fn comment_advisory(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Extension(ctx): axum::extract::Extension<AuthContext>,
    Json(body): Json<CommentBody>,
) -> Response {
    let user_id = ctx_user(&ctx);
    let action = UserAction {
        state: &state,
        advisory_id: &id,
        user_id: &user_id,
    };
    match action.add_comment(&body.body).await {
        Ok(cid) => (
            axum::http::StatusCode::CREATED,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&serde_json::json!({ "id": cid })).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("comment: {e}"),
        )
            .into_response(),
    }
}

fn ctx_user(ctx: &AuthContext) -> String {
    ctx.0
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

async fn trigger_derive(State(state): State<AppState>) -> Response {
    let sources = SourceClients::from_env();
    let engine = Engine::new(state, sources);
    match engine.tick().await {
        Ok(report) => (
            axum::http::StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&serde_json::json!({
                "raised": report.raised,
                "resolved": report.resolved,
                "re_raised": report.re_raised,
            }))
            .unwrap_or_default(),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("derive: {e}"),
        )
            .into_response(),
    }
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::try_init().ok();

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .expect("PORT must be a valid u16");
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    // SQLite-only: the mongo build talks to Atlas and must not write to the
    // (read-only, on Lambda) filesystem.
    #[cfg(feature = "sqlite")]
    std::fs::create_dir_all(&data_dir)?;

    // Cross-app public URLs — published via /config.json so the frontend can
    // build deep-links to the owning app (crossLink). Subject deep-links need
    // groundwork/union/cityhall/yard; manifold is the brand link.
    let groundwork_public_url = std::env::var("GROUNDWORK_PUBLIC_URL")
        .unwrap_or_else(|_| "https://groundwork.tildarc.com".into());
    let union_public_url =
        std::env::var("UNION_PUBLIC_URL").unwrap_or_else(|_| "https://union.tildarc.com".into());
    let cityhall_public_url = std::env::var("CITYHALL_PUBLIC_URL")
        .unwrap_or_else(|_| "https://cityhall.tildarc.com".into());
    let yard_public_url =
        std::env::var("YARD_PUBLIC_URL").unwrap_or_else(|_| "https://yard.tildarc.com".into());
    let manifold_public_url = std::env::var("MANIFOLD_PUBLIC_URL").unwrap_or_else(|_| "/".into());

    let config_body = serde_json::json!({
        "groundwork_public_url": groundwork_public_url,
        "union_public_url":      union_public_url,
        "cityhall_public_url":   cityhall_public_url,
        "yard_public_url":       yard_public_url,
        "manifold_public_url":   manifold_public_url,
    })
    .to_string();

    let advisory = make_entity(&data_dir, "advisory").await;
    let program = make_entity(&data_dir, "program").await;
    let program_membership = make_entity(&data_dir, "program_membership").await;
    let lifecycle_entry = make_entity(&data_dir, "lifecycle_entry").await;
    let saved_view = make_entity(&data_dir, "saved_view").await;
    let comment = make_entity(&data_dir, "comment").await;

    let app_state = AppState {
        advisory: advisory.clone(),
        program: program.clone(),
        program_membership: program_membership.clone(),
        lifecycle_entry: lifecycle_entry.clone(),
        saved_view: saved_view.clone(),
        comment: comment.clone(),
    };

    // Validators
    let advisory_schema: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/advisory.schema.json"))?;
    let program_schema: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/program.schema.json"))?;
    let pm_schema: serde_json::Value = serde_json::from_str(include_str!(
        "../config/json/program_membership.schema.json"
    ))?;
    let le_schema: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/lifecycle_entry.schema.json"))?;
    let sv_schema: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/saved_view.schema.json"))?;
    let comment_schema: serde_json::Value =
        serde_json::from_str(include_str!("../config/json/comment.schema.json"))?;

    // Graph configs
    let advisory_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByState", r#"{"payload.state": "{{state}}"}"#)
        .vector("getByKind", r#"{"payload.kind": "{{kind}}"}"#)
        .vector(
            "getBySubjectId",
            r#"{"payload.subject_id": "{{subject_id}}"}"#,
        )
        .vector(
            "getBySubjectType",
            r#"{"payload.subject_type": "{{subject_type}}"}"#,
        )
        .build();
    let program_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByName", r#"{"payload.name": "{{name}}"}"#)
        .build();
    let pm_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByProgramId",
            r#"{"payload.program_id": "{{program_id}}"}"#,
        )
        .vector(
            "getBySubjectId",
            r#"{"payload.subject_id": "{{subject_id}}"}"#,
        )
        .build();
    let le_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByAdvisoryId",
            r#"{"payload.advisory_id": "{{advisory_id}}"}"#,
        )
        .vector("getByActorId", r#"{"payload.actor_id": "{{actor_id}}"}"#)
        .build();
    let sv_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector("getByOwner", r#"{"payload.owner": "{{owner}}"}"#)
        .build();
    let comment_gql = RootConfig::builder()
        .singleton("getById", r#"{"id": "{{id}}"}"#)
        .vector("getAll", "{}")
        .vector(
            "getByAdvisoryId",
            r#"{"payload.advisory_id": "{{advisory_id}}"}"#,
        )
        .build();

    let config = ServerConfig {
        port,
        graphlettes: vec![
            GraphletteConfig {
                path: "/advisory/graph".into(),
                schema_text: ADVISORY_GRAPHQL.into(),
                root_config: advisory_gql,
                searcher: advisory.searcher.clone(),
            },
            GraphletteConfig {
                path: "/program/graph".into(),
                schema_text: PROGRAM_GRAPHQL.into(),
                root_config: program_gql,
                searcher: program.searcher.clone(),
            },
            GraphletteConfig {
                path: "/program_membership/graph".into(),
                schema_text: PROGRAM_MEMBERSHIP_GRAPHQL.into(),
                root_config: pm_gql,
                searcher: program_membership.searcher.clone(),
            },
            GraphletteConfig {
                path: "/lifecycle_entry/graph".into(),
                schema_text: LIFECYCLE_GRAPHQL.into(),
                root_config: le_gql,
                searcher: lifecycle_entry.searcher.clone(),
            },
            GraphletteConfig {
                path: "/saved_view/graph".into(),
                schema_text: SAVED_VIEW_GRAPHQL.into(),
                root_config: sv_gql,
                searcher: saved_view.searcher.clone(),
            },
            GraphletteConfig {
                path: "/comment/graph".into(),
                schema_text: COMMENT_GRAPHQL.into(),
                root_config: comment_gql,
                searcher: comment.searcher.clone(),
            },
        ],
        restlettes: vec![],
    };

    let auth: Arc<dyn Auth> = Arc::new(
        CasbinAuth::from_strings(AUTH_MODEL, AUTH_POLICY, StashKeyAuth::new("user_id")).await?,
    );

    let advisory_restlette = meshql_server::build_restlette_router_ext(
        "/advisory/api",
        advisory.repo.clone(),
        auth.clone(),
        None,
        Some(make_required_validator(&advisory_schema)),
        None,
        None,
    );
    let program_restlette = meshql_server::build_restlette_router_ext(
        "/program/api",
        program.repo.clone(),
        auth.clone(),
        None,
        Some(make_required_validator(&program_schema)),
        None,
        None,
    );
    let pm_restlette = meshql_server::build_restlette_router_ext(
        "/program_membership/api",
        program_membership.repo.clone(),
        auth.clone(),
        None,
        Some(make_required_validator(&pm_schema)),
        None,
        None,
    );
    let le_restlette = meshql_server::build_restlette_router_ext(
        "/lifecycle_entry/api",
        lifecycle_entry.repo.clone(),
        auth.clone(),
        None,
        Some(make_required_validator(&le_schema)),
        None,
        None,
    );
    let sv_restlette = meshql_server::build_restlette_router_ext(
        "/saved_view/api",
        saved_view.repo.clone(),
        auth.clone(),
        None,
        Some(make_required_validator(&sv_schema)),
        None,
        None,
    );
    let comment_restlette = meshql_server::build_restlette_router_ext(
        "/comment/api",
        comment.repo.clone(),
        auth.clone(),
        None,
        Some(make_required_validator(&comment_schema)),
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

    let custom = Router::new()
        .route("/health", get(health_check))
        .route("/", get(serve_index))
        .route("/static/app.js", get(serve_app_js))
        .route("/static/manifold-ui.css", get(serve_manifold_ui_css))
        .route("/static/favicon.png", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        .route("/static/manifold-ui.js", get(serve_manifold_ui_js))
        .route("/advisory/{id}/acknowledge", post(ack_advisory))
        .route("/advisory/{id}/dismiss", post(dismiss_advisory))
        .route("/advisory/{id}/escalate", post(escalate_advisory))
        .route("/advisory/{id}/assign", post(assign_advisory))
        .route("/advisory/{id}/comment", post(comment_advisory))
        .route("/_derive", post(trigger_derive))
        .with_state(app_state.clone())
        .merge(advisory_restlette)
        .merge(program_restlette)
        .merge(pm_restlette)
        .merge(le_restlette)
        .merge(sv_restlette)
        .merge(comment_restlette)
        .merge(config_route);

    let app = meshql_server::build_app_with_auth(config, auth, custom).await?;
    let app = with_header_identity(app, HeaderConfig::from_env());

    // Read-through response cache (outermost; no-op unless CACHE_TTL_SECS set).
    let app = with_response_cache(app);

    // One-shot Meridian programs seed (idempotent — no-op if any program
    // already exists). Best-effort; failure is logged and doesn't block the
    // server from starting.
    match manifold_lobby::seed::seed_if_empty(&app_state).await {
        Ok(report) => {
            if report.programs_created > 0 || report.memberships_created > 0 {
                println!(
                    "manifold-lobby seeded {} programs, {} memberships",
                    report.programs_created, report.memberships_created
                );
            }
        }
        Err(e) => eprintln!("manifold-lobby seed warning: {e}"),
    }

    // Spawn the derivation engine in the background.
    let engine = Engine::new(app_state, SourceClients::from_env());
    let _engine_handle = engine.spawn();

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
        println!("manifold-lobby listening on port {port}");
        axum::serve(listener, app).await?;
    }
    Ok(())
}
