//! Shared utilities for Manifold integration adapters.
//!
//! Every adapter is a separate Rust binary that:
//!
//! 1. Reads its system of record (GitHub / GitLab / Okta / file / …)
//! 2. Transforms records to canonical Manifold shape
//! 3. Idempotently writes to a primary-domain meshlette (find by
//!    `(external_system, external_id)` in `manifold-ingest`; PUT existing
//!    canonical_id or POST a new one)
//! 4. Records provenance in `manifold-ingest` after every new write
//!
//! Adapters run *inside the trust boundary* — they hold a service-issued
//! `on_behalf_of` identity (a human) and a role (e.g.
//! `automation:github-sync`). These are sent as trusted identity headers on
//! every outbound request, the same as the MCP servers do. Audit and
//! disaster recovery are graph queries against `manifold-ingest`.

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};

/// Configured adapter client. Construct once via [`ManifoldClient::from_env`]
/// and reuse across all upserts.
///
/// Holds the base URL of every primary-domain meshlette the adapter might
/// write to. `upsert(primary, …)` picks the right one — adapters that only
/// touch one domain (e.g. catalog-from-github) just always pass
/// `"groundwork"`; multi-domain adapters (e.g. union-from-okta writing to
/// multiple union entities) reuse the same client.
pub struct ManifoldClient {
    primary_urls: std::collections::HashMap<&'static str, String>,
    ingest_url: String,
    user_id: String,
    groups: String,
    http: reqwest::Client,
}

/// Result of an idempotent upsert. `created=true` means the row was new and
/// a provenance record was written to `manifold-ingest`.
#[derive(Debug)]
pub struct UpsertResult {
    pub canonical_id: String,
    pub created: bool,
}

impl ManifoldClient {
    /// Read configuration from the environment:
    ///
    /// - `MANIFOLD_GROUNDWORK_URL`  (default `http://localhost:3050`)
    /// - `MANIFOLD_UNION_URL`       (default `http://localhost:3051`)
    /// - `MANIFOLD_CITYHALL_URL`    (default `http://localhost:3052`)
    /// - `MANIFOLD_YARD_URL`        (default `http://localhost:3053`)
    /// - `MANIFOLD_INGEST_URL`      (default `http://localhost:3054`)
    /// - `MANIFOLD_USER_ID`         **required** — the human on whose behalf
    ///                              the adapter is running
    /// - `MANIFOLD_USER_GROUPS`     comma-separated role list; should
    ///                              include the adapter's automation role
    ///                              (e.g. `automation:github-sync`)
    pub fn from_env() -> Result<Self> {
        let mut primary_urls = std::collections::HashMap::new();
        primary_urls.insert(
            "groundwork",
            std::env::var("MANIFOLD_GROUNDWORK_URL")
                .unwrap_or_else(|_| "http://localhost:3050".into()),
        );
        primary_urls.insert(
            "union",
            std::env::var("MANIFOLD_UNION_URL").unwrap_or_else(|_| "http://localhost:3051".into()),
        );
        primary_urls.insert(
            "cityhall",
            std::env::var("MANIFOLD_CITYHALL_URL")
                .unwrap_or_else(|_| "http://localhost:3052".into()),
        );
        primary_urls.insert(
            "yard",
            std::env::var("MANIFOLD_YARD_URL").unwrap_or_else(|_| "http://localhost:3053".into()),
        );

        let ingest_url =
            std::env::var("MANIFOLD_INGEST_URL").unwrap_or_else(|_| "http://localhost:3054".into());
        let user_id = std::env::var("MANIFOLD_USER_ID").with_context(|| {
            "MANIFOLD_USER_ID is required (the human on whose behalf this adapter runs)"
        })?;
        let groups = std::env::var("MANIFOLD_USER_GROUPS").unwrap_or_default();
        Ok(Self {
            primary_urls,
            ingest_url,
            user_id,
            groups,
            http: reqwest::Client::new(),
        })
    }

    /// Construct explicitly with a `primary => base_url` map. Mostly useful
    /// for tests.
    pub fn new<I, K, V>(
        primaries: I,
        ingest_url: impl Into<String>,
        user_id: impl Into<String>,
        groups: impl Into<String>,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<&'static str>,
        V: Into<String>,
    {
        Self {
            primary_urls: primaries
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            ingest_url: ingest_url.into(),
            user_id: user_id.into(),
            groups: groups.into(),
            http: reqwest::Client::new(),
        }
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&self.user_id) {
            h.insert("X-Manifold-User-Id", v);
        }
        if !self.groups.is_empty() {
            if let Ok(v) = HeaderValue::from_str(&self.groups) {
                h.insert("X-Manifold-User-Groups", v);
            }
        }
        h
    }

    /// Look up an ingestion record by `(external_system, external_id)`.
    /// Returns the canonical id if a row exists, `None` otherwise.
    pub async fn find_canonical_id(
        &self,
        external_system: &str,
        external_id: &str,
    ) -> Result<Option<String>> {
        let q = format!(
            r#"{{ getByExternalSystem(external_system: "{}") {{ id external_id canonical_id }} }}"#,
            escape_graphql_string(external_system)
        );
        let resp: Value = self
            .http
            .post(format!("{}/ingestion/graph", self.ingest_url))
            .headers(self.auth_headers())
            .json(&json!({ "query": q }))
            .send()
            .await
            .with_context(|| "POST /ingestion/graph")?
            .json()
            .await
            .with_context(|| "parse /ingestion/graph response")?;
        if let Some(errors) = resp.get("errors").and_then(|v| v.as_array()) {
            if !errors.is_empty() {
                return Err(anyhow!("graphql errors: {:?}", errors));
            }
        }
        let rows = match resp
            .pointer("/data/getByExternalSystem")
            .and_then(|v| v.as_array())
        {
            Some(rs) => rs,
            None => return Ok(None),
        };
        for row in rows {
            if row.get("external_id").and_then(|v| v.as_str()) == Some(external_id) {
                return Ok(row
                    .get("canonical_id")
                    .and_then(|v| v.as_str())
                    .map(String::from));
            }
        }
        Ok(None)
    }

    /// Idempotently upsert a record into a primary-domain meshlette.
    ///
    /// - `primary` selects the base URL: `"groundwork"`, `"union"`,
    ///   `"cityhall"`, or `"yard"`.
    /// - `entity_path` is the meshlette's REST root for the entity, e.g.
    ///   `"/deployable/api"`, `"/team/api"`, `"/test_run/api"`.
    /// - `target_domain` is the value written to the provenance row,
    ///   e.g. `"groundwork.deployable"`.
    pub async fn upsert(
        &self,
        primary: &str,
        entity_path: &str,
        target_domain: &str,
        external_system: &str,
        external_id: &str,
        via_role: &str,
        payload: Value,
        raw: Value,
    ) -> Result<UpsertResult> {
        let primary_base = self
            .primary_urls
            .get(primary)
            .ok_or_else(|| {
                anyhow!(
                    "unknown primary domain `{primary}` — expected one of {:?}",
                    self.primary_urls.keys().collect::<Vec<_>>()
                )
            })?
            .trim_end_matches('/')
            .to_string();
        let existing = self.find_canonical_id(external_system, external_id).await?;

        if let Some(canonical_id) = existing {
            let url = format!("{primary_base}{entity_path}/{canonical_id}");
            let resp = self
                .http
                .put(&url)
                .headers(self.auth_headers())
                .json(&payload)
                .send()
                .await
                .with_context(|| format!("PUT {url}"))?;
            if !resp.status().is_success() {
                anyhow::bail!("PUT {url} -> {}", resp.status());
            }
            return Ok(UpsertResult {
                canonical_id,
                created: false,
            });
        }

        let url = format!("{primary_base}{entity_path}");
        let resp = self
            .http
            .post(&url)
            .headers(self.auth_headers())
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("POST {url} -> {}", resp.status());
        }
        let created: Value = resp.json().await.with_context(|| format!("parse {url}"))?;
        let canonical_id = created
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("primary-domain create returned no id: {created}"))?
            .to_string();

        let ingest_body = json!({
            "external_system": external_system,
            "external_id": external_id,
            "target_domain": target_domain,
            "canonical_id": canonical_id.clone(),
            "on_behalf_of": self.user_id,
            "via_role": via_role,
            "raw": raw,
        });
        let ingest_url = format!("{}/ingestion/api", self.ingest_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&ingest_url)
            .headers(self.auth_headers())
            .json(&ingest_body)
            .send()
            .await
            .with_context(|| format!("POST {ingest_url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("POST {ingest_url} -> {}", resp.status());
        }

        Ok(UpsertResult {
            canonical_id,
            created: true,
        })
    }
}

/// Escape characters that need quoting inside a GraphQL string literal.
fn escape_graphql_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str(r#"\""#),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c => out.push(c),
        }
    }
    out
}

/// Parse `Link: <url>; rel="next", ...` into the next URL, if any.
/// Used by both GitHub and GitLab — both APIs paginate the same way.
pub fn parse_next_link(link: &str) -> Option<String> {
    for part in link.split(',') {
        let part = part.trim();
        if part.contains(r#"rel="next""#) {
            if let Some(seg) = part.split(';').next() {
                let url = seg.trim().trim_start_matches('<').trim_end_matches('>');
                if !url.is_empty() {
                    return Some(url.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(escape_graphql_string(r#"abc"def\ghi"#), r#"abc\"def\\ghi"#);
    }

    #[test]
    fn parses_next_link_when_present() {
        let h = r#"<https://api.example.com/repos?page=2>; rel="next", <https://api.example.com/repos?page=10>; rel="last""#;
        assert_eq!(
            parse_next_link(h),
            Some("https://api.example.com/repos?page=2".to_string())
        );
    }

    #[test]
    fn parses_next_link_returns_none_without_rel_next() {
        let h = r#"<https://api.example.com/repos?page=10>; rel="last""#;
        assert_eq!(parse_next_link(h), None);
    }

    #[test]
    fn parses_next_link_handles_single_rel_next() {
        let h = r#"<https://gitlab.com/api/v4/projects?page=3>; rel="next""#;
        assert_eq!(
            parse_next_link(h),
            Some("https://gitlab.com/api/v4/projects?page=3".to_string())
        );
    }
}
