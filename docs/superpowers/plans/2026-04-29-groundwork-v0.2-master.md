# Groundwork v0.2 — Master Plan

> **For agentic workers:** This is the master plan. Each phase has its own plan document. Implementation must follow the phases in order. REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans for each phase plan.

**Goal:** Evolve Groundwork from a flat application catalogue into a multi-format service graph that can ingest from and emit to Kubernetes / Ansible / Terraform, and expose itself to LLMs over MCP.

**Architecture:** Five entities (Deployable, Service, Exposes, Dependency, Contract, Sla) backed by per-entity SQLite stores via `meshql-rs`. Each IaC format gets a self-contained `importers::<format>` module with `parse` and `emit` halves that operate on a normalised `CatalogDelta`. A separate `groundwork-mcp` binary exposes the catalogue over MCP for LLM consumption. The HTTP/GraphQL/REST surface remains the source of truth.

**Tech Stack:** Rust 2021, axum 0.7, sqlx 0.8 (sqlite), `meshql-rs` workspace siblings (core/server/sqlite/graphlette/restlette), serde + serde_yaml + serde_json, hcl-rs (for terraform), cucumber 0.21 for BDD. MCP transport via `mcp-rs` (or stdio JSON-RPC if `mcp-rs` is unavailable; decide in Phase 5).

---

## Why these changes

The v0.1 model conflated *the thing that runs* with *the thing other things consume*. In v0.1 an `Application` had a `tech_stack` and was the only first-class noun; a `Dependency` pointed `application_id → service_id` but a `Service` had no owner. That made it impossible to model:

- a managed/external service (Stripe, RDS) — a Service with no Deployable behind it;
- a single Deployable exposing multiple Services (e.g. a sidecar exposing both gRPC and HTTP);
- multiple Deployables behind one logical Service (blue/green, region failover).

v0.2 fixes this by separating *runtime* from *interface*:

| Concept | Role |
|---------|------|
| **Deployable** | A thing that runs somewhere. Owned by a team, lives in a repo. Required: `name`. |
| **Service** | A named, consumable interface with an endpoint. May or may not have a Deployable behind it. |
| **Exposes** | `deployable_id → service_id`. Zero or more per Deployable. |
| **Dependency** | `deployable_id → service_id`. Replaces v0.1's `application_id → service_id`. |
| **Contract**, **Sla** | Unchanged. |

`tech_stack` is removed: it was free-text noise that no consumer queried. If we ever want to track runtime stacks they belong on Deployable as a structured field (e.g. `runtime: { language, framework }`), and that's a future migration, not v0.2.

---

## Migration strategy

This is pre-1.0 with no production deployments. **Hard rename, no shims.** Local SQLite files in `groundwork/data/` are dev-only and will be regenerated. No data migration is required.

The HTTP surface changes:
- `/application/*` → `/deployable/*`
- new endpoint family `/exposes/*`
- `/dependency/*` body field renames `application_id` → `deployable_id`

The CI/Docker/k8s/terraform deployment artefacts (in `groundwork/terraform/`) and the GitLab CI pipeline are unaffected by the rename — they target the binary, not the schema.

---

## Phases

Each phase produces working, testable software on its own. Each has its own plan document.

| # | Phase | Plan |
|---|---|---|
| 1 | Model refactor: Application → Deployable, Exposes, drop tech_stack | [2026-04-29-phase-1-model-refactor.md](./2026-04-29-phase-1-model-refactor.md) |
| 2 | Kubernetes import/export | [2026-04-29-phase-2-kubernetes.md](./2026-04-29-phase-2-kubernetes.md) |
| 3 | Ansible import/export | [2026-04-29-phase-3-ansible.md](./2026-04-29-phase-3-ansible.md) |
| 4 | Terraform import/export | [2026-04-29-phase-4-terraform.md](./2026-04-29-phase-4-terraform.md) |
| 5 | MCP server (graph queries + import/export tools) | [2026-04-29-phase-5-mcp.md](./2026-04-29-phase-5-mcp.md) |

Phase 1 must complete before any other phase: every later phase writes Deployables and Exposes rows, so the model must be in place first. Phases 2–4 are independent of each other and can run in parallel after Phase 1. Phase 5 depends on Phase 1 (core graph queries) and gains incremental power from Phases 2–4 (import/export tools), but a useful subset (graph queries only) can ship the moment Phase 1 lands.

---

## Cross-cutting design decisions

### 1. The `CatalogDelta` normalised form

Every importer parses *into* a `CatalogDelta` and every emitter renders *from* a `CatalogSnapshot`. Defined once in `src/catalog.rs`:

```rust
pub struct CatalogDelta {
    pub deployables: Vec<DeployableInput>,
    pub services:    Vec<ServiceInput>,
    pub exposes:     Vec<ExposesInput>,
    pub dependencies: Vec<DependencyInput>,
    pub contracts:   Vec<ContractInput>,
    pub slas:        Vec<SlaInput>,
}

pub struct CatalogSnapshot {
    pub deployables: Vec<Deployable>,
    pub services:    Vec<Service>,
    pub exposes:     Vec<Exposes>,
    pub dependencies: Vec<Dependency>,
    pub contracts:   Vec<Contract>,
    pub slas:        Vec<Sla>,
}
```

`*Input` records may carry *natural keys* (e.g. a Deployable named `checkout`) instead of UUIDs, since importers don't know existing IDs. `apply_delta` resolves natural keys against the live store and creates/updates rows.

### 2. Importer/emitter module layout

```
groundwork/src/
  importers/
    mod.rs
    k8s.rs          # parse() / emit() for kubernetes manifests
    k8s/
      tests.rs
      fixtures/     # golden YAML files
    ansible.rs
    ansible/
      tests.rs
      fixtures/
    terraform.rs
    terraform/
      tests.rs
      fixtures/
  catalog.rs        # CatalogDelta, CatalogSnapshot, apply_delta
```

Each importer is independently compilable, with its own tests and fixtures. Importers use `serde_yaml` / `hcl-rs` to deserialise; they never call HTTP.

### 3. HTTP surface for import/export

```
POST /import/kubernetes    body: YAML manifest(s)
POST /import/ansible       body: inventory + playbook (multipart) or YAML bundle
POST /import/terraform     body: HCL
GET  /export/kubernetes    query: ?deployable=<name>&namespace=<ns>
GET  /export/ansible       query: ?inventory=<name>
GET  /export/terraform     query: ?provider=aws|gcp|azure
```

Implemented as axum routes that wrap the importer modules. Live behind the existing axum router in `main.rs`.

### 4. MCP architecture

`groundwork-mcp` is a **separate binary** in the same crate (or a sibling crate — TBD in Phase 5 plan) that:

- speaks MCP over stdio (or HTTP+SSE if MCP-rs supports it);
- talks to the running Groundwork server over HTTP for reads;
- exposes import/export tools by proxying to `/import/*` and `/export/*`;
- exposes graph-walk tools (`blast_radius`, `dependency_graph`, `deployment_plan`) computed locally from a fetched catalogue snapshot.

Loose coupling between MCP and the catalogue store keeps the MCP surface independent of storage backend (sqlite/mongo/merkql).

### 5. Testing strategy

- Every entity gets `tests/features/<entity>.feature` cucumber scenarios for CRUD + relationships.
- Every importer gets a `tests/features/import_<format>.feature` and a `tests/features/export_<format>.feature`.
- Importer round-trip property: `parse(emit(snapshot)) == snapshot` modulo IDs. Each phase plan includes the round-trip scenario.
- MCP server tested with cucumber via stdio harness (Phase 5 plan).

### 6. Frequent commits

Every BDD scenario is its own commit. Implementation commits land between the failing-test commit and the next failing-test commit, following TDD discipline (red → green → refactor). Each phase plan enforces this in its bite-sized step list.

---

## Out of scope for v0.2

- Authn/authz (the `NoAuth` placeholder stays).
- Multi-tenancy / namespacing across teams.
- A web UI for the Exposes relationship — the existing list-of-FK pattern in `static/app.js` will be extended; no new UI primitives needed.
- Pulumi, Crossplane, Helm chart import (Helm could be a later sub-phase of Kubernetes).
- Drift detection between live cluster and catalogue (a natural follow-on, but not required to ship v0.2).
- Versioning the GraphQL/REST surface — pre-1.0, breakage allowed.

---

## Execution order

1. Land Phase 1 in full, on `main`, with all CI green.
2. Branch Phases 2, 3, 4 from `main`. Land them in any order as they complete.
3. Branch Phase 5 once at least Phase 1 is on `main`. Add import/export tools to MCP as their respective phases land.

Each phase plan ends with a "Definition of done" checklist that gates merging.
