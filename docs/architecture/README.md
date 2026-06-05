# Manifold — Architecture Documentation

This directory holds the client-shareable architecture documentation for **Manifold** and
its dependencies. The documents are split by altitude: one **Conceptual Architecture** that
holds across every deployment, and three **Logical System Architectures** — one per target
platform.

| Document | Altitude | Read it to understand… |
|----------|----------|------------------------|
| [Conceptual Architecture](conceptual-architecture.md) | Platform-neutral | What Manifold is, its domains, the federated information model, the actors, the principles, and the scale envelope. |
| [LSA — Kubernetes](logical-system-architecture-kubernetes.md) | Platform-specific | How Manifold runs on a Kubernetes cluster (EKS/AKS/GKE/on-prem). The portability / "customer mandates K8s" path. |
| [LSA — Azure](logical-system-architecture-azure.md) | Platform-specific | How Manifold runs on Azure App Service. **The default first-customer shape**, driven by the `conduit` Terraform repo. |
| [LSA — AWS](logical-system-architecture-aws.md) | Platform-specific | How Manifold runs on AWS Lambda + API Gateway + EFS. The serverless, scale-to-zero shape. |

## How to read these

1. **Start with the [Conceptual Architecture](conceptual-architecture.md).** Every LSA
   assumes it. It establishes the six domains, the federation map, the CQRS/MeshQL
   foundation, the edge-auth model, and the instance-per-tenant deployment philosophy.
2. **Then read the LSA for your target platform.** Each is self-contained from there and
   follows the same structure: logical building blocks → topology → component realisation →
   networking → build/supply chain → scaling → observability → backup/DR → security → "when
   to choose this".

## What's covered

The documents describe **Manifold and all of its dependencies** as one system:

- The six domain services — `groundwork`, `union`, `cityhall`, `yard`, `manifold-ingest`,
  `manifold-lobby`.
- The **MeshQL-RS** framework they are built on (read/write split, federation, pluggable
  persistence, Lambda packaging).
- The **edge / identity** layer (`manifold-edge`, Caddy, the IdP integration).
- The **integration / ingestion** adapters (`manifold-integrations`) and **provenance**
  ledger.
- The **MCP** agent-access layer and the embedded **UI**.

## Related references (not duplicated here)

- [`../deployment.md`](../deployment.md) — the deployment reference notes (scale envelope,
  shape rationale, first-real-deploy open items). The LSAs build on it.
- [`../../README.md`](../../README.md) — the repository README, with the authoritative
  entity field definitions and the federation map.
- The `conduit` repository — the per-client Azure Terraform IaC (authoritative for the Azure
  LSA).

## Maintenance

These are **living documents**. When the architecture changes — a new domain, a persistence
swap (SQLite → MerkQL), a new platform target, or a resolved open item — update the relevant
document and the "captured" date in its header. Keep platform detail in the LSAs and
cross-platform truth in the Conceptual Architecture; don't let them drift into duplicating
each other.
