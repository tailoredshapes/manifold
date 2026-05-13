//! End-to-end test: drive a real axum router through the layer and assert
//! that downstream handlers see the populated `AuthContext`.

use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use http_body_util::BodyExt;
use manifold_edge::{with_header_identity, HeaderConfig, STASH_KEY_GROUPS, STASH_KEY_USER_ID};
use meshql_core::AuthContext;
use tower::ServiceExt;

async fn dump_auth_ctx(auth_ctx: Option<Extension<AuthContext>>) -> String {
    let stash = auth_ctx.map(|e| e.0 .0).unwrap_or_default();
    serde_json::to_string(&stash).unwrap()
}

fn router() -> Router {
    let cfg = HeaderConfig::default();
    with_header_identity(Router::new().route("/whoami", get(dump_auth_ctx)), cfg)
}

#[tokio::test]
async fn middleware_installs_auth_context_from_headers() {
    let app = router();
    let req = Request::builder()
        .method("GET")
        .uri("/whoami")
        .header("X-Manifold-User-Id", "alice@example.dev")
        .header("X-Manifold-User-Groups", "admin, engineering")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(
        body_str.contains(&format!("\"{}\":\"alice@example.dev\"", STASH_KEY_USER_ID)),
        "body = {body_str}"
    );
    assert!(body_str.contains(STASH_KEY_GROUPS), "body = {body_str}");
    assert!(body_str.contains("admin"), "body = {body_str}");
}

#[tokio::test]
async fn middleware_yields_empty_stash_when_headers_absent() {
    let app = router();
    let req = Request::builder()
        .method("GET")
        .uri("/whoami")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert_eq!(body_str, "{}");
}
