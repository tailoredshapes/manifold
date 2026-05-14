//! HTTP clients for the four federated source meshlettes. Today this is a
//! polling fetch over each meshlette's `/graph` endpoint; when a real
//! event-stream adapter (merkql-notify, Mongo+Debezium+Kafka, …) is wired
//! in, this module is the only thing that changes.

use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use serde_json::Value;

use crate::snapshot::*;

pub struct SourceClients {
    http: reqwest::Client,
    groundwork_url: String,
    cityhall_url: String,
    yard_url: String,
    union_url: String,
    /// Sent as `X-Manifold-User-Id` on every outbound call so the source
    /// meshlettes' Casbin auth resolves the engine's role correctly.
    user_id: String,
    groups: String,
}

impl SourceClients {
    pub fn from_env() -> Self {
        Self {
            http: reqwest::Client::new(),
            groundwork_url: std::env::var("GROUNDWORK_URL")
                .unwrap_or_else(|_| "http://localhost:3050".into()),
            cityhall_url: std::env::var("CITYHALL_URL")
                .unwrap_or_else(|_| "http://localhost:3052".into()),
            yard_url: std::env::var("YARD_URL").unwrap_or_else(|_| "http://localhost:3053".into()),
            union_url: std::env::var("UNION_URL")
                .unwrap_or_else(|_| "http://localhost:3051".into()),
            user_id: std::env::var("MANIFOLD_USER_ID").unwrap_or_else(|_| "lobby-system".into()),
            groups: std::env::var("MANIFOLD_USER_GROUPS")
                .unwrap_or_else(|_| "automation:lobby-derive".into()),
        }
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&self.user_id) {
            h.insert("X-Manifold-User-Id", v);
        }
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&self.groups) {
            h.insert("X-Manifold-User-Groups", v);
        }
        h
    }

    async fn gql(&self, url: &str, path: &str, query: &str) -> Result<Value> {
        let full = format!("{}{}", url, path);
        let resp = self
            .http
            .post(&full)
            .headers(self.auth_headers())
            .json(&serde_json::json!({ "query": query }))
            .send()
            .await
            .with_context(|| format!("POST {full}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("POST {full} -> {}", resp.status());
        }
        let body: Value = resp
            .json()
            .await
            .with_context(|| format!("decode {full}"))?;
        if let Some(errs) = body.get("errors").and_then(|e| e.as_array()) {
            if !errs.is_empty() {
                anyhow::bail!("graphql errors on {full}: {errs:?}");
            }
        }
        Ok(body.get("data").cloned().unwrap_or(Value::Null))
    }

    pub async fn fetch_snapshot(&self) -> Result<GraphSnapshot> {
        let mut snap = GraphSnapshot::default();

        // Groundwork
        let deployables = self
            .gql(
                &self.groundwork_url,
                "/deployable/graph",
                "{ getAll { id name description team_id deployment_status } }",
            )
            .await?;
        snap.deployables = parse_array(&deployables, "getAll")?;

        let services = self
            .gql(
                &self.groundwork_url,
                "/service/graph",
                "{ getAll { id name type description } }",
            )
            .await?;
        snap.services = parse_array(&services, "getAll")?;

        let dependencies = self
            .gql(
                &self.groundwork_url,
                "/dependency/graph",
                "{ getAll { id deployable_id service_id criticality } }",
            )
            .await?;
        snap.dependencies = parse_array(&dependencies, "getAll")?;

        let exposes = self
            .gql(
                &self.groundwork_url,
                "/exposes/graph",
                "{ getAll { id deployable_id service_id } }",
            )
            .await?;
        snap.exposes = parse_array(&exposes, "getAll")?;

        let contracts = self
            .gql(
                &self.groundwork_url,
                "/contract/graph",
                "{ getAll { id service_id format version } }",
            )
            .await?;
        snap.contracts = parse_array(&contracts, "getAll")?;

        // Cityhall
        let crs = self
            .gql(
                &self.cityhall_url,
                "/change_request/graph",
                "{ getAll { id summary status tier target_deployables } }",
            )
            .await?;
        snap.change_requests = parse_array(&crs, "getAll")?;

        // Deployment plans live with steps embedded as a JSON-encoded string.
        // We need REST to get the full envelope payload string.
        snap.deployment_plans = self.fetch_plans().await.unwrap_or_default();

        // Yard
        // Try the watershed-aware query first; fall back if the running
        // yard predates the schema addition. Lobby tolerates source-schema
        // drift — old yards just produce no WatershedMismatch advisories.
        let envs = match self
            .gql(
                &self.yard_url,
                "/test_environment/graph",
                "{ getAll { id name kind deployable_id watershed } }",
            )
            .await
        {
            Ok(v) => v,
            Err(_) => {
                self.gql(
                    &self.yard_url,
                    "/test_environment/graph",
                    "{ getAll { id name kind deployable_id } }",
                )
                .await?
            }
        };
        snap.test_environments = parse_array(&envs, "getAll")?;

        let syncs = self
            .gql(
                &self.yard_url,
                "/data_sync/graph",
                "{ getAll { id source_env_id target_env_id kind } }",
            )
            .await?;
        snap.data_syncs = parse_array(&syncs, "getAll")?;

        // Union
        let teams = self
            .gql(
                &self.union_url,
                "/team/graph",
                "{ getAll { id name kind } }",
            )
            .await?;
        snap.teams = parse_array(&teams, "getAll")?;

        let work_orders = self
            .gql(
                &self.union_url,
                "/work_order/graph",
                "{ getAll { id status deployable_id team_id } }",
            )
            .await?;
        snap.work_orders = parse_array(&work_orders, "getAll")?;

        Ok(snap)
    }

    async fn fetch_plans(&self) -> Result<Vec<DeploymentPlan>> {
        // /graph on deployment_plan only exposes the envelope-level fields.
        // The full step list is stored as a JSON-encoded string in `payload`.
        // Use the REST list endpoint to get the full envelope.
        let url = format!("{}/deployment_plan/api", self.cityhall_url);
        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("GET {url} -> {}", resp.status());
        }
        let arr: Vec<Value> = resp.json().await.with_context(|| format!("decode {url}"))?;
        let mut out: Vec<DeploymentPlan> = Vec::with_capacity(arr.len());
        for env in arr {
            let id = env
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let payload = env.get("payload").cloned().unwrap_or_default();
            let change_request_id = payload
                .get("change_request_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let steps_str = payload
                .get("steps")
                .and_then(|v| v.as_str())
                .unwrap_or("[]");
            let steps: Vec<PlanStepLite> = serde_json::from_str(steps_str).unwrap_or_default();
            out.push(DeploymentPlan {
                id,
                change_request_id,
                steps,
            });
        }
        Ok(out)
    }
}

fn parse_array<T: serde::de::DeserializeOwned>(value: &Value, key: &str) -> Result<Vec<T>> {
    let arr = value
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("expected `{key}` array in response"))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        out.push(serde_json::from_value(item.clone())?);
    }
    Ok(out)
}
