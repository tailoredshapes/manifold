# Manifold deployment reference

> Captured 2026-05-13. Authoritative for first-customer deployments. Update as we learn from the first real deploy.

## Scale envelope

Manifold is **back-office tooling**. Assume:
- < 100 users/day per tenant
- < 1 GB of data per tenant (the working set fits in RAM)
- Read-heavy — every read is indexed by design (meshql's GraphQL layer is fan-out / batched-id)
- **Responsiveness > throughput.** Cold starts kill the UX; tail latency does not.

The dominant access pattern is **MCP-driven** (an LLM agent makes 20-50 tool calls per session), not browser-driven. Latency budget belongs to MCP reads.

## Tenancy

**One instance per tenant.** No multi-tenancy inside a single instance. Two sales motions, same artifact:

1. Customer self-hosts in their own cloud (Azure for first client; AWS / GCP follow). Customer owns the infra.
2. We host as managed service — same deployable bundle, in our cloud account, billed per-tenant.

Per-customer customization happens at the **edge auth config + Casbin policy** level, never inside the Rust services.

## Architecture facts that constrain the shape

- **5 meshlettes** (groundwork, union, cityhall, yard, manifold-ingest) — each an axum HTTP server today
- **Trusted-header auth at the edge** — Caddy or cloud-native equivalent (Easy Auth on Azure, API Gateway authorizer on AWS) does authn; services read identity from configurable headers via `manifold-edge::HeaderConfig`
- **Casbin authz in-process** — `CasbinAuth<StashKeyAuth>` per meshlette, embedded model + policy files
- **merkql for persistent storage** — append-only log, safe over network filesystems (Azure Files SMB, AWS EFS). Single writer per file is the design assumption.
- **MCP servers are stdio binaries** on the user's machine / CI; they don't deploy with the service tier.

At our scale, the 5 meshlettes can deploy as **one monolith binary** (`manifold-monolith`) with all five routers merged into a single axum app — same source, different packaging. The federation seam stays in the source for clarity and for customers who later outgrow the monolith.

## Recommended shapes

### Azure (default for first customer)

```
Azure subscription
└── Resource Group
    ├── App Service Plan B1 ($13/mo) — Linux, always-on, single shared compute
    │   └── Linux Web App "manifold-monolith"
    │       ├── Easy Auth (Entra) → injects X-MS-CLIENT-PRINCIPAL-NAME
    │       ├── Custom handler: manifold-monolith binary on port 3000
    │       └── Azure Files SMB mount → /data/merkql
    └── Storage Account ($2/mo)
        └── File Share "merkql"  (5 GB quota, 5 subdirs for the 5 meshlettes)

Per-tenant: ~$15/mo all-in
```

Why B1 not EP1: `meshql-rs/examples/farm-azure/` uses EP1 Premium because Azure Functions custom handler with VNet + file mount requires Premium. We don't need Functions or VNet — App Service Linux Web App on B1 supports the same SMB mount, costs ~10× less, and is always-warm by default (no cold starts).

Reference for IaC: copy from `meshql-rs/examples/farm-azure/terraform/` but swap the `azurerm_linux_function_app` (EP1) for `azurerm_linux_web_app` (B1) following `conduit/terraform/azure/functions.tf`'s `azurerm_linux_web_app` pattern.

### AWS

```
AWS account
└── VPC
    ├── EFS file system + access points → /mnt/merkql/<meshlette>
    ├── Lambda functions (ARM64 PROVIDED_AL2023):
    │     ├── manifold-monolith     (provisioned concurrency = 1)
    │     └── manifold-ingest       (provisioned concurrency = 1)
    └── API Gateway HTTP API (catch-all /{proxy+})
        └── JWT authorizer or Cognito user pool — IdP per customer

Per-tenant: ~$10/mo (mostly provisioned concurrency to kill cold starts)
```

Reference for IaC: `meshql-rs/examples/egg-economy-lambda/cdk/` (AWS CDK TypeScript).

**Cold-start caveat:** Rust Lambda cold start is 100-300ms even on ARM64. At 100 users/day, traffic is bursty enough that Lambda scales to zero between sessions, so every visitor pays the cold-start tax. Provisioned concurrency at ~$2-5/mo per function eliminates it; ~$5-10/mo total is well inside the budget envelope.

### Fallback shapes

- **Kubernetes** (`meshql/examples/logistics/k8s/`) — Deployment + Service + ConfigMap, Nginx ingress, MongoDB sidecar. Works on EKS / AKS / on-prem. Use only if customer mandates K8s.
- **AWS + Kafka** (`meshql-rs/examples/egg-economy-ksql/`) — Phase-4 distributed-scale path. Not relevant at our scale; flagged for future-customers-with-real-load case.

## Per-customer customization layer

| What changes per customer | Where it lives |
|---|---|
| IdP integration (Entra / Cognito / Okta / Auth0 / …) | Edge auth config: Easy Auth identity provider, or API Gateway authorizer settings |
| Header name mapping (`X-MS-CLIENT-PRINCIPAL-NAME` → `X-Manifold-User-Id`) | App env: `MANIFOLD_USER_HEADER`, `MANIFOLD_GROUPS_HEADER` |
| Roles & policy | Per-app `config/auth/policy.csv` (rebuilds the image) |
| Storage account / EFS sizing | Terraform variables |

The Rust binary doesn't change per customer.

## Codebase work still required for first real deploy

In order, none of them large:

1. **`manifold-monolith` bin** — new file in the workspace; merges all 5 routers into one axum app behind path prefixes. ~30 lines of glue.
2. **Swap `SqliteRepository` → `MerkqlRepository`** in each meshlette's `make_entity()`. 5 × ~3 lines.
3. **Re-run all four cucumber cert suites against merkql** to surface any matcher / query-pattern incompatibilities.
4. **Adapt `meshql-rs/examples/farm-azure/terraform/`** into `manifold/deploy/azure/`. Web App not Function App; mount path `/data/merkql`.
5. **Adapt `meshql-rs/examples/egg-economy-lambda/cdk/`** into `manifold/deploy/aws/`. Two Lambdas (monolith + ingest) instead of one.
6. **Published images / artifacts** — CI build + push for the monolith binary (Docker for Azure, Lambda zip for AWS).
7. **"Stand up a new tenant" runbook** for the managed-hosting variant. Probably a script wrapping the terraform/CDK with a tenant name.

## Things to verify on first real deploy

- merkql performance vs sqlite for the larger meshlettes (yard's 23 envs × multi-relational reads)
- Easy Auth header naming: does Azure Easy Auth actually emit the headers we expect, or do we need a Caddy-shaped sidecar?
- Cross-meshlette federation latency in the monolith (should be in-process, but measure)
- Backup/restore: rsync the merkql log directory + storage account snapshot policy
- First-customer pricing motion confirmed (managed-host charge needs to be > ~$50/mo to leave clean margin on a $15/mo Azure spend)

## Source-of-truth references

| Reference | Use for |
|---|---|
| `meshql-rs/examples/farm-azure/` | Azure binary packaging (custom handler, app_command_line, Azure Files mount semantics) |
| `meshql-rs/examples/egg-economy-lambda/cdk/` | AWS Lambda packaging, EFS, API Gateway, CDK structure |
| `conduit/terraform/azure/functions.tf` | App Service Linux Web App on B1 with Azure Files mount (the cheap path) |
| `meshql-rs/meshql-merkql/` | Repository + Searcher trait impls over merkql |
| `meshql-rs/meshql-lambda/` | `lambda_http`-wrapping of an axum app |
| `meshql/examples/logistics/k8s/` | Kubernetes manifest pattern (fallback only) |
