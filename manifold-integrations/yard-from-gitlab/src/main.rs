//! yard-from-gitlab — import GitLab CI pipelines into Yard.
//!
//! Maps GitLab's CI surface onto Yard's test entities (mirrors
//! `yard-from-github` but for GitLab pipelines):
//!
//! | GitLab                  | Yard                                                              |
//! |-------------------------|-------------------------------------------------------------------|
//! | shared runners          | `TestInfrastructure(provider="gitlab_pipelines")`                 |
//! | project's CI surface    | `TestEnvironment(kind="ci")` — one per project                    |
//! | pipeline (as a suite)   | `TestSuite(runner="gitlab_pipelines")` — one per ref+source       |
//! | pipeline run            | `TestRun` — one per pipeline                                      |
//!
//! Idempotency:
//!
//! - infrastructure: `_gitlab_pipelines`
//! - environment:    `<path_with_namespace>:_ci`
//! - suite:          `<path_with_namespace>:_pipeline_def` (single suite per repo for v1)
//! - run:            `<path_with_namespace>:_pipeline_<id>`

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use manifold_adapter_common::ManifoldClient;
use serde::Deserialize;
use serde_json::json;

const RUNS_PER_PROJECT: u32 = 50;

#[derive(Parser, Debug)]
#[command(name = "yard-from-gitlab", version)]
struct Args {
    /// GitLab group full path.
    #[arg(long)]
    group: Option<String>,
    /// Single project full path, e.g. `my-org/my-project`.
    #[arg(long)]
    project: Option<String>,
    /// Personal access token. Defaults to GITLAB_TOKEN env var.
    #[arg(long)]
    token: Option<String>,
    /// GitLab base URL.
    #[arg(long, default_value = "https://gitlab.com")]
    base_url: String,
}

#[derive(Deserialize, Debug)]
struct Project {
    path_with_namespace: String,
    id: u64,
}

#[derive(Deserialize, Debug)]
struct Pipeline {
    id: u64,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    source: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = args
        .token
        .clone()
        .or_else(|| std::env::var("GITLAB_TOKEN").ok())
        .ok_or_else(|| anyhow!("GitLab token required: --token or GITLAB_TOKEN env"))?;
    let role =
        std::env::var("MANIFOLD_ROLE").unwrap_or_else(|_| "automation:gitlab-yard-sync".into());
    let manifold = ManifoldClient::from_env()?;
    let http = reqwest::Client::new();
    let base = args.base_url.trim_end_matches('/').to_string();

    // 1) Resolve projects
    let projects = list_projects(&http, &args, &base, &token).await?;
    if projects.is_empty() {
        println!("no projects found");
        return Ok(());
    }

    // 2) Infrastructure (once)
    let infra = manifold
        .upsert(
            "yard",
            "/test_infrastructure/api",
            "yard.test_infrastructure",
            "gitlab",
            "_gitlab_pipelines",
            &role,
            json!({
                "name": "GitLab Pipeline Runners",
                "provider": "gitlab_pipelines",
                "notes": "GitLab shared/specific runners. Auto-imported.",
            }),
            json!({ "source": "gitlab", "base_url": base }),
        )
        .await
        .with_context(|| "upsert TestInfrastructure")?;

    let mut total_runs = 0usize;
    let mut created_runs = 0usize;

    for project in &projects {
        let full = &project.path_with_namespace;
        println!("\n# {}", full);

        let deployable_id = manifold.find_canonical_id("gitlab", full).await?;
        match &deployable_id {
            Some(id) => println!("  ↪ linked to deployable {}", &id[..8]),
            None => println!("  ↪ no deployable (run catalog-from-gitlab first to link)"),
        }

        // 3) TestEnvironment per project
        let env = manifold
            .upsert(
                "yard",
                "/test_environment/api",
                "yard.test_environment",
                "gitlab",
                &format!("{full}:_ci"),
                &role,
                json!({
                    "name": format!("{full} / CI (GitLab Pipelines)"),
                    "kind": "ci",
                    "infrastructure_id": &infra.canonical_id,
                    "deployable_id": deployable_id.clone().unwrap_or_default(),
                    "notes": "Auto-imported from GitLab CI.",
                }),
                json!({ "project": full, "project_id": project.id }),
            )
            .await
            .with_context(|| format!("upsert TestEnvironment for {full}"))?;

        // 4) One TestSuite per project (representing .gitlab-ci.yml). Per-job
        //    suite mapping is a future refinement once we parse the YAML.
        let suite = manifold
            .upsert(
                "yard",
                "/test_suite/api",
                "yard.test_suite",
                "gitlab",
                &format!("{full}:_pipeline_def"),
                &role,
                json!({
                    "name": format!("{full} — .gitlab-ci.yml"),
                    "runner": "gitlab_pipelines",
                    "command": ".gitlab-ci.yml",
                    "deployable_id": deployable_id.clone().unwrap_or_default(),
                    "description": "Top-level pipeline definition. Per-job suites are a future refinement.",
                }),
                json!({ "project": full, "project_id": project.id }),
            )
            .await
            .with_context(|| format!("upsert TestSuite for {full}"))?;

        // 5) Recent pipelines → TestRuns
        let pipelines = list_pipelines(&http, &base, &token, project.id).await?;
        for p in pipelines {
            total_runs += 1;
            let started = p.created_at.clone().unwrap_or_default();
            let finished = p.updated_at.clone().unwrap_or_default();
            let status = gitlab_status_to_yard(&p.status);
            let upsert = manifold
                .upsert(
                    "yard",
                    "/test_run/api",
                    "yard.test_run",
                    "gitlab",
                    &format!("{full}:_pipeline_{}", p.id),
                    &role,
                    json!({
                        "test_environment_id": env.canonical_id,
                        "test_suite_id": suite.canonical_id,
                        "status": status,
                        "started_at": started,
                        "finished_at": finished,
                    }),
                    json!({
                        "project": full,
                        "pipeline_id": p.id,
                        "ref": p.git_ref,
                        "source": p.source,
                    }),
                )
                .await
                .with_context(|| format!("upsert TestRun for {full} pipeline {}", p.id))?;
            if upsert.created {
                created_runs += 1;
            }
        }
    }

    println!();
    println!(
        "{} pipelines processed, {} newly created",
        total_runs, created_runs
    );
    Ok(())
}

async fn list_projects(
    http: &reqwest::Client,
    args: &Args,
    base: &str,
    token: &str,
) -> Result<Vec<Project>> {
    if let Some(p) = &args.project {
        let url = format!("{base}/api/v4/projects/{}", encode_path_segment(p));
        let resp = gitlab_get(http, &url, token).await?;
        let v: Project = resp.json().await.with_context(|| "parse single project")?;
        return Ok(vec![v]);
    }
    let Some(g) = &args.group else {
        anyhow::bail!("one of --group <name> or --project <full/path> is required");
    };
    let url = format!(
        "{base}/api/v4/groups/{}/projects?per_page=100&include_subgroups=true",
        encode_path_segment(g)
    );
    let resp = gitlab_get(http, &url, token).await?;
    resp.json().await.with_context(|| "parse project list")
}

async fn list_pipelines(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    project_id: u64,
) -> Result<Vec<Pipeline>> {
    let url = format!("{base}/api/v4/projects/{project_id}/pipelines?per_page={RUNS_PER_PROJECT}");
    let resp = gitlab_get(http, &url, token).await?;
    resp.json().await.with_context(|| "parse pipelines")
}

async fn gitlab_get(http: &reqwest::Client, url: &str, token: &str) -> Result<reqwest::Response> {
    let resp = http
        .get(url)
        .header("PRIVATE-TOKEN", token)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GET {url} -> {status}: {body}");
    }
    Ok(resp)
}

/// Map GitLab pipeline status into yard's TestRun enum.
fn gitlab_status_to_yard(status: &Option<String>) -> String {
    match status.as_deref().unwrap_or("") {
        "success" => "passed".into(),
        "failed" => "failed".into(),
        "canceled" | "skipped" => "cancelled".into(),
        "running" => "running".into(),
        "pending" | "created" | "waiting_for_resource" | "preparing" | "scheduled" | "manual" => {
            "pending".into()
        }
        _ => "errored".into(),
    }
}

fn encode_path_segment(s: &str) -> String {
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
