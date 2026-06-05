//! Cloudflare Access JWT verification.
//!
//! When a request comes through a Cloudflare Access-protected hostname,
//! Cloudflare adds a signed JWT in the `Cf-Access-Jwt-Assertion` header. We
//! verify it against the team's public keys (JWKS) — so the authenticated
//! identity can't be forged by a caller who bypasses Cloudflare and hits the
//! origin directly. On success we return the verified email plus the role it
//! maps to (admin for the configured team domain, viewer for everyone else —
//! i.e. SSO prospects get read-only).
//!
//! Configured via env; absent config = disabled (returns `None`, so the caller
//! falls back to the trusted-header / demo identity path).

use http::HeaderMap;
use moka::future::Cache;
use serde::Deserialize;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

#[derive(Deserialize, Clone)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Claims {
    email: Option<String>,
}

struct AccessConfig {
    /// Zero Trust team host, e.g. `tildarc.cloudflareaccess.com`.
    team_domain: String,
    /// The Access application's AUD tag (the JWT `aud`).
    aud: String,
    /// Email domain that maps to the `admin` role; everyone else -> `viewer`.
    admin_domain: String,
}

impl AccessConfig {
    fn from_env() -> Option<Self> {
        let team = std::env::var("ACCESS_TEAM_DOMAIN").ok().filter(|s| !s.is_empty())?;
        let aud = std::env::var("ACCESS_AUD").ok().filter(|s| !s.is_empty())?;
        Some(Self {
            team_domain: team,
            aud,
            admin_domain: std::env::var("ACCESS_ADMIN_DOMAIN").unwrap_or_default(),
        })
    }
}

static CONFIG: LazyLock<Option<AccessConfig>> = LazyLock::new(AccessConfig::from_env);

static JWKS: LazyLock<Cache<String, Arc<Vec<Jwk>>>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(3600))
        .max_capacity(4)
        .build()
});

/// Verify `Cf-Access-Jwt-Assertion`. `Some((email, roles))` when a valid token
/// is present and config is set; `None` otherwise (disabled, no token, or
/// invalid — the caller then uses its normal identity path).
pub async fn verify(headers: &HeaderMap) -> Option<(String, Vec<String>)> {
    let cfg = CONFIG.as_ref()?;
    let token = headers
        .get("cf-access-jwt-assertion")
        .and_then(|v| v.to_str().ok())?;

    let kid = jsonwebtoken::decode_header(token).ok()?.kid?;
    let keys = jwks(cfg).await?;
    let jwk = keys.iter().find(|k| k.kid == kid)?;
    let key = jsonwebtoken::DecodingKey::from_rsa_components(&jwk.n, &jwk.e).ok()?;

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[cfg.aud.as_str()]);
    validation.set_issuer(&[format!("https://{}", cfg.team_domain).as_str()]);

    let data = jsonwebtoken::decode::<Claims>(token, &key, &validation).ok()?;
    let email = data.claims.email?;
    let roles = roles_for(&email, &cfg.admin_domain);
    Some((email, roles))
}

fn roles_for(email: &str, admin_domain: &str) -> Vec<String> {
    let is_admin = !admin_domain.is_empty()
        && email
            .rsplit('@')
            .next()
            .is_some_and(|d| d.eq_ignore_ascii_case(admin_domain));
    vec![if is_admin { "admin" } else { "viewer" }.to_string()]
}

async fn jwks(cfg: &AccessConfig) -> Option<Arc<Vec<Jwk>>> {
    let url = format!("https://{}/cdn-cgi/access/certs", cfg.team_domain);
    if let Some(cached) = JWKS.get(&url).await {
        return Some(cached);
    }
    let fetched = reqwest::get(&url).await.ok()?.json::<Jwks>().await.ok()?;
    let keys = Arc::new(fetched.keys);
    JWKS.insert(url, keys.clone()).await;
    Some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_domain_maps_to_admin_else_viewer() {
        assert_eq!(roles_for("tom@tailoredshapes.com", "tailoredshapes.com"), vec!["admin"]);
        assert_eq!(roles_for("Tom@TailoredShapes.com", "tailoredshapes.com"), vec!["admin"]);
        assert_eq!(roles_for("lead@gmail.com", "tailoredshapes.com"), vec!["viewer"]);
        assert_eq!(roles_for("anyone@x.io", ""), vec!["viewer"]);
    }
}
