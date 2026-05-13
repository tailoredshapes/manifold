# MCP writes + provenance ledger design

> Captured 2026-05-13. Shipped same session.

## Goal

Let an LLM agent (Claude Desktop / Claude Code / any MCP client) **write** into the Manifold federation, and record where every write came from, so:

1. One-shot imports (Docker Compose / Helm / Kustomize) become an agent + MCP conversation, not a bespoke parser-and-UI per format.
2. Continuous adapters (GitHub / GitLab / Okta) have a single canonical place to land both the primary-domain record and the provenance row.
3. Audit and disaster recovery are graph queries against one meshlette, not a forensic dig through six.

## Two principles

**P1. The MCP server is a stateless trust-pass-through.** It does not store credentials, does not negotiate identity with the LLM, does not have a fallback identity. Either it was started with `MANIFOLD_USER_ID` in its env, in which case every outbound write carries that as `X-Manifold-User-Id`, or it wasn't, in which case write tools return a clean error to the MCP client. *How* the meat got the value into the env is the LLM's / human's problem.

**P2. Primary-domain meshlettes stay clean.** Provenance does not pollute `groundwork.deployable.payload` with `external_id` / `via_role` / `imported_at` fields. The mapping lives in its own meshlette (`manifold-ingest`) and links by `canonical_id`. Audit is a federated graph query; recovery is "rehydrate from ingest's `raw` field."

## Wire

```
LLM client (.mcp.json)
    env: MANIFOLD_USER_ID=alice@example.dev
       │
       ▼
groundwork-mcp                                          ┌──────────────┐
  ├─ create_deployable(...)   ─── POST /deployable/api ─▶│  groundwork  │
  │   X-Manifold-User-Id: alice@example.dev               │              │
  │   (trusted-header auth → CasbinAuth → role=admin)    │              │
  │                                                      └──────────────┘
  │   ◀── { id: <canonical>, ... }
  │
manifold-ingest-mcp
  └─ create_ingestion({
        external_system: "docker-compose-upload",
        external_id:     "meridian.compose.yml",
        target_domain:   "groundwork.deployable",
        canonical_id:    <canonical>,
        on_behalf_of:    "alice@example.dev",
        via_role:        "human:cli-import"
     })                       ─── POST /ingestion/api ──▶┌──────────────┐
                                                        │ manifold-    │
                                                        │ ingest       │
                                                        └──────────────┘
```

## Changes (meshql-rs)

* **`meshql-mcp::client::MeshqlClient`** carries an optional `Identity { user_id, groups }`. `MeshqlClient::with_identity_from_env()` reads `MANIFOLD_USER_ID` and `MANIFOLD_USER_GROUPS`. `from_env(...)` calls it automatically so existing apps get writes for free. Every outbound HTTP call (GET / POST / PUT / DELETE / GraphQL) attaches the configured identity as `X-Manifold-User-Id` / `X-Manifold-User-Groups`.

* **`meshql-mcp::capability::CapabilityHandler`** gains three write variants — `EntityCreate { api_path }`, `EntityUpdate { api_path }`, `EntityDelete { api_path }`. The dispatcher calls `client.require_identity()?` before any HTTP; if identity is unset the caller receives a clear error directing them to set `MANIFOLD_USER_ID` in the MCP client's config.

* **`CapabilitiesBuilder::auto_from_schemas`** now derives, for every entity, three additional capabilities on top of the existing reads:
  - `create_X(payload fields)` — required fields = schema's non-null fields. Federated projections and `id` excluded.
  - `update_X(id, payload fields)` — required = `id`; all payload fields optional.
  - `delete_X(id)` — required = `id`.

* **`MeshqlClient` writes** add `put_path` and `delete_path` to round out the verb set.

## Changes (manifold)

* **New crate `manifold-ingest`** (smallest possible meshlette — one entity `Ingestion`, four GraphQL ops, embedded Casbin policy with `automation:*` roles). Spawned with the same edge-header + CasbinAuth stack as the primary apps.

* **New bin `manifold-ingest-mcp`** with the auto-derived list/get/find/create/update/delete capabilities. `create_ingestion` gets a richer description so agents know to call it after every primary-domain write.

* **`.mcp.json`** adds `MANIFOLD_USER_ID=alice@example.dev` to all five MCP server entries (dev synthetic identity). The fifth entry is `manifold-ingest`.

* **`docker-compose.yml`** adds `manifold-ingest` on port `3054` and a `manifold-ingest-data` volume.

## What the agent sees

After the next `claude` session:

```
mcp__groundwork__list_deployables             (read)
mcp__groundwork__get_deployable_by_id         (read)
mcp__groundwork__find_deployables_by_name     (read)
mcp__groundwork__create_deployable            (write — requires identity)
mcp__groundwork__update_deployable            (write — requires identity)
mcp__groundwork__delete_deployable            (write — requires identity)
...                                           (same shape for service / dependency / exposes / contract / sla)
mcp__union__create_team        (etc.)
mcp__cityhall__create_org_node (etc.)
mcp__yard__create_test_environment (etc.)
mcp__manifold-ingest__create_ingestion        (provenance record — call after every write)
```

Pre-change: 91 tools. Post-change: ~150 tools (5 servers × auto-derived reads + 3 writes per entity).

## What's not in scope this round

* Pre-built skill prompts that walk an agent through the import flow — comes next (item 4 in the auth roadmap; small markdown).
* Continuous adapters (GitHub / GitLab / Okta) — separate crates in a follow-up.
* Cross-meshlette transactional consistency. If the primary-domain write succeeds and the ingest write fails, the row is orphaned; for v1 we tolerate it (auditing can detect "deployable with no ingest record" via federation).
* Group-hierarchy in Casbin (`automation:*` is a flat role for now).
