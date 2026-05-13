//! catalog-from-github — read repositories from a GitHub org or user and
//! idempotently upsert them as Groundwork `Deployable` records, recording
//! provenance in `manifold-ingest`.
//!
//! Usage:
//!
//! ```text
//! export GITHUB_TOKEN=ghp_…
//! export MANIFOLD_USER_ID=alice@example.dev
//! export MANIFOLD_USER_GROUPS=automation:github-sync
//! export MANIFOLD_GROUNDWORK_URL=http://localhost:3050
//! export MANIFOLD_INGEST_URL=http://localhost:3054
//! catalog-from-github --target tailoredshapes
//! ```
//!
//! Re-running is idempotent: existing repos are looked up by
//! `(external_system="github", external_id="<owner>/<repo>")` in
//! `manifold-ingest` and the canonical Groundwork record is PUT-updated.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use manifold_adapter_common::{parse_next_link, ManifoldClient};
use serde::Deserialize;
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "catalog-from-github", version)]
struct Args {
    /// GitHub org name to crawl. (User crawl: pass `--user`.)
    #[arg(long, group = "scope")]
    target: Option<String>,
    /// GitHub user name to crawl.
    #[arg(long, group = "scope")]
    user: Option<String>,
    /// Personal access token. Defaults to GITHUB_TOKEN env var.
    #[arg(long)]
    token: Option<String>,
    /// GitHub API base URL (override for enterprise).
    #[arg(long, default_value = "https://api.github.com")]
    api_base: String,
}

#[derive(Deserialize, Debug)]
struct Repo {
    full_name: String,
    name: String,
    description: Option<String>,
    html_url: String,
    archived: Option<bool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = args
        .token
        .clone()
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .ok_or_else(|| anyhow!("GitHub token required: --token or GITHUB_TOKEN env"))?;

    let role = std::env::var("MANIFOLD_ROLE").unwrap_or_else(|_| "automation:github-sync".into());
    let manifold = ManifoldClient::from_env()?;

    let initial = match (&args.target, &args.user) {
        (Some(org), _) => format!("{}/orgs/{}/repos?per_page=100&type=all", args.api_base, org),
        (None, Some(user)) => format!("{}/users/{}/repos?per_page=100", args.api_base, user),
        _ => anyhow::bail!("one of --target <org> or --user <user> is required"),
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
            .header("User-Agent", "manifold-catalog-from-github/0.1")
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
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
        let repos: Vec<Repo> = resp.json().await.with_context(|| "parse repos")?;

        for repo in repos {
            if repo.archived.unwrap_or(false) {
                skipped += 1;
                continue;
            }
            let payload = json!({
                "name": repo.name,
                "description": repo.description.clone().unwrap_or_default(),
                "repo_url": repo.html_url,
            });
            let raw = json!({
                "full_name": repo.full_name,
                "html_url": repo.html_url,
            });
            let res = manifold
                .upsert_in_groundwork(
                    "/deployable/api",
                    "groundwork.deployable",
                    "github",
                    &repo.full_name,
                    &role,
                    payload,
                    raw,
                )
                .await
                .with_context(|| format!("upsert deployable for {}", repo.full_name))?;
            total += 1;
            if res.created {
                created += 1;
                println!("created  {} → {}", repo.full_name, &res.canonical_id[..8]);
            } else {
                updated += 1;
                println!("updated  {} → {}", repo.full_name, &res.canonical_id[..8]);
            }
        }

        match next_url {
            Some(u) => url = u,
            None => break,
        }
    }

    println!();
    println!(
        "{} repos processed: {} created, {} updated, {} skipped (archived)",
        total, created, updated, skipped
    );
    Ok(())
}
