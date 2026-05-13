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
        Self::new(user, groups).expect("valid header names in env vars")
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
    let stash = build_stash(req.headers(), &cfg);
    // axum's Extension<T> extractor reads `T` directly out of request
    // extensions; insert the bare value, not the wrapping Extension.
    req.extensions_mut().insert(AuthContext(stash));
    next.run(req).await
}

/// Pure function: read identity headers and produce a Stash with canonical keys.
/// Exposed for unit tests and for callers that prefer to populate the Stash
/// outside of the axum middleware pipeline.
pub fn build_stash(headers: &HeaderMap, cfg: &HeaderConfig) -> Stash {
    let mut stash = Stash::new();
    if let Some(id) = headers
        .get(&cfg.user_id_header)
        .and_then(|v| v.to_str().ok())
    {
        if !id.is_empty() {
            stash.insert(STASH_KEY_USER_ID.to_string(), Value::String(id.to_string()));
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
    stash
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
}
