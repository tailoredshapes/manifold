//! union-from-okta — import users + groups + memberships from an Okta org
//! into Union's people / teams / team_members entities.
//!
//! Mapping:
//!
//! | Okta                      | Union                                           |
//! |---------------------------|-------------------------------------------------|
//! | active user               | `Person(name, contact=email, role=title)`       |
//! | group                     | `Team(name, kind="okta-group", description)`    |
//! | (user, group) membership  | `TeamMember(person_id, team_id, role)`          |
//!
//! Idempotency by Okta natural id:
//!
//! - person:        `okta_user_<id>`
//! - team:          `okta_group_<id>`
//! - team_member:   `okta_membership_<userId>_<groupId>`

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use manifold_adapter_common::{parse_next_link, ManifoldClient};
use serde::Deserialize;
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "union-from-okta", version)]
struct Args {
    /// Okta org domain, e.g. `dev-123456.okta.com`.
    #[arg(long)]
    okta_domain: String,
    /// API token. Defaults to OKTA_TOKEN env var.
    #[arg(long)]
    token: Option<String>,
    /// Override the API base URL (useful for stub-based testing).
    /// When set, the okta-domain arg is ignored.
    #[arg(long)]
    api_base: Option<String>,
}

#[derive(Deserialize, Debug)]
struct User {
    id: String,
    status: Option<String>,
    profile: UserProfile,
}

#[derive(Deserialize, Debug)]
struct UserProfile {
    #[serde(rename = "firstName")]
    first_name: Option<String>,
    #[serde(rename = "lastName")]
    last_name: Option<String>,
    email: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Group {
    id: String,
    profile: GroupProfile,
}

#[derive(Deserialize, Debug)]
struct GroupProfile {
    name: String,
    description: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = args
        .token
        .clone()
        .or_else(|| std::env::var("OKTA_TOKEN").ok())
        .ok_or_else(|| anyhow!("Okta token required: --token or OKTA_TOKEN env"))?;
    let role = std::env::var("MANIFOLD_ROLE").unwrap_or_else(|_| "automation:okta-sync".into());
    let manifold = ManifoldClient::from_env()?;
    let http = reqwest::Client::new();

    let api_base = args
        .api_base
        .clone()
        .unwrap_or_else(|| format!("https://{}/api/v1", args.okta_domain.trim_end_matches('/')));

    // 1) Users → Person
    let users = list_users(&http, &api_base, &token).await?;
    println!("# {} active users", users.len());
    let mut person_id_by_okta: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for u in &users {
        let display = format!(
            "{} {}",
            u.profile.first_name.clone().unwrap_or_default(),
            u.profile.last_name.clone().unwrap_or_default()
        )
        .trim()
        .to_string();
        let display = if display.is_empty() {
            u.profile.email.clone().unwrap_or_else(|| u.id.clone())
        } else {
            display
        };
        let payload = json!({
            "name": display,
            "contact": u.profile.email.clone().unwrap_or_default(),
            "role": u.profile.title.clone().unwrap_or_default(),
        });
        let raw = json!({ "okta_id": u.id, "status": u.status });
        let res = manifold
            .upsert(
                "union",
                "/person/api",
                "union.person",
                "okta",
                &format!("okta_user_{}", u.id),
                &role,
                payload,
                raw,
            )
            .await
            .with_context(|| format!("upsert Person for okta user {}", u.id))?;
        person_id_by_okta.insert(u.id.clone(), res.canonical_id.clone());
        if res.created {
            println!("  created Person {} → {}", display, &res.canonical_id[..8]);
        } else {
            println!("  updated Person {} → {}", display, &res.canonical_id[..8]);
        }
    }

    // 2) Groups → Team, with members → TeamMember
    let groups = list_groups(&http, &api_base, &token).await?;
    println!("\n# {} groups", groups.len());
    let mut created_memberships = 0usize;
    let mut updated_memberships = 0usize;
    for g in &groups {
        // Okta groups don't carry a semantic `kind` matching union's enum
        // (product / platform / security / …). Default to "domain" as a
        // safe catchall; humans can re-categorize via the UI / MCP after
        // import. The original Okta source is preserved in the provenance
        // record's `raw` field.
        let payload = json!({
            "name": g.profile.name,
            "kind": "domain",
            "description": g.profile.description.clone().unwrap_or_default(),
        });
        let raw = json!({ "okta_id": g.id });
        let team = manifold
            .upsert(
                "union",
                "/team/api",
                "union.team",
                "okta",
                &format!("okta_group_{}", g.id),
                &role,
                payload,
                raw,
            )
            .await
            .with_context(|| format!("upsert Team for okta group {}", g.id))?;
        if team.created {
            println!(
                "  created Team {} → {}",
                g.profile.name,
                &team.canonical_id[..8]
            );
        } else {
            println!(
                "  updated Team {} → {}",
                g.profile.name,
                &team.canonical_id[..8]
            );
        }

        // memberships
        let members = list_group_members(&http, &api_base, &token, &g.id).await?;
        for m in &members {
            let person_id = match person_id_by_okta.get(&m.id) {
                Some(p) => p.clone(),
                None => {
                    // user not in our active set (deactivated, suspended, etc.) — skip
                    continue;
                }
            };
            let tm = manifold
                .upsert(
                    "union",
                    "/team_member/api",
                    "union.team_member",
                    "okta",
                    &format!("okta_membership_{}_{}", m.id, g.id),
                    &role,
                    json!({
                        "person_id": person_id,
                        "team_id": team.canonical_id,
                        "role": "member",
                    }),
                    json!({ "okta_user_id": m.id, "okta_group_id": g.id }),
                )
                .await
                .with_context(|| format!("upsert TeamMember okta user {} group {}", m.id, g.id))?;
            if tm.created {
                created_memberships += 1;
            } else {
                updated_memberships += 1;
            }
        }
    }

    println!();
    println!(
        "{} persons, {} teams, {} memberships ({} created, {} updated)",
        users.len(),
        groups.len(),
        created_memberships + updated_memberships,
        created_memberships,
        updated_memberships
    );
    Ok(())
}

async fn list_users(http: &reqwest::Client, api_base: &str, token: &str) -> Result<Vec<User>> {
    paginate::<User>(
        http,
        &format!("{api_base}/users?limit=200&filter=status%20eq%20%22ACTIVE%22"),
        token,
    )
    .await
}

async fn list_groups(http: &reqwest::Client, api_base: &str, token: &str) -> Result<Vec<Group>> {
    paginate::<Group>(http, &format!("{api_base}/groups?limit=200"), token).await
}

async fn list_group_members(
    http: &reqwest::Client,
    api_base: &str,
    token: &str,
    group_id: &str,
) -> Result<Vec<User>> {
    paginate::<User>(
        http,
        &format!("{api_base}/groups/{group_id}/users?limit=200"),
        token,
    )
    .await
}

async fn paginate<T: for<'de> Deserialize<'de>>(
    http: &reqwest::Client,
    initial_url: &str,
    token: &str,
) -> Result<Vec<T>> {
    let mut url = initial_url.to_string();
    let mut out: Vec<T> = Vec::new();
    loop {
        let resp = http
            .get(&url)
            .header("Authorization", format!("SSWS {token}"))
            .header("Accept", "application/json")
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
        let mut page: Vec<T> = resp.json().await.with_context(|| format!("parse {url}"))?;
        out.append(&mut page);
        match next_url {
            Some(u) => url = u,
            None => break,
        }
    }
    Ok(out)
}
