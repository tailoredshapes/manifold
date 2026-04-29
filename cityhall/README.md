# Cityhall

Org hierarchy, governance bylaws, and deployment planning for the [Manifold](../) suite. Owns OrgNode, Bylaw, ChangeRequest, DeploymentPlan, GanttOutput. Federates Team out of [Union](../union/) and Deployable out of [Groundwork](../groundwork/).

## Philosophy

- **Bylaws layer top-down** — Enterprise rules cannot be loosened by Domain or Team rules underneath. Children may add, never override.
- **Plans are computed and stored** — a DeploymentPlan is the materialised result of resolving a ChangeRequest against the bylaw chain and the dependency graph.
- **Mermaid Gantt is the deliverable** — the output of a DeploymentPlan is a renderable Mermaid Gantt chart, deterministic given the same inputs.
- **Loose coupling** — Cityhall doesn't store deployables or teams; it asks Groundwork and Union over HTTP.

## Entities

| Entity | Required | Optional |
|--------|----------|----------|
| OrgNode | `name`, `kind` | `parent_id`, `team_id` |
| Bylaw | `org_node_id`, `gate_type` | `priority`, `description`, `conditions`, `window`, `quiesce_for`, `approvers` |
| ChangeRequest | `summary` | `description`, `target_deployables`, `target_versions`, `requested_by`, `tier`, `status` |
| DeploymentPlan | `change_request_id` | `tier`, `steps`, `blockers`, `computed_at` |
| GanttOutput | `deployment_plan_id` | `tier`, `mermaid` |

`OrgNode.kind` ∈ {enterprise, division, domain, product, team}.
`Bylaw.gate_type` ∈ {AutoGate, ApprovalGate, WindowGate, QuiesceGate, FreezePeriod}.
`ChangeRequest.tier` ∈ {dev, integration, uat, prod}.
`ChangeRequest.status` ∈ {draft, submitted, approved, rejected, deployed, rolled_back}.

## Bylaw layering

When evaluating gates for a leaf OrgNode, walk ancestors from the root down. Collect every Bylaw attached to any ancestor. **Higher layers cannot be overridden** — a child may add a stricter gate, but cannot mark a parent's gate as not applicable. The merge is union, not override.

## Custom routes

- `GET /org_node/:id/ancestors` — root-first ancestor chain.
- `GET /org_node/:id/effective_bylaws` — collected bylaws along the chain.
- `POST /change_request/:id/plan` — compute (and persist) a DeploymentPlan from a ChangeRequest.
- `POST /deployment_plan/:id/gantt` — render (and persist) a Mermaid Gantt chart.

## Run

```bash
cargo run -p cityhall                   # local, port 3002
PORT=3002 GROUNDWORK_URL=http://localhost:3000 UNION_URL=http://localhost:3001 cargo run -p cityhall
```
