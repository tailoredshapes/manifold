# Manifold

A federated suite of services for catalogue, governance, and people management. Built on [MeshQL-RS](https://github.com/tsmarsh/meshql-rs); each app is its own deployable that exposes REST + GraphQL and federates over the GraphQL surface so the catalogue, the org chart, and the deployment-planning machinery share one virtual graph.

## Applications

| App | Concern | Status |
|-----|---------|--------|
| [groundwork](groundwork/) | What runs, what it exposes, what it depends on | 🚧 v0.2 in flight |
| [cityhall](cityhall/) | Org hierarchy, governance bylaws, deployment plans, Gantt output | 🚧 scaffolding |
| [union](union/) | People, teams, work orders | 🚧 scaffolding |

Each app is single-responsibility. Cross-app data flows through MeshQL federation; nobody co-owns an entity.

## Entities

### Groundwork — runtime catalogue

| Entity | Required | Optional |
|--------|----------|----------|
| Deployable | `name` | `description`, `repo_url`, `team_id`* |
| Service | `name` | `type`, `description`, `endpoint` |
| Exposes | `deployable_id`, `service_id` | `port`, `protocol` |
| Dependency | `deployable_id`, `service_id` | `protocol`, `auth_method`, `criticality` |
| Contract | `service_id` | `spec_url`, `version`, `format` |
| Sla | `contract_id` | `metric`, `target`, `window` |

\* `team_id` resolves to **Union.Team** via federation.

### Union — people, teams, work

| Entity | Required | Optional |
|--------|----------|----------|
| Person | `name` | `contact`, `role` |
| Team | `name`, `kind` | `description` |
| TeamMember | `person_id`, `team_id` | `role` |
| WorkOrder | `team_id`, `summary` | `deployable_id`*, `change_request_id`†, `status`, `priority` |

\* `deployable_id` resolves to **Groundwork.Deployable**. † `change_request_id` resolves to **Cityhall.ChangeRequest**.

`Team.kind` is one of: `product`, `platform`, `security`, `domain`, `enterprise`, `infrastructure`, `support`.

### Cityhall — governance, change planning

| Entity | Required | Optional |
|--------|----------|----------|
| OrgNode | `name`, `kind` | `parent_id`, `team_id`* |
| Bylaw | `org_node_id`, `gate_type` | `priority`, `conditions`, `window`, `quiesce_for` |
| ChangeRequest | `summary` | `target_deployables`†, `target_versions`, `requested_by`‡ |
| DeploymentPlan | `change_request_id` | `steps`, `gates`, `windows`, `blockers` |
| GanttOutput | `deployment_plan_id` | `mermaid`, `tier` |

\* leaf `OrgNode`s reference **Union.Team**. † `target_deployables` references **Groundwork.Deployable**. ‡ `requested_by` references **Union.Person**.

`OrgNode.kind` is one of: `enterprise`, `division`, `domain`, `product`, `team`. Bylaws layer top-down: an Enterprise bylaw cannot be loosened by a Domain or Team bylaw underneath it.

`Bylaw.gate_type` is one of:

- `AutoGate` — passes without intervention if conditions hold.
- `ApprovalGate` — requires named approver(s) (resolved through Union).
- `WindowGate` — only opens during a stated time window.
- `QuiesceGate` — requires N minutes of zero alerts on the relevant deployable before opening.
- `FreezePeriod` — blocks changes outright during a window (e.g. quarter close).

## Federation map

```
Groundwork                Union                    Cityhall
─────────────             ────────────             ───────────────
Deployable.team_id ───────► Team
                            ▲ ▲
                            │ │
                            │ └────── OrgNode.team_id    (leaf nodes)
                            │
                            └──────── WorkOrder.team_id
                                       │
                                       │
WorkOrder.deployable_id ─► Deployable  │
                                       │
ChangeRequest.target_deployables ──► Deployable
ChangeRequest.requested_by    ─────────► Person
WorkOrder.change_request_id ◄──── ChangeRequest
```

Federation uses MeshQL's `@key`-style resolvers: each app exposes a stable id per entity, and consumers pull the foreign payload through their own `getById` fan-out.

## Cross-app reports

- **Union — orphan services:** join Groundwork.Deployable with Union.Team; flag deployables whose `team_id` is null or unresolvable.
- **Union — overcommitted teams:** count active WorkOrders per team; threshold per `Team.kind`.
- **Cityhall — blast-radius gate:** when planning a ChangeRequest, walk Groundwork dependency edges to find affected deployables; require an ApprovalGate per affected team.
- **Cityhall — orphan in plan:** if a transitively-affected deployable has no team, that's a hard blocker on the DeploymentPlan, not a warning.

## Deployment Targets

- **Docker / Kubernetes** — default local + staging (see `docker-compose.yml` for the three-service stack).
- **AWS** — ECS + MongoDB Atlas + EventBridge.
- **Azure** — Container Apps + MerkQL on Azure Files.

## Getting Started

```bash
cargo build --workspace
docker-compose up
# groundwork on :3000, union on :3001, cityhall on :3002
```

## Repository layout

```
manifold/
├── README.md                 # this file
├── Cargo.toml                # workspace manifest
├── docker-compose.yml        # three-service local stack
├── docs/
│   └── superpowers/plans/    # implementation plans, one per phase
├── groundwork/               # runtime catalogue
├── union/                    # people, teams, work
└── cityhall/                 # governance + change planning
```
