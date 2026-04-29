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
