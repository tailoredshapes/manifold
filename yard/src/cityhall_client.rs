//! HTTP-backed lookup of Cityhall ChangeRequest for the estimator's
//! `change_request → estimate` route.

use crate::estimator::{ChangeRequestLookup, ChangeRequestSummary};
use async_trait::async_trait;

pub struct HttpCityhallClient {
    base_url: String,
    http: reqwest::Client,
}

impl HttpCityhallClient {
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("CITYHALL_URL").unwrap_or_else(|_| "http://localhost:3002".into());
        Self::new(base_url)
    }

    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ChangeRequestLookup for HttpCityhallClient {
    async fn get_change_request(&self, id: &str) -> anyhow::Result<Option<ChangeRequestSummary>> {
        let url = format!("{}/change_request/api/{id}", self.base_url);
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            anyhow::bail!("cityhall {url} -> {}", resp.status());
        }
        let env: serde_json::Value = resp.json().await?;
        let payload = env.get("payload").cloned().unwrap_or(serde_json::Value::Null);
        let summary = payload.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tier = payload.get("tier").and_then(|v| v.as_str()).map(String::from);
        let target_str = payload
            .get("target_deployables")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let target_deployables: Vec<String> = if target_str.trim().is_empty() {
            Vec::new()
        } else if let Ok(v) = serde_json::from_str::<Vec<String>>(&target_str) {
            v
        } else {
            target_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };

        Ok(Some(ChangeRequestSummary {
            id: id.to_string(),
            summary,
            tier,
            target_deployables,
        }))
    }
}
