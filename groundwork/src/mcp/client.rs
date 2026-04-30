//! Thin HTTP client wrapping the Groundwork REST API.
//!
//! Used by both `groundwork-mcp`'s graph snapshot loader and its catalog tools.
//! Reads `GROUNDWORK_URL` from the env (defaulting to `http://localhost:3000`)
//! when constructed via [`GroundworkClient::from_env`].

use anyhow::Context;
use serde_json::Value;

pub struct GroundworkClient {
    base_url: String,
    http: reqwest::Client,
}

impl GroundworkClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Self {
        let base_url =
            std::env::var("GROUNDWORK_URL").unwrap_or_else(|_| "http://localhost:3000".into());
        Self::new(base_url)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// `GET /<entity>/api` — returns the array of envelopes (id + payload fields).
    pub async fn list(&self, entity: &str) -> anyhow::Result<Value> {
        let url = format!("{}/{entity}/api", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("GET {url} -> {}", resp.status());
        }
        resp.json::<Value>().await.with_context(|| format!("decode {url}"))
    }

    /// `GET /<entity>/api/<id>` — returns the envelope, or `None` on 404.
    pub async fn get(&self, entity: &str, id: &str) -> anyhow::Result<Option<Value>> {
        let url = format!("{}/{entity}/api/{id}", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            anyhow::bail!("GET {url} -> {}", resp.status());
        }
        let v = resp.json::<Value>().await.with_context(|| format!("decode {url}"))?;
        Ok(Some(v))
    }
}
