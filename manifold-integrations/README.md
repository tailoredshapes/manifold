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

| Adapter | Source | Target meshlette | Entities |
|---|---|---|---|
| [`catalog-from-github`](./catalog-from-github) | GitHub org / user repos | groundwork | `Deployable` |
| [`catalog-from-gitlab`](./catalog-from-gitlab) | GitLab group / user projects | groundwork | `Deployable` |
| [`yard-from-github`](./yard-from-github) | GitHub Actions workflows + runs | yard | `TestInfrastructure`, `TestEnvironment`, `TestSuite`, `TestRun` |
| [`yard-from-gitlab`](./yard-from-gitlab) | GitLab CI pipelines | yard | `TestInfrastructure`, `TestEnvironment`, `TestSuite`, `TestRun` |
| [`union-from-okta`](./union-from-okta) | Okta users + groups + memberships | union | `Person`, `Team`, `TeamMember` |

The yard adapters look up the deployable for each repo via `manifold-ingest`'s
`(github|gitlab, owner/repo)` records — if you run `catalog-from-*` first
the yard records get linked to deployables; otherwise the `deployable_id`
fields are left empty.

Planned: one-shot agent-driven importers for Docker Compose / Helm /
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
# Common
export MANIFOLD_USER_ID=alice@example.dev          # the human behind the run
export MANIFOLD_GROUNDWORK_URL=http://localhost:3050
export MANIFOLD_UNION_URL=http://localhost:3051
export MANIFOLD_YARD_URL=http://localhost:3053
export MANIFOLD_INGEST_URL=http://localhost:3054

# GitHub repos → Groundwork
export GITHUB_TOKEN=ghp_…
export MANIFOLD_USER_GROUPS=automation:github-sync
cargo run -p catalog-from-github -- --target tailoredshapes

# GitLab projects → Groundwork
export GITLAB_TOKEN=glpat-…
export MANIFOLD_USER_GROUPS=automation:gitlab-sync
cargo run -p catalog-from-gitlab -- --group my-team
# self-hosted GitLab:
cargo run -p catalog-from-gitlab -- --base-url https://gitlab.internal --group …

# GitHub Actions runs → Yard (run catalog-from-github first to link deployables)
export GITHUB_TOKEN=ghp_…
export MANIFOLD_USER_GROUPS=automation:github-yard-sync
cargo run -p yard-from-github -- --target tailoredshapes

# GitLab CI pipelines → Yard
export GITLAB_TOKEN=glpat-…
export MANIFOLD_USER_GROUPS=automation:gitlab-yard-sync
cargo run -p yard-from-gitlab -- --group my-team

# Okta users + groups → Union
export OKTA_TOKEN=00…
export MANIFOLD_USER_GROUPS=automation:okta-sync
cargo run -p union-from-okta -- --okta-domain my-org.okta.com
```

Re-running is safe — already-imported records are updated in place.

## Scheduling

Adapters are one-shot binaries today. For continuous sync, wrap in cron /
systemd timer / Kubernetes CronJob / a Cloud Scheduler entry. A real
scheduling layer (with idempotent retries, observability, secret rotation)
is a future deliverable; for first-customer pilots, cron is sufficient.
