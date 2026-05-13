//! HTTP-backed lookup of Groundwork Deployable + dependency graph for the
//! Yard estimator.
//!
//! Identical shape to cityhall::groundwork_client — same `DeployableSummary`,
//! same edge convention. Yard depends on it for the estimator walk.

use crate::estimator::{DeployableSummary, GroundworkLookup};
use async_trait::async_trait;

pub struct HttpGroundworkClient {
    base_url: String,
    http: reqwest::Client,
}

impl HttpGroundworkClient {
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("GROUNDWORK_URL").unwrap_or_else(|_| "http://localhost:3000".into());
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
impl GroundworkLookup for HttpGroundworkClient {
    async fn get_deployable(&self, id: &str) -> anyhow::Result<Option<DeployableSummary>> {
        let url = format!("{}/deployable/api/{id}", self.base_url);
        let resp = self.http.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            anyhow::bail!("groundwork {url} -> {}", resp.status());
        }
        let env: serde_json::Value = resp.json().await?;
        // meshql-restlette flattens id+payload onto one object; tests sometimes
        // nest under "payload". Accept either shape.
        let payload = match env.get("payload") {
            Some(v) if v.is_object() => v.clone(),
            _ => env.clone(),
        };
        let name = payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let team_id = payload
            .get("team_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);

        let dep_query =
            format!("{{ getByDeployableId(deployable_id: \"{id}\") {{ service_id }} }}");
        let dep_url = format!("{}/dependency/graph", self.base_url);
        let dep_body = serde_json::json!({ "query": dep_query });
        let dep_resp = self.http.post(&dep_url).json(&dep_body).send().await?;
        let dep_value: serde_json::Value = dep_resp.json().await?;
        let service_ids: Vec<String> = dep_value
            .get("data")
            .and_then(|d| d.get("getByDeployableId"))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        d.get("service_id")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut depends_on: Vec<String> = Vec::new();
        for svc_id in &service_ids {
            let exposes_query =
                format!("{{ getByServiceId(service_id: \"{svc_id}\") {{ deployable_id }} }}");
            let exposes_url = format!("{}/exposes/graph", self.base_url);
            let exposes_body = serde_json::json!({ "query": exposes_query });
            let exposes_resp = self
                .http
                .post(&exposes_url)
                .json(&exposes_body)
                .send()
                .await?;
            let v: serde_json::Value = exposes_resp.json().await?;
            if let Some(arr) = v
                .get("data")
                .and_then(|d| d.get("getByServiceId"))
                .and_then(|a| a.as_array())
            {
                for e in arr {
                    if let Some(d) = e.get("deployable_id").and_then(|v| v.as_str()) {
                        if !d.is_empty() && !depends_on.contains(&d.to_string()) {
                            depends_on.push(d.to_string());
                        }
                    }
                }
            }
        }

        Ok(Some(DeployableSummary {
            id: id.to_string(),
            name,
            team_id,
            depends_on,
            depends_on_services: service_ids,
        }))
    }
}
