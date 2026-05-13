# Manifold integrations

Adapters that read external systems-of-record (GitHub repos, GitLab projects,
Okta users, Docker Compose files, …) and idempotently populate primary
Manifold meshlettes (groundwork, union, …) while recording provenance in
`manifold-ingest`.

Each adapter is an independently deployable Rust binary. The
[`common/`](./common) crate provides the shared `ManifoldClient` (upsert +
record-ingestion + idempotency-by-lookup-in-ingest) so each adapter is
typically ~120 lines of source-system-specific code on top.

## Adapters shipped

| Adapter | Source | Target | Status |
|---|---|---|---|
| [`catalog-from-github`](./catalog-from-github) | GitHub org / user repos | `groundwork.deployable` | ✓ working |
| [`catalog-from-gitlab`](./catalog-from-gitlab) | GitLab group / user projects | `groundwork.deployable` | ✓ working |

Planned (not built yet): `yard-from-github` / `yard-from-gitlab` (Actions /
Pipelines → `yard.test_run`), `union-from-okta` (users / groups →
`union.team`), one-shot agent-driven importers for Docker Compose / Helm /
Kustomize (run as MCP-driven skills, no separate binary).

## Operating model

Adapters run **inside the trust boundary** with a service-issued
on-behalf-of identity. The pattern matches the trusted-header auth shipped
for the MCP servers:

```
catalog-from-github  ──HTTP──►  GitHub API
                                                ┌──────────────┐
                  ──POST──────────────────────► │  groundwork  │
                  X-Manifold-User-Id: alice…    │  /deployable │
                                                └──────────────┘
                                                ┌──────────────┐
                  ──POST──────────────────────► │ manifold-    │
                  (record provenance)            │ ingest       │
                                                └──────────────┘
```

The adapter is configured with:

- `MANIFOLD_USER_ID` — the human on whose behalf this run is operating
- `MANIFOLD_USER_GROUPS` — comma-separated role list including the
  adapter's automation role (e.g. `automation:github-sync`). Casbin policy
  decides what that role can write.

Idempotency: on every record, the adapter calls
`/ingestion/graph::getByExternalSystem(...)` and looks for an entry whose
`external_id` matches the source-system natural key (e.g. `owner/repo` for
GitHub, `path_with_namespace` for GitLab). If found, it PUTs the existing
canonical id; if not, it POSTs a new record and writes a provenance row.

## Running an adapter

```bash
# GitHub
export GITHUB_TOKEN=ghp_…
export MANIFOLD_USER_ID=alice@example.dev
export MANIFOLD_USER_GROUPS=automation:github-sync
export MANIFOLD_GROUNDWORK_URL=http://localhost:3050
export MANIFOLD_INGEST_URL=http://localhost:3054
cargo run -p catalog-from-github -- --target tailoredshapes

# GitLab
export GITLAB_TOKEN=glpat-…
export MANIFOLD_USER_GROUPS=automation:gitlab-sync
cargo run -p catalog-from-gitlab -- --group my-team
# self-hosted GitLab:
cargo run -p catalog-from-gitlab -- --base-url https://gitlab.internal --group …
```

Re-running is safe — already-imported records are updated in place.

## Scheduling

Adapters are one-shot binaries today. For continuous sync, wrap in cron /
systemd timer / Kubernetes CronJob / a Cloud Scheduler entry. A real
scheduling layer (with idempotent retries, observability, secret rotation)
is a future deliverable; for first-customer pilots, cron is sufficient.
