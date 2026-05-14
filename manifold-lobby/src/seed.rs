//! One-shot seed of the five Meridian programs + program memberships on
//! first run. Idempotent: skips if any programs already exist.
//!
//! Programs come from the Lobby design doc §9:
//!   STREAMS     — customer-facing apps
//!   GBE         — dispatch + driver (general business engineering)
//!   DERMS       — analytics + ETL
//!   Kraken      — platform
//!   NextGen ERP — OMS + invoicing
//!
//! Membership is auto-derived by name heuristics over the deployables
//! groundwork already owns. Humans can re-tag via the MCP / API after seed.

use crate::state::AppState;
use anyhow::Result;
use meshql_core::{Envelope, Stash};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
struct ProgramSeed {
    name: &'static str,
    description: &'static str,
    leadership: &'static str,
    color: &'static str,
    /// Substrings (case-insensitive) that, if present in a deployable name,
    /// associate the deployable with this program.
    keywords: &'static [&'static str],
}

const PROGRAMS: &[ProgramSeed] = &[
    ProgramSeed {
        name: "STREAMS",
        description:
            "Customer-facing apps and integrations — portal, mobile, tracking, public API.",
        leadership: "CIDO, Customer Experience",
        color: "#2563eb",
        keywords: &[
            "Customer Portal",
            "Tracking",
            "Mobile",
            "Customer Gateway",
            "Customer API",
            "Email Service",
            "Notification",
        ],
    },
    ProgramSeed {
        name: "GBE",
        description: "General business engineering — dispatch, driver ops, fleet, warehouse.",
        leadership: "CIDO, Operations",
        color: "#16a34a",
        keywords: &[
            "Dispatch",
            "Driver",
            "Fleet",
            "Warehouse",
            "Route",
            "Carrier",
            "SMS",
        ],
    },
    ProgramSeed {
        name: "DERMS",
        description: "Data, ETL, analytics, audit — the read side of the freight business.",
        leadership: "CIDO, Data & Analytics",
        color: "#f59e0b",
        keywords: &["ETL", "Analytics", "Audit", "Geocoding", "Data "],
    },
    ProgramSeed {
        name: "Kraken",
        description:
            "Platform — auth, config, file storage, event bus, the bedrock everything sits on.",
        leadership: "CIDO, Platform",
        color: "#7c3aed",
        keywords: &["Auth", "Config", "File Storage", "Event Bus", "OMS Event"],
    },
    ProgramSeed {
        name: "NextGen ERP",
        description: "Order management and invoicing — the new ERP replacing Legacy CRM.",
        leadership: "CIDO, ERP",
        color: "#dc2626",
        keywords: &[
            "Order Management",
            "OMS REST",
            "Invoice",
            "Legacy CRM",
            "Customs",
        ],
    },
];

pub async fn seed_if_empty(state: &AppState) -> Result<SeedReport> {
    // Idempotency: skip if any programs exist.
    let existing = state.program.repo.list(&["*".into()]).await?;
    if !existing.is_empty() {
        return Ok(SeedReport::default());
    }
    let mut report = SeedReport::default();

    // 1. Create programs.
    let mut program_ids: Vec<(ProgramSeed, String)> = Vec::with_capacity(PROGRAMS.len());
    for p in PROGRAMS {
        let id = uuid::Uuid::new_v4().to_string();
        let mut payload = Stash::new();
        payload.insert("name".into(), Value::String(p.name.into()));
        payload.insert("description".into(), Value::String(p.description.into()));
        payload.insert("leadership".into(), Value::String(p.leadership.into()));
        payload.insert("color".into(), Value::String(p.color.into()));
        let env = Envelope::new(id.clone(), payload, vec!["*".into()]);
        state.program.repo.create(env, &[]).await?;
        program_ids.push((p.clone(), id));
        report.programs_created += 1;
    }

    // 2. Pull deployable names from groundwork via /graph; assign memberships
    //    by keyword match. Best-effort — if groundwork is unreachable at
    //    startup, programs are still seeded; memberships can be filled in
    //    later via the MCP / a re-derive call.
    let groundwork_url =
        std::env::var("GROUNDWORK_URL").unwrap_or_else(|_| "http://localhost:3050".into());
    let user_id = std::env::var("MANIFOLD_USER_ID").unwrap_or_else(|_| "lobby-system".into());
    let groups =
        std::env::var("MANIFOLD_USER_GROUPS").unwrap_or_else(|_| "automation:lobby-derive".into());

    if let Ok(deployables) = fetch_deployables(&groundwork_url, &user_id, &groups).await {
        for d in deployables {
            for (p, prog_id) in &program_ids {
                if p.keywords
                    .iter()
                    .any(|kw| d.name.to_lowercase().contains(&kw.to_lowercase()))
                {
                    let mem_id = uuid::Uuid::new_v4().to_string();
                    let mut payload = Stash::new();
                    payload.insert("program_id".into(), Value::String(prog_id.clone()));
                    payload.insert("subject_type".into(), Value::String("deployable".into()));
                    payload.insert("subject_id".into(), Value::String(d.id.clone()));
                    let env = Envelope::new(mem_id, payload, vec!["*".into()]);
                    state.program_membership.repo.create(env, &[]).await?;
                    report.memberships_created += 1;
                }
            }
        }
    }

    Ok(report)
}

#[derive(Default, Debug)]
pub struct SeedReport {
    pub programs_created: usize,
    pub memberships_created: usize,
}

struct DeployableLite {
    id: String,
    name: String,
}

async fn fetch_deployables(base: &str, user_id: &str, groups: &str) -> Result<Vec<DeployableLite>> {
    let http = reqwest::Client::new();
    let url = format!("{base}/deployable/graph");
    let resp = http
        .post(&url)
        .header("X-Manifold-User-Id", user_id)
        .header("X-Manifold-User-Groups", groups)
        .json(&serde_json::json!({ "query": "{ getAll { id name } }" }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("seed: GET {url} -> {}", resp.status());
    }
    let body: Value = resp.json().await?;
    let arr = body
        .pointer("/data/getAll")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("seed: malformed deployable response"))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing id"))?
            .to_string();
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(DeployableLite { id, name });
    }
    Ok(out)
}
