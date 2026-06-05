//! Edge plumbing for Manifold services.
//!
//! Caddy at the edge does authentication and injects a small set of trusted
//! identity headers. This crate's job is to lift those headers into a
//! [`meshql_restlette::AuthContext`] so meshql's `Auth` implementations
//! (typically `CasbinAuth<StashKeyAuth>`) can resolve the caller's roles.
//!
//! The contract between Caddy and the services is deployment-time: header
//! names are configurable per-customer via [`HeaderConfig`]. Inside the
//! service, the populated `Stash` uses fixed canonical keys (`user_id`,
//! `groups`) so `Auth` impls are deployment-agnostic.
//!
//! # Example
//!
//! ```no_run
//! use axum::Router;
//! use manifold_edge::{with_header_identity, HeaderConfig};
//!
//! let cfg = HeaderConfig::from_env();
//! let app: Router = with_header_identity(Router::new(), cfg);
//! ```

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderName},
    middleware::{from_fn_with_state, Next},
    response::Response,
    Router,
};
use meshql_core::{AuthContext, Stash};
use serde_json::{json, Value};

mod access;
use std::str::FromStr;

/// Canonical Stash keys that downstream Auth implementations read.
pub const STASH_KEY_USER_ID: &str = "user_id";
pub const STASH_KEY_GROUPS: &str = "groups";

/// Per-deployment mapping from HTTP header names to canonical Stash keys.
///
/// Different customers may use different edge auth and therefore different
/// header names (`X-Forwarded-User`, `X-MS-CLIENT-PRINCIPAL-NAME`, etc.).
/// `HeaderConfig` is constructed from env vars at startup and stays
/// constant for the life of the service.
#[derive(Clone, Debug)]
pub struct HeaderConfig {
    pub user_id_header: HeaderName,
    pub groups_header: HeaderName,
    /// Fallback identity applied when no user-id header is present — e.g. a
    /// public, read-only demo with no edge auth in front. Off (None) unless
    /// `DEMO_USER_ID` is set, so normal edge-authed deploys are unaffected.
    pub default_user_id: Option<String>,
    pub default_groups: Option<String>,
    /// Whether to trust the inbound identity headers. True for deployments
    /// where a trusted edge (e.g. Caddy) injects them and the service isn't
    /// directly reachable. Set false when the origin IS reachable (e.g. behind
    /// a CDN that can be bypassed): then identity comes only from a verified
    /// Access JWT or the demo fallback, never from forgeable headers.
    pub trust_headers: bool,
}

impl HeaderConfig {
    /// Construct from explicit header names.
    pub fn new(
        user_id_header: impl AsRef<str>,
        groups_header: impl AsRef<str>,
    ) -> Result<Self, http::header::InvalidHeaderName> {
        Ok(Self {
            user_id_header: HeaderName::from_str(user_id_header.as_ref())?,
            groups_header: HeaderName::from_str(groups_header.as_ref())?,
            default_user_id: None,
            default_groups: None,
            trust_headers: true,
        })
    }

    /// Construct from `MANIFOLD_USER_HEADER` and `MANIFOLD_GROUPS_HEADER`
    /// env vars, defaulting to `X-Manifold-User-Id` and
    /// `X-Manifold-User-Groups`.
    pub fn from_env() -> Self {
        let user = std::env::var("MANIFOLD_USER_HEADER")
            .unwrap_or_else(|_| "X-Manifold-User-Id".to_string());
        let groups = std::env::var("MANIFOLD_GROUPS_HEADER")
            .unwrap_or_else(|_| "X-Manifold-User-Groups".to_string());
        let mut cfg = Self::new(user, groups).expect("valid header names in env vars");
        // Public-demo fallback: when a request carries no identity header, act
        // as this user (e.g. a read-only `viewer`). Both unset in normal,
        // edge-authed deploys, so behaviour there is unchanged.
        cfg.default_user_id = std::env::var("DEMO_USER_ID").ok().filter(|s| !s.is_empty());
        cfg.default_groups = std::env::var("DEMO_USER_GROUPS").ok().filter(|s| !s.is_empty());
        // Trust inbound identity headers unless explicitly disabled. Set
        // MANIFOLD_TRUST_HEADERS=false where the origin is directly reachable so
        // forged X-Manifold-* headers are ignored (identity = JWT or demo only).
        cfg.trust_headers = std::env::var("MANIFOLD_TRUST_HEADERS")
            .map(|v| v != "false")
            .unwrap_or(true);
        cfg
    }
}

impl Default for HeaderConfig {
    fn default() -> Self {
        Self::new("X-Manifold-User-Id", "X-Manifold-User-Groups")
            .expect("default header names are valid")
    }
}

/// Add the header-identity middleware to `router`.
///
/// Every incoming request gets its identity headers lifted into a
/// [`AuthContext`] request extension, which downstream meshql handlers
/// read via the request-scoped Stash.
pub fn with_header_identity<S>(router: Router<S>, cfg: HeaderConfig) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(from_fn_with_state(cfg, identity_middleware))
}

async fn identity_middleware(
    State(cfg): State<HeaderConfig>,
    mut req: Request,
    next: Next,
) -> Response {
    // A verified Cloudflare Access JWT wins (it can't be forged); otherwise fall
    // back to the trusted-header / demo identity.
    let stash = match access::verify(req.headers()).await {
        Some((email, roles)) => identity_stash(email, roles),
        None => build_stash(req.headers(), &cfg),
    };
    // axum's Extension<T> extractor reads `T` directly out of request
    // extensions; insert the bare value, not the wrapping Extension.
    req.extensions_mut().insert(AuthContext(stash));
    next.run(req).await
}

/// Build a Stash directly from an already-verified identity (the Access path).
fn identity_stash(user_id: String, groups: Vec<String>) -> Stash {
    let mut stash = Stash::new();
    stash.insert(STASH_KEY_USER_ID.to_string(), Value::String(user_id));
    if !groups.is_empty() {
        stash.insert(STASH_KEY_GROUPS.to_string(), json!(groups));
    }
    stash
}

/// Pure function: read identity headers and produce a Stash with canonical keys.
/// Exposed for unit tests and for callers that prefer to populate the Stash
/// outside of the axum middleware pipeline.
pub fn build_stash(headers: &HeaderMap, cfg: &HeaderConfig) -> Stash {
    let mut stash = Stash::new();
    let mut have_id = false;
    // Only read the identity headers when we trust them. When we don't (the
    // origin is directly reachable), forged X-Manifold-* must not grant a role —
    // identity then comes solely from a verified JWT or the demo fallback.
    if cfg.trust_headers {
        if let Some(id) = headers
            .get(&cfg.user_id_header)
            .and_then(|v| v.to_str().ok())
        {
            if !id.is_empty() {
                stash.insert(STASH_KEY_USER_ID.to_string(), Value::String(id.to_string()));
                have_id = true;
            }
        }
        if let Some(groups) = headers
            .get(&cfg.groups_header)
            .and_then(|v| v.to_str().ok())
        {
            let parsed: Vec<Value> = groups
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Value::String(s.to_string()))
                .collect();
            if !parsed.is_empty() {
                stash.insert(STASH_KEY_GROUPS.to_string(), json!(parsed));
            }
        }
    }
    // No identity header (e.g. a public demo with no edge auth in front): fall
    // back to the configured demo identity, if any. The default groups replace
    // anything parsed above so the fallback identity is internally consistent.
    if !have_id {
        if let Some(def_id) = cfg.default_user_id.as_deref().filter(|s| !s.is_empty()) {
            stash.insert(
                STASH_KEY_USER_ID.to_string(),
                Value::String(def_id.to_string()),
            );
            if let Some(def_groups) = cfg.default_groups.as_deref() {
                let parsed: Vec<Value> = def_groups
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| Value::String(s.to_string()))
                    .collect();
                if !parsed.is_empty() {
                    stash.insert(STASH_KEY_GROUPS.to_string(), json!(parsed));
                }
            }
        }
    }
    stash
}

// ── Read-through response cache ─────────────────────────────────────────────
//
// A per-instance, TTL'd in-memory cache for READ responses. For a read-only
// deployment (writes blocked at the edge) this takes the database out of the
// hot path: each warm instance hits the store at most once per distinct read
// per TTL window. Enabled only when `CACHE_TTL_SECS` is set (> 0); unset = a
// transparent pass-through, so normal deployments are unaffected.
//
// Cacheable = `GET`, or `POST` to a `/graph` path (GraphQL queries). Restlette
// writes (`POST/PUT/PATCH/DELETE` on `/api`) pass straight through. The key
// includes the raw identity headers so callers with different roles never share
// an entry, and (for POST) the request body so different queries don't collide.
// Error responses are never cached — including GraphQL 200s whose body carries
// an `errors` array — so a transient overload can't get pinned for the TTL.

use axum::body::{to_bytes, Body};
use axum::http::{header::CONTENT_TYPE, HeaderValue, Method, StatusCode};
use axum::middleware::from_fn;
use bytes::Bytes;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

struct CachedResponse {
    status: StatusCode,
    content_type: Option<HeaderValue>,
    body: Bytes,
}

static RESPONSE_CACHE: LazyLock<Option<moka::future::Cache<String, Arc<CachedResponse>>>> =
    LazyLock::new(|| {
        let ttl = std::env::var("CACHE_TTL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        if ttl == 0 {
            return None;
        }
        Some(
            moka::future::Cache::builder()
                .time_to_live(Duration::from_secs(ttl))
                .max_capacity(10_000)
                .build(),
        )
    });

/// Wrap `router` with the read-through response cache. No-op unless
/// `CACHE_TTL_SECS` is set.
pub fn with_response_cache<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(from_fn(cache_middleware))
}

async fn cache_middleware(req: Request, next: Next) -> Response {
    let Some(cache) = RESPONSE_CACHE.as_ref() else {
        return next.run(req).await;
    };

    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let is_graph = path.contains("/graph");
    let cacheable = method == Method::GET || (method == Method::POST && is_graph);
    if !cacheable {
        return next.run(req).await;
    }

    let query = req.uri().query().unwrap_or("").to_string();
    let hdr = |h: &HeaderMap, n: &str| {
        h.get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let uid = hdr(req.headers(), "x-manifold-user-id");
    let grp = hdr(req.headers(), "x-manifold-user-groups");

    // Buffer the request body (needed in the key for POST graph queries).
    let (parts, body) = req.into_parts();
    let body_bytes = if method == Method::POST {
        to_bytes(body, 1 << 20).await.unwrap_or_default()
    } else {
        Bytes::new()
    };
    let key = format!(
        "{method}|{path}?{query}|{uid}|{grp}|{}",
        String::from_utf8_lossy(&body_bytes)
    );

    if let Some(hit) = cache.get(&key).await {
        let mut b = Response::builder()
            .status(hit.status)
            .header("x-manifold-cache", "hit");
        if let Some(ct) = &hit.content_type {
            b = b.header(CONTENT_TYPE, ct);
        }
        return b.body(Body::from(hit.body.clone())).unwrap();
    }

    let req = Request::from_parts(parts, Body::from(body_bytes));
    let resp = next.run(req).await;
    let (rparts, rbody) = resp.into_parts();
    let rbytes = to_bytes(rbody, 8 << 20).await.unwrap_or_default();

    // Never cache errors: non-2xx, or a GraphQL 200 carrying an `errors` array.
    let graph_error = is_graph
        && rbytes.windows(9).any(|w| w == b"\"errors\":")
        && !rbytes.windows(13).any(|w| w == b"\"errors\":null");
    if rparts.status.is_success() && !graph_error {
        cache
            .insert(
                key,
                Arc::new(CachedResponse {
                    status: rparts.status,
                    content_type: rparts.headers.get(CONTENT_TYPE).cloned(),
                    body: rbytes.clone(),
                }),
            )
            .await;
    }

    let mut out = Response::from_parts(rparts, Body::from(rbytes));
    out.headers_mut()
        .insert("x-manifold-cache", HeaderValue::from_static("miss"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn cfg() -> HeaderConfig {
        HeaderConfig::default()
    }

    #[test]
    fn build_stash_populates_user_id_from_default_header() {
        let mut h = HeaderMap::new();
        h.insert(
            "x-manifold-user-id",
            HeaderValue::from_static("alice@example.dev"),
        );
        let s = build_stash(&h, &cfg());
        assert_eq!(
            s.get(STASH_KEY_USER_ID),
            Some(&Value::String("alice@example.dev".to_string()))
        );
    }

    #[test]
    fn build_stash_splits_groups() {
        let mut h = HeaderMap::new();
        h.insert(
            "x-manifold-user-groups",
            HeaderValue::from_static("admin, engineering"),
        );
        let s = build_stash(&h, &cfg());
        let groups = s.get(STASH_KEY_GROUPS).unwrap().as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], Value::String("admin".to_string()));
        assert_eq!(groups[1], Value::String("engineering".to_string()));
    }

    #[test]
    fn build_stash_empty_when_no_headers() {
        let h = HeaderMap::new();
        let s = build_stash(&h, &cfg());
        assert!(s.is_empty());
    }

    #[test]
    fn build_stash_respects_custom_header_names() {
        let custom = HeaderConfig::new("X-Whatever-User", "X-Whatever-Groups").unwrap();
        let mut h = HeaderMap::new();
        h.insert("x-whatever-user", HeaderValue::from_static("bob"));
        let s = build_stash(&h, &custom);
        assert_eq!(s.get(STASH_KEY_USER_ID), Some(&Value::String("bob".into())));
    }

    #[test]
    fn build_stash_ignores_empty_header_values() {
        let mut h = HeaderMap::new();
        h.insert("x-manifold-user-id", HeaderValue::from_static(""));
        let s = build_stash(&h, &cfg());
        assert!(s.is_empty());
    }

    #[test]
    fn build_stash_applies_demo_default_when_no_header() {
        let mut c = HeaderConfig::default();
        c.default_user_id = Some("demo@manifold.tailoredshapes.com".to_string());
        c.default_groups = Some("viewer".to_string());
        let s = build_stash(&HeaderMap::new(), &c);
        assert_eq!(
            s.get(STASH_KEY_USER_ID),
            Some(&Value::String("demo@manifold.tailoredshapes.com".to_string()))
        );
        let groups = s.get(STASH_KEY_GROUPS).unwrap().as_array().unwrap();
        assert_eq!(groups, &[Value::String("viewer".to_string())]);
    }

    #[test]
    fn build_stash_header_identity_wins_over_demo_default() {
        let mut c = HeaderConfig::default();
        c.default_user_id = Some("demo@manifold.tailoredshapes.com".to_string());
        c.default_groups = Some("viewer".to_string());
        let mut h = HeaderMap::new();
        h.insert(
            "x-manifold-user-id",
            HeaderValue::from_static("alice@example.dev"),
        );
        h.insert("x-manifold-user-groups", HeaderValue::from_static("admin"));
        let s = build_stash(&h, &c);
        assert_eq!(
            s.get(STASH_KEY_USER_ID),
            Some(&Value::String("alice@example.dev".to_string()))
        );
        let groups = s.get(STASH_KEY_GROUPS).unwrap().as_array().unwrap();
        assert_eq!(groups, &[Value::String("admin".to_string())]);
    }
}
