//! The derivation engine: poll → snapshot → run rules → reconcile.
//!
//! Reconciliation is the interesting part. Each run produces a set of
//! `(rule, subject_id)` advisories the rules want raised. The engine
//! cross-references this set against the existing Advisory envelopes in
//! Lobby's own meshlette:
//!
//! - **In both**: the advisory stays. Its `state` (raised / acknowledged /
//!   dismissed) is unchanged. The `explain` is refreshed to whatever the
//!   rule now says.
//! - **New (rule wants, not present)**: create a fresh Advisory in state
//!   `raised`, write a `raise` lifecycle entry with actor=system.
//! - **Gone (present, rule no longer wants)**: transition to `resolved`,
//!   write a `resolve` lifecycle entry. Respects a per-rule quiet window
//!   so transient flapping (rule fires, then doesn't, then does again on
//!   the next poll) doesn't auto-resolve/re-raise. v1 quiet window is 1
//!   hour, tunable per rule.
//! - **Dismissed that the rule wants again**: increments `re_raise_count`,
//!   transitions back to `raised`, writes a `re-raise` lifecycle entry.
//!
//! The engine runs as a tokio task; the HTTP handlers query the advisory
//! repo directly. The engine never touches state via HTTP — it has the
//! same `AppState` as the handlers.

use crate::rules::{all_rules, DerivedAdvisory};
use crate::snapshot::GraphSnapshot;
use crate::sources::SourceClients;
use crate::state::AppState;
use anyhow::Result;
use chrono::Utc;
use meshql_core::{Envelope, Stash};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// Default 1-hour quiet window before auto-resolving an advisory whose rule
/// stops firing. Per-rule overrides are read from
/// `LOBBY_QUIET_WINDOW_MINUTES_<RULE>` env vars.
const DEFAULT_QUIET_WINDOW_MIN: i64 = 60;

pub struct Engine {
    state: AppState,
    sources: SourceClients,
    poll_interval: Duration,
    /// Actor id Lobby writes into lifecycle entries when it acts on its own.
    system_id: String,
}

impl Engine {
    pub fn new(state: AppState, sources: SourceClients) -> Self {
        let interval_s = std::env::var("LOBBY_POLL_INTERVAL_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);
        let system_id =
            std::env::var("LOBBY_SYSTEM_ACTOR").unwrap_or_else(|_| "lobby-system".into());
        Self {
            state,
            sources,
            poll_interval: Duration::from_secs(interval_s),
            system_id,
        }
    }

    /// Run one derivation pass against a fresh snapshot.
    pub async fn tick(&self) -> Result<TickReport> {
        let snap = self.sources.fetch_snapshot().await?;
        let derived = run_all_rules(&snap);
        self.reconcile(derived).await
    }

    /// Spawn the polling loop and return immediately. The caller (main.rs)
    /// keeps the spawned handle alive for the life of the server.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.poll_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // First tick fires immediately; we want the first derivation to
            // run after a short delay so the source services have a chance
            // to come up.
            interval.tick().await;
            tokio::time::sleep(Duration::from_secs(2)).await;
            loop {
                interval.tick().await;
                match self.tick().await {
                    Ok(report) => {
                        tracing::info!(
                            target: "lobby::engine",
                            raised = report.raised,
                            resolved = report.resolved,
                            re_raised = report.re_raised,
                            "derivation tick"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "lobby::engine",
                            error = ?e,
                            "derivation tick failed"
                        );
                    }
                }
            }
        })
    }

    async fn reconcile(&self, derived: Vec<DerivedAdvisory>) -> Result<TickReport> {
        // Index the rule's output by (rule, subject_id) — that's the natural
        // identity of an advisory.
        let mut want: HashMap<(String, String), DerivedAdvisory> = HashMap::new();
        for d in derived {
            want.insert((d.rule.to_string(), d.subject_id.clone()), d);
        }

        // Read all existing advisories.
        let existing_envs = self.state.advisory.repo.list(&["*".into()]).await?;

        let mut report = TickReport::default();
        let mut existing_keys: HashSet<(String, String)> = HashSet::new();

        for env in &existing_envs {
            if env.deleted {
                continue;
            }
            let rule = env
                .payload
                .get("rule")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let subject_id = env
                .payload
                .get("subject_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let key = (rule.clone(), subject_id.clone());
            existing_keys.insert(key.clone());

            let state = env
                .payload
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("raised");

            if let Some(d) = want.remove(&key) {
                // Already present — refresh `explain` but otherwise leave
                // state alone. If it was dismissed/resolved, re-raise it.
                if state == "dismissed" || state == "resolved" {
                    self.re_raise(env, &d).await?;
                    report.re_raised += 1;
                } else {
                    self.refresh(env, &d).await?;
                }
            } else if state == "raised" || state == "acknowledged" {
                // Rule stopped firing — auto-resolve if quiet window has
                // elapsed since the last lifecycle entry.
                let quiet_min = quiet_window_for(&rule);
                if let Some(env) = self.should_auto_resolve(env, quiet_min).await? {
                    self.resolve(&env).await?;
                    report.resolved += 1;
                }
            }
        }

        // Remaining `want` entries are new advisories.
        for ((_rule, _sid), d) in want {
            self.raise_new(&d).await?;
            report.raised += 1;
        }

        Ok(report)
    }

    async fn raise_new(&self, d: &DerivedAdvisory) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let mut payload = Stash::new();
        payload.insert("kind".into(), Value::String(d.kind.into()));
        payload.insert("subject_type".into(), Value::String(d.subject_type.into()));
        payload.insert("subject_id".into(), Value::String(d.subject_id.clone()));
        payload.insert("subject_name".into(), Value::String(d.subject_name.clone()));
        payload.insert("severity".into(), Value::String(d.severity.as_str().into()));
        payload.insert("state".into(), Value::String("raised".into()));
        payload.insert("rule".into(), Value::String(d.rule.into()));
        payload.insert("explain".into(), Value::String(d.explain.clone()));
        payload.insert("caused_by".into(), Value::String(d.caused_by.join(",")));
        payload.insert("raised_at".into(), Value::String(now.clone()));
        payload.insert("re_raise_count".into(), Value::String("0".into()));
        payload.insert("last_action".into(), Value::String(format!("raise@{now}")));
        let env = Envelope::new(id.clone(), payload, vec!["*".into()]);
        self.state.advisory.repo.create(env, &[]).await?;
        self.write_lifecycle(&id, "raise", None, None).await?;
        Ok(())
    }

    async fn refresh(&self, env: &Envelope, d: &DerivedAdvisory) -> Result<()> {
        let mut payload = env.payload.clone();
        payload.insert("explain".into(), Value::String(d.explain.clone()));
        payload.insert("caused_by".into(), Value::String(d.caused_by.join(",")));
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        Ok(())
    }

    async fn re_raise(&self, env: &Envelope, d: &DerivedAdvisory) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut payload = env.payload.clone();
        payload.insert("state".into(), Value::String("raised".into()));
        payload.insert("explain".into(), Value::String(d.explain.clone()));
        payload.insert("caused_by".into(), Value::String(d.caused_by.join(",")));
        payload.insert("raised_at".into(), Value::String(now.clone()));
        payload.insert("dismissed_at".into(), Value::Null);
        payload.insert("resolved_at".into(), Value::Null);
        let prev = payload
            .get("re_raise_count")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        payload.insert(
            "re_raise_count".into(),
            Value::String((prev + 1).to_string()),
        );
        payload.insert(
            "last_action".into(),
            Value::String(format!("re-raise@{now}")),
        );
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        self.write_lifecycle(&env.id, "re-raise", None, None)
            .await?;
        Ok(())
    }

    async fn resolve(&self, env: &Envelope) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut payload = env.payload.clone();
        payload.insert("state".into(), Value::String("resolved".into()));
        payload.insert("resolved_at".into(), Value::String(now.clone()));
        payload.insert(
            "last_action".into(),
            Value::String(format!("resolve@{now}")),
        );
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        self.write_lifecycle(&env.id, "resolve", None, None).await?;
        Ok(())
    }

    async fn should_auto_resolve(
        &self,
        env: &Envelope,
        quiet_min: i64,
    ) -> Result<Option<Envelope>> {
        // last action timestamp — use last_action's `@<rfc3339>` suffix, or
        // fall back to raised_at
        let payload = &env.payload;
        let last_action_at = payload
            .get("last_action")
            .and_then(|v| v.as_str())
            .and_then(|s| s.rsplit('@').next())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        let raised_at = payload
            .get("raised_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        let reference = last_action_at.or(raised_at);
        match reference {
            None => Ok(Some(env.clone())),
            Some(t) => {
                let now = Utc::now();
                let elapsed = now.signed_duration_since(t.with_timezone(&Utc));
                if elapsed >= chrono::Duration::minutes(quiet_min) {
                    Ok(Some(env.clone()))
                } else {
                    Ok(None)
                }
            }
        }
    }

    async fn write_lifecycle(
        &self,
        advisory_id: &str,
        action: &str,
        reason: Option<&str>,
        note: Option<&str>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let mut payload = Stash::new();
        payload.insert("advisory_id".into(), Value::String(advisory_id.into()));
        payload.insert("at".into(), Value::String(now));
        payload.insert("actor_type".into(), Value::String("system".into()));
        payload.insert("actor_id".into(), Value::String(self.system_id.clone()));
        payload.insert("action".into(), Value::String(action.into()));
        if let Some(r) = reason {
            payload.insert("reason".into(), Value::String(r.into()));
        }
        if let Some(n) = note {
            payload.insert("note".into(), Value::String(n.into()));
        }
        let env = Envelope::new(id, payload, vec!["*".into()]);
        self.state.lifecycle_entry.repo.create(env, &[]).await?;
        Ok(())
    }
}

fn quiet_window_for(rule: &str) -> i64 {
    let key = format!(
        "LOBBY_QUIET_WINDOW_MINUTES_{}",
        rule.replace(['@', '-'], "_").to_uppercase()
    );
    std::env::var(&key)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(DEFAULT_QUIET_WINDOW_MIN)
}

fn run_all_rules(snap: &GraphSnapshot) -> Vec<DerivedAdvisory> {
    let mut out: Vec<DerivedAdvisory> = Vec::new();
    for rule_fn in all_rules() {
        out.extend(rule_fn(snap));
    }
    out
}

#[derive(Default, Debug)]
pub struct TickReport {
    pub raised: usize,
    pub resolved: usize,
    pub re_raised: usize,
}

/// Helpers used by the HTTP handlers when humans act on advisories.
pub struct UserAction<'a> {
    pub state: &'a AppState,
    pub advisory_id: &'a str,
    pub user_id: &'a str,
}

impl<'a> UserAction<'a> {
    async fn write_lifecycle(
        &self,
        action: &str,
        reason: Option<&str>,
        note: Option<&str>,
    ) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let mut payload = Stash::new();
        payload.insert("advisory_id".into(), Value::String(self.advisory_id.into()));
        payload.insert("at".into(), Value::String(now));
        payload.insert("actor_type".into(), Value::String("user".into()));
        payload.insert("actor_id".into(), Value::String(self.user_id.into()));
        payload.insert("action".into(), Value::String(action.into()));
        if let Some(r) = reason {
            payload.insert("reason".into(), Value::String(r.into()));
        }
        if let Some(n) = note {
            payload.insert("note".into(), Value::String(n.into()));
        }
        let env = Envelope::new(id, payload, vec!["*".into()]);
        self.state.lifecycle_entry.repo.create(env, &[]).await?;
        Ok(())
    }

    async fn load_env(&self) -> Result<Option<Envelope>> {
        Ok(self
            .state
            .advisory
            .repo
            .read(self.advisory_id, &[], None)
            .await?)
    }

    pub async fn acknowledge(&self) -> Result<()> {
        let Some(env) = self.load_env().await? else {
            anyhow::bail!("advisory not found: {}", self.advisory_id);
        };
        let now = Utc::now().to_rfc3339();
        let mut payload = env.payload.clone();
        payload.insert("state".into(), Value::String("acknowledged".into()));
        payload.insert("acknowledged_at".into(), Value::String(now.clone()));
        payload.insert(
            "last_action".into(),
            Value::String(format!("acknowledge@{now}")),
        );
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        self.write_lifecycle("acknowledge", None, None).await?;
        Ok(())
    }

    pub async fn dismiss(&self, reason: &str, note: Option<&str>) -> Result<()> {
        let Some(env) = self.load_env().await? else {
            anyhow::bail!("advisory not found: {}", self.advisory_id);
        };
        let now = Utc::now().to_rfc3339();
        let mut payload = env.payload.clone();
        payload.insert("state".into(), Value::String("dismissed".into()));
        payload.insert("dismissed_at".into(), Value::String(now.clone()));
        payload.insert("dismiss_reason".into(), Value::String(reason.into()));
        if let Some(n) = note {
            payload.insert("dismiss_note".into(), Value::String(n.into()));
        }
        payload.insert(
            "last_action".into(),
            Value::String(format!("dismiss@{now}")),
        );
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        self.write_lifecycle("dismiss", Some(reason), note).await?;
        Ok(())
    }

    pub async fn escalate(&self, to: &str, note: Option<&str>) -> Result<()> {
        let Some(env) = self.load_env().await? else {
            anyhow::bail!("advisory not found: {}", self.advisory_id);
        };
        let now = Utc::now().to_rfc3339();
        let mut payload = env.payload.clone();
        payload.insert("escalated_to".into(), Value::String(to.into()));
        payload.insert(
            "last_action".into(),
            Value::String(format!("escalate@{now}")),
        );
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        self.write_lifecycle("escalate", None, Some(to)).await?;
        Ok(())
    }

    pub async fn assign(&self, assignee: &str) -> Result<()> {
        let Some(env) = self.load_env().await? else {
            anyhow::bail!("advisory not found: {}", self.advisory_id);
        };
        let now = Utc::now().to_rfc3339();
        let mut payload = env.payload.clone();
        payload.insert("assignee".into(), Value::String(assignee.into()));
        payload.insert("last_action".into(), Value::String(format!("assign@{now}")));
        let new_env = Envelope::new(env.id.clone(), payload, env.authorized_tokens.clone());
        self.state.advisory.repo.create(new_env, &[]).await?;
        self.write_lifecycle("assign", None, Some(assignee)).await?;
        Ok(())
    }

    pub async fn add_comment(&self, body: &str) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let mut payload = Stash::new();
        payload.insert("advisory_id".into(), Value::String(self.advisory_id.into()));
        payload.insert("author".into(), Value::String(self.user_id.into()));
        payload.insert("body".into(), Value::String(body.into()));
        payload.insert("at".into(), Value::String(now));
        let env = Envelope::new(id.clone(), payload, vec!["*".into()]);
        self.state.comment.repo.create(env, &[]).await?;
        self.write_lifecycle("comment", None, Some(body)).await?;
        Ok(id)
    }
}

// Re-export for test convenience.
pub use crate::rules::DerivedAdvisory as _DerivedAdvisory;
