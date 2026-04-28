# Manifold

A suite of tools for service catalog and dependency management, built on [MeshQL-RS](https://github.com/tsmarsh/meshql-rs).

## Applications

| App | Description | Status |
|-----|-------------|--------|
| [groundwork](groundwork/) | Service catalog — register applications, services, dependencies, SLAs, and contracts | 🚧 In development |

## Deployment Targets

- **Docker / Kubernetes** — default local + staging
- **AWS** — ECS + MongoDB Atlas + EventBridge
- **Azure** — Container Apps + MerkQL on Azure Files

## Getting Started

```bash
cargo build --workspace
```
