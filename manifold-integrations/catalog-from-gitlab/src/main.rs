//! catalog-from-gitlab — read projects from a GitLab group (or user) and
//! idempotently upsert them as Groundwork `Deployable` records, recording
//! provenance in `manifold-ingest`. Mirrors `catalog-from-github` for
//! GitLab; supports self-hosted GitLab via `--base-url`.
//!
//! Usage:
//!
//! ```text
//! export GITLAB_TOKEN=glpat-…
//! export MANIFOLD_USER_ID=alice@example.dev
//! export MANIFOLD_USER_GROUPS=automation:gitlab-sync
//! export MANIFOLD_GROUNDWORK_URL=http://localhost:3050
//! export MANIFOLD_INGEST_URL=http://localhost:3054
//! catalog-from-gitlab --group my-team
//! ```

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use manifold_adapter_common::{parse_next_link, ManifoldClient};
use serde::Deserialize;
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "catalog-from-gitlab", version)]
struct Args {
    /// GitLab group full path (e.g. `my-team` or `my-team/sub-team`).
    #[arg(long, group = "scope")]
    group: Option<String>,
    /// GitLab user name.
    #[arg(long, group = "scope")]
    user: Option<String>,
    /// Personal access token. Defaults to GITLAB_TOKEN env var.
    #[arg(long)]
    token: Option<String>,
    /// GitLab instance base URL (defaults to https://gitlab.com).
    #[arg(long, default_value = "https://gitlab.com")]
    base_url: String,
}

#[derive(Deserialize, Debug)]
struct Project {
    path_with_namespace: String,
    name: String,
    description: Option<String>,
    web_url: String,
    archived: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = args
        .token
        .clone()
        .or_else(|| std::env::var("GITLAB_TOKEN").ok())
        .ok_or_else(|| anyhow!("GitLab token required: --token or GITLAB_TOKEN env"))?;

    let role = std::env::var("MANIFOLD_ROLE").unwrap_or_else(|_| "automation:gitlab-sync".into());
    let manifold = ManifoldClient::from_env()?;
    let base = args.base_url.trim_end_matches('/').to_string();

    let initial = match (&args.group, &args.user) {
        (Some(g), _) => format!(
            "{base}/api/v4/groups/{}/projects?per_page=100&include_subgroups=true",
            urlencoding::encode_path(g)
        ),
        (None, Some(u)) => format!(
            "{base}/api/v4/users/{}/projects?per_page=100",
            urlencoding::encode_path(u)
        ),
        _ => anyhow::bail!("one of --group <name> or --user <name> is required"),
    };

    let http = reqwest::Client::new();
    let mut url = initial;
    let mut total = 0usize;
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;

    loop {
        let resp = http
            .get(&url)
            .header("PRIVATE-TOKEN", &token)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GET {url} -> {status}: {body}");
        }

        let next_url = resp
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_next_link);
        let projects: Vec<Project> = resp.json().await.with_context(|| "parse projects")?;

        for project in projects {
            if project.archived.unwrap_or(false) {
                skipped += 1;
                continue;
            }
            let payload = json!({
                "name": project.name,
                "description": project.description.clone().unwrap_or_default(),
                "repo_url": project.web_url,
            });
            let raw = json!({
                "path_with_namespace": project.path_with_namespace,
                "web_url": project.web_url,
            });
            let res = manifold
                .upsert_in_groundwork(
                    "/deployable/api",
                    "groundwork.deployable",
                    "gitlab",
                    &project.path_with_namespace,
                    &role,
                    payload,
                    raw,
                )
                .await
                .with_context(|| {
                    format!("upsert deployable for {}", project.path_with_namespace)
                })?;
            total += 1;
            if res.created {
                created += 1;
                println!(
                    "created  {} → {}",
                    project.path_with_namespace,
                    &res.canonical_id[..8]
                );
            } else {
                updated += 1;
                println!(
                    "updated  {} → {}",
                    project.path_with_namespace,
                    &res.canonical_id[..8]
                );
            }
        }

        match next_url {
            Some(u) => url = u,
            None => break,
        }
    }

    println!();
    println!(
        "{} projects processed: {} created, {} updated, {} skipped (archived)",
        total, created, updated, skipped
    );
    Ok(())
}

mod urlencoding {
    /// Minimal path-segment encoder — GitLab group paths can contain `/`
    /// that we need percent-encoded (e.g. `my-org/sub-group` becomes
    /// `my-org%2Fsub-group`). We only need this narrow encoding, not a
    /// general one — avoids pulling in a dep.
    pub fn encode_path(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{:02X}", b)),
            }
        }
        out
    }
}
