//! Aggregate TestRun history into per-environment statistics.
//!
//! Cityhall's planner asks Yard "last time we tested X, how long did it take?"
//! This module takes the raw TestRun envelopes and produces averages.

use meshql_core::Repository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvHistory {
    pub test_environment_id: String,
    pub run_count: usize,
    pub passed: usize,
    pub failed: usize,
    pub average_duration_minutes: f64,
    pub average_cost: f64,
    pub pass_rate: f64,
}

pub async fn history_for_env(
    test_run_repo: &Arc<dyn Repository>,
    env_id: &str,
) -> anyhow::Result<EnvHistory> {
    let runs = test_run_repo.list(&[]).await?;
    let mut count: usize = 0;
    let mut passed: usize = 0;
    let mut failed: usize = 0;
    let mut total_minutes: f64 = 0.0;
    let mut total_cost: f64 = 0.0;
    let mut minutes_samples: usize = 0;
    let mut cost_samples: usize = 0;

    for env in &runs {
        let tid = env
            .payload
            .get("test_environment_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if tid != env_id {
            continue;
        }
        count += 1;
        match env
            .payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
        {
            "passed" => passed += 1,
            "failed" | "errored" => failed += 1,
            _ => {}
        }
        if let Some(m) = env
            .payload
            .get("duration_minutes")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
        {
            total_minutes += m;
            minutes_samples += 1;
        }
        if let Some(c) = env
            .payload
            .get("cost_actual")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
        {
            total_cost += c;
            cost_samples += 1;
        }
    }

    let avg_minutes = if minutes_samples == 0 {
        0.0
    } else {
        total_minutes / minutes_samples as f64
    };
    let avg_cost = if cost_samples == 0 {
        0.0
    } else {
        total_cost / cost_samples as f64
    };
    let pass_rate = if count == 0 {
        0.0
    } else {
        passed as f64 / count as f64
    };

    Ok(EnvHistory {
        test_environment_id: env_id.to_string(),
        run_count: count,
        passed,
        failed,
        average_duration_minutes: avg_minutes,
        average_cost: avg_cost,
        pass_rate,
    })
}

/// Availability check: is this env within its concurrency / contractual
/// limits given the currently-running TestRuns?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Availability {
    pub test_environment_id: String,
    pub running_count: usize,
    pub concurrency_limit: Option<u32>,
    pub contractual_limit: Option<u32>,
    pub available: bool,
    pub reason: Option<String>,
}

pub async fn availability_for_env(
    test_env_repo: &Arc<dyn Repository>,
    test_run_repo: &Arc<dyn Repository>,
    env_id: &str,
) -> anyhow::Result<Availability> {
    let env = test_env_repo.read(env_id, &[], None).await?;
    let Some(env) = env else {
        return Ok(Availability {
            test_environment_id: env_id.to_string(),
            running_count: 0,
            concurrency_limit: None,
            contractual_limit: None,
            available: false,
            reason: Some("test_environment not found".into()),
        });
    };
    let concurrency_limit = env
        .payload
        .get("concurrency_limit")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok());
    let contractual_limit = env
        .payload
        .get("contractual_limit")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok());

    let runs = test_run_repo.list(&[]).await?;
    let running_count = runs
        .iter()
        .filter(|r| {
            r.payload
                .get("test_environment_id")
                .and_then(|v| v.as_str())
                == Some(env_id)
                && r.payload
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "running" || s == "pending")
                    .unwrap_or(false)
        })
        .count();

    let mut available = true;
    let mut reason: Option<String> = None;
    if let Some(c) = concurrency_limit {
        if running_count as u32 >= c {
            available = false;
            reason = Some(format!("concurrency cap reached: {running_count}/{c}"));
        }
    }
    if let Some(c) = contractual_limit {
        if running_count as u32 >= c {
            available = false;
            reason = Some(format!("contractual cap reached: {running_count}/{c}"));
        }
    }

    Ok(Availability {
        test_environment_id: env_id.to_string(),
        running_count,
        concurrency_limit,
        contractual_limit,
        available,
        reason,
    })
}
