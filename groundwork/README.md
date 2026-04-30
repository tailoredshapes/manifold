# Groundwork

The first service in the [Manifold](../) suite. Lets teams register the things they run, the interfaces those things expose, and what depends on what. Start with just a name — enrich over time.

## Philosophy

- **Zero required fields beyond name** — just knowing a deployable exists is valuable
- **Progressive enrichment** — depth grows with demand, never blocked by incompleteness
- **Audit not ownership** — who registered what, not who owns what
- **Temporal history** — every change is versioned; query any entity as it was at any point in time

## Entities

| Entity | Required | Optional |
|--------|----------|---------|
| Deployable | `name` | description, repo_url, team |
| Service | `name` | type, description, endpoint |
| Exposes | `deployable_id`, `service_id` | port, protocol |
| Dependency | `deployable_id`, `service_id` | protocol, auth_method, criticality |
| Contract | `service_id` | spec_url, version, format |
| Sla | `contract_id` | metric, target, window |

## Deployment

### Local / Docker

```bash
cargo run
```

### AWS (MongoDB Atlas)

```bash
STORAGE=mongo MONGO_URI=<atlas-uri> cargo run --features mongo
```

### Azure (MerkQL)

```bash
STORAGE=merkql MERKQL_DATA_PATH=/mnt/merkql cargo run --features merkql
```

## Terraform

- `terraform/aws/` — ECS + MongoDB Atlas
- `terraform/azure/` — Container Apps + Azure Files
- `terraform/k8s/` — Kubernetes manifests

## MCP server

Groundwork ships an MCP server (`groundwork-mcp`) so an LLM (Claude, Qwen, etc.) can interrogate the catalogue in a structured way — list entities, walk the dependency graph, scope an outage, compute a deployment order. It speaks JSON-RPC 2.0 over stdio (one frame per line).

### Register with Claude Code

```bash
cargo build --release --bin groundwork-mcp
claude mcp add groundwork --env GROUNDWORK_URL=http://localhost:3050 -- "$(pwd)/target/release/groundwork-mcp"
```

`GROUNDWORK_URL` defaults to `http://localhost:3000`; point it at whichever Groundwork the LLM should query.

### Tools (Phase 5)

| Tool | Purpose |
|---|---|
| `catalog.list` | List every record of an entity type (deployable / service / exposes / dependency / contract / sla) |
| `catalog.get` | Fetch one record by id |
| `catalog.search` | Find records whose name matches a substring (case-insensitive) |
| `graph.blast_radius` | If this service goes down, which deployables — and which services those deployables expose — break, transitively? |
| `graph.dependencies_of` | What does this deployable consume? Forward walk. |
| `graph.deployment_plan` | Topologically order every deployable transitively required by the target; surface external (publisher-less) services as prerequisites; report cycles |

The IaC import/export tool family (Phase 2/3/4) is scoped for a future phase.

### Try it

Once registered, ask Claude something like *"What would break if Auth Service API went down?"* — it should call `catalog.search` to find the service id, then `graph.blast_radius` to enumerate the dependents.
