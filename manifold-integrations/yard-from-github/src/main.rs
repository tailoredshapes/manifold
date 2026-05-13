//! yard-from-github — import GitHub Actions workflow runs into Yard.
//!
//! Maps the GitHub Actions surface onto Yard's test entities:
//!
//! | GitHub                  | Yard                                                              |
//! |-------------------------|-------------------------------------------------------------------|
//! | hosted runners          | `TestInfrastructure(provider="github_actions")` — one per source  |
//! | repo's CI surface       | `TestEnvironment(kind="ci")` — one per repo                       |
//! | workflow                | `TestSuite(runner="github_actions")` — one per workflow           |
//! | workflow run            | `TestRun` — one per run                                           |
//!
//! Idempotency is by `(external_system="github", external_id=<natural-key>)`:
//!
//! - infrastructure: `_github_actions`
//! - environment:    `<owner>/<repo>:_ci`
//! - suite:          `<owner>/<repo>:_workflow_<workflow_id>`
//! - run:            `<owner>/<repo>:_run_<run_id>`
//!
//! Deployable linkage: the adapter looks up the canonical deployable id for
//! `(github, <owner>/<repo>)` in `manifold-ingest` — if `catalog-from-github`
//! has been run first, the test_environment and test_suite get
//! `deployable_id` populated. If not, those fields are left null.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use manifold_adapter_common::ManifoldClient;
use serde::Deserialize;
use serde_json::json;

const RUNS_PER_REPO: u32 = 50;

#[derive(Parser, Debug)]
#[command(name = "yard-from-github", version)]
struct Args {
    /// GitHub org or user to crawl. (Single repo: `--repo owner/name`.)
    #[arg(long)]
    target: Option<String>,
    /// Single repo override, e.g. `octocat/Hello-World`.
    #[arg(long)]
    repo: Option<String>,
    /// Personal access token. Defaults to GITHUB_TOKEN env var.
    #[arg(long)]
    token: Option<String>,
    /// GitHub API base URL (override for enterprise / stub).
    #[arg(long, default_value = "https://api.github.com")]
    api_base: String,
}

#[derive(Deserialize, Debug)]
struct Repo {
    full_name: String,
}

#[derive(Deserialize, Debug)]
struct WorkflowList {
    workflows: Vec<Workflow>,
}

#[derive(Deserialize, Debug)]
struct Workflow {
    id: u64,
    name: String,
    path: String,
}

#[derive(Deserialize, Debug)]
struct WorkflowRunList {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize, Debug)]
struct WorkflowRun {
    id: u64,
    workflow_id: u64,
    status: Option<String>,
    conclusion: Option<String>,
    run_started_at: Option<String>,
    updated_at: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = args
        .token
        .clone()
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .ok_or_else(|| anyhow!("GitHub token required: --token or GITHUB_TOKEN env"))?;
    let role =
        std::env::var("MANIFOLD_ROLE").unwrap_or_else(|_| "automation:github-yard-sync".into());
    let manifold = ManifoldClient::from_env()?;
    let http = reqwest::Client::new();

    // 1) Resolve repos to crawl
    let repos = list_repos(&http, &args, &token).await?;
    if repos.is_empty() {
        println!("no repos found");
        return Ok(());
    }

    // 2) Idempotent infrastructure (once)
    let infra_id = manifold
        .upsert(
            "yard",
            "/test_infrastructure/api",
            "yard.test_infrastructure",
            "github",
            "_github_actions",
            &role,
            json!({
                "name": "GitHub Actions Runners",
                "provider": "github_actions",
                "notes": "GitHub-hosted runners (Linux/Windows/macOS). Auto-imported.",
            }),
            json!({ "source": "github" }),
        )
        .await
        .with_context(|| "upsert TestInfrastructure")?;

    let mut total_runs = 0usize;
    let mut created_runs = 0usize;

    for repo in &repos {
        let full = &repo.full_name;
        println!("\n# {}", full);

        // 3) Look up the canonical deployable (optional — null if not yet imported)
        let deployable_id = manifold.find_canonical_id("github", full).await?;
        match &deployable_id {
            Some(id) => println!("  ↪ linked to deployable {}", &id[..8]),
            None => {
                println!("  ↪ no deployable for this repo (run catalog-from-github first to link)")
            }
        }

        // 4) TestEnvironment per repo
        let env_payload = json!({
            "name": format!("{full} / CI (GitHub Actions)"),
            "kind": "ci",
            "infrastructure_id": &infra_id.canonical_id,
            "deployable_id": deployable_id.clone().unwrap_or_default(),
            "notes": "Auto-imported from GitHub Actions.",
        });
        let env = manifold
            .upsert(
                "yard",
                "/test_environment/api",
                "yard.test_environment",
                "github",
                &format!("{full}:_ci"),
                &role,
                env_payload,
                json!({ "repo": full }),
            )
            .await
            .with_context(|| format!("upsert TestEnvironment for {full}"))?;

        // 5) Workflows → TestSuites
        let workflows = list_workflows(&http, &args.api_base, &token, full).await?;
        let mut suite_by_workflow_id = std::collections::HashMap::new();
        for w in &workflows {
            let suite_payload = json!({
                "name": w.name,
                "runner": "github_actions",
                "command": w.path,
                "deployable_id": deployable_id.clone().unwrap_or_default(),
                "description": format!("GitHub workflow at {}", w.path),
            });
            let suite = manifold
                .upsert(
                    "yard",
                    "/test_suite/api",
                    "yard.test_suite",
                    "github",
                    &format!("{full}:_workflow_{}", w.id),
                    &role,
                    suite_payload,
                    json!({ "repo": full, "workflow_id": w.id, "path": w.path }),
                )
                .await
                .with_context(|| format!("upsert TestSuite for {full} workflow {}", w.id))?;
            suite_by_workflow_id.insert(w.id, suite.canonical_id);
        }

        // 6) Recent runs → TestRuns
        let runs = list_workflow_runs(&http, &args.api_base, &token, full).await?;
        for r in runs {
            total_runs += 1;
            let suite_id = suite_by_workflow_id
                .get(&r.workflow_id)
                .cloned()
                .unwrap_or_default();
            let started = r.run_started_at.clone().unwrap_or_default();
            let finished = r.updated_at.clone().unwrap_or_default();
            let status = github_status_to_yard(&r.conclusion, &r.status);
            let duration_minutes = compute_duration_minutes(&started, &finished);
            let run_payload = json!({
                "test_environment_id": env.canonical_id,
                "test_suite_id": suite_id,
                "status": status,
                "started_at": started,
                "finished_at": finished,
                "duration_minutes": duration_minutes,
            });
            let raw = json!({
                "repo": full,
                "run_id": r.id,
                "workflow_id": r.workflow_id,
                "github_status": r.status,
                "github_conclusion": r.conclusion,
            });
            let upsert = manifold
                .upsert(
                    "yard",
                    "/test_run/api",
                    "yard.test_run",
                    "github",
                    &format!("{full}:_run_{}", r.id),
                    &role,
                    run_payload,
                    raw,
                )
                .await
                .with_context(|| format!("upsert TestRun for {full} run {}", r.id))?;
            if upsert.created {
                created_runs += 1;
            }
        }
    }

    println!();
    println!(
        "{} runs processed, {} newly created",
        total_runs, created_runs
    );
    Ok(())
}

async fn list_repos(http: &reqwest::Client, args: &Args, token: &str) -> Result<Vec<Repo>> {
    if let Some(repo) = &args.repo {
        let url = format!("{}/repos/{}", args.api_base, repo);
        let resp = github_get(http, &url, token).await?;
        let v: Repo = resp.json().await.with_context(|| "parse single repo")?;
        return Ok(vec![v]);
    }
    let Some(target) = &args.target else {
        anyhow::bail!("one of --target <org> or --repo <owner/name> is required");
    };
    let url = format!(
        "{}/orgs/{}/repos?per_page=100&type=all",
        args.api_base, target
    );
    let resp = github_get(http, &url, token).await?;
    resp.json().await.with_context(|| "parse repo list")
}

async fn list_workflows(
    http: &reqwest::Client,
    api_base: &str,
    token: &str,
    repo: &str,
) -> Result<Vec<Workflow>> {
    let url = format!("{api_base}/repos/{repo}/actions/workflows?per_page=100");
    let resp = github_get(http, &url, token).await?;
    let body: WorkflowList = resp.json().await.with_context(|| "parse workflows")?;
    Ok(body.workflows)
}

async fn list_workflow_runs(
    http: &reqwest::Client,
    api_base: &str,
    token: &str,
    repo: &str,
) -> Result<Vec<WorkflowRun>> {
    let url = format!("{api_base}/repos/{repo}/actions/runs?per_page={RUNS_PER_REPO}");
    let resp = github_get(http, &url, token).await?;
    let body: WorkflowRunList = resp.json().await.with_context(|| "parse workflow runs")?;
    Ok(body.workflow_runs)
}

async fn github_get(http: &reqwest::Client, url: &str, token: &str) -> Result<reqwest::Response> {
    let resp = http
        .get(url)
        .header("User-Agent", "manifold-yard-from-github/0.1")
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
    Ok(resp)
}

/// Minutes between two ISO-8601 timestamps, as a numeric string for yard.
/// Returns empty string on parse failure (the field is optional).
fn compute_duration_minutes(started: &str, finished: &str) -> String {
    fn parse_iso8601_to_epoch(s: &str) -> Option<i64> {
        // GitHub returns e.g. "2026-05-13T15:23:04Z". Minimal parser to avoid
        // pulling in chrono just for this — split on T, then on Z and ':'.
        let (date, time) = s.split_once('T')?;
        let parts: Vec<&str> = date.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let y: i64 = parts[0].parse().ok()?;
        let mo: i64 = parts[1].parse().ok()?;
        let d: i64 = parts[2].parse().ok()?;
        let time = time.trim_end_matches('Z');
        let tparts: Vec<&str> = time.split(':').collect();
        if tparts.len() < 3 {
            return None;
        }
        let h: i64 = tparts[0].parse().ok()?;
        let mi: i64 = tparts[1].parse().ok()?;
        let sec: i64 = tparts[2].split('.').next()?.parse().ok()?;
        // Cheap days-from-epoch via a calendar table — accurate enough for
        // duration math (subtraction cancels the calendar nonsense).
        let days = days_from_civil(y, mo as u32, d as u32);
        Some(days * 86400 + h * 3600 + mi * 60 + sec)
    }
    let Some(a) = parse_iso8601_to_epoch(started) else {
        return String::new();
    };
    let Some(b) = parse_iso8601_to_epoch(finished) else {
        return String::new();
    };
    let minutes = (b - a) / 60;
    if minutes < 0 {
        String::new()
    } else {
        minutes.to_string()
    }
}

/// Map GitHub Actions' (status, conclusion) into yard's TestRun status enum
/// (`pending`/`running`/`passed`/`failed`/`cancelled`/`errored`). When the
/// run is still in flight, `conclusion` is null and we use `status` instead.
fn github_status_to_yard(conclusion: &Option<String>, status: &Option<String>) -> String {
    if let Some(c) = conclusion.as_deref() {
        return match c {
            "success" => "passed",
            "failure" => "failed",
            "cancelled" | "skipped" => "cancelled",
            "timed_out" | "action_required" | "neutral" | "stale" | "startup_failure" => "errored",
            _ => "errored",
        }
        .into();
    }
    match status.as_deref().unwrap_or("") {
        "queued" | "waiting" | "requested" | "pending" => "pending".into(),
        "in_progress" => "running".into(),
        _ => "pending".into(),
    }
}

/// Howard Hinnant's date algorithm — days since 1970-01-01.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let m = m as u64;
    let d = d as u64;
    let doy = (153 * if m > 2 { m - 3 } else { m + 9 } + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}
