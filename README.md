# Manifold

A federated suite of services for catalogue, governance, and people management. Built on [MeshQL-RS](https://github.com/tsmarsh/meshql-rs); each app is its own deployable that exposes REST + GraphQL and federates over the GraphQL surface so the catalogue, the org chart, and the deployment-planning machinery share one virtual graph.

## Applications

| App | Concern | Status |
|-----|---------|--------|
| [groundwork](groundwork/) | What runs, what it exposes, what it depends on | 🚧 v0.2 in flight |
| [cityhall](cityhall/) | Org hierarchy, governance bylaws, deployment plans, Gantt output | 🚧 scaffolding |
| [union](union/) | People, teams, work orders | 🚧 scaffolding |
| [yard](yard/) | Test environments, data sync, run history, infra estimation | 🚧 scaffolding |

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

### Yard — test infrastructure & data coordination

| Entity | Required | Optional |
|--------|----------|----------|
| TestEnvironment | `name`, `kind` | `deployable_id`*, `service_id`*, `infrastructure_id`, `mock_source_id`, `cost_per_hour`, `spinup_minutes`, `teardown_policy`, `max_duration_minutes`, `concurrency_limit`, `rate_limit`, `contractual_limit`, `notes` |
| TestInfrastructure | `name`, `provider` | `region`, `instance_type`, `cost_per_hour`, `notes` |
| MockSource | `name` | `repo_url`, `path`, `language`, `notes` |
| DataSource | `name`, `kind` | `location`, `refresh_policy`, `notes` |
| DataSync | `kind`, `target_env_id` | `source_env_id`, `source_data_id`, `refresh_policy`, `estimated_minutes`, `notes` |
| TestRun | `test_environment_id` | `change_request_id`†, `test_suite_id`, `team_id`‡, `started_at`, `finished_at`, `status`, `duration_minutes`, `cost_actual` |
| TestSuite | `name` | `deployable_id`*, `runner`, `command`, `description` |

\* `deployable_id` / `service_id` resolve to **Groundwork.Deployable** / **Groundwork.Service**. † `change_request_id` resolves to **Cityhall.ChangeRequest**. ‡ `team_id` resolves to **Union.Team**.

`TestEnvironment.kind` ∈ {mock, stub, sandbox, isolated, multi-tenant, external}.
`DataSource.kind` ∈ {prod_snapshot, synthetic, fixtures, external_mock}.
`DataSync.kind` ∈ {push, pull, shared}.
`DataSync.refresh_policy` ∈ {on_demand, periodic, per_test_run, versioned}.
`TestRun.status` ∈ {pending, running, passed, failed, cancelled, errored}.

## Federation map

```
Groundwork                Union                    Cityhall                       Yard
─────────────             ────────────             ───────────────                ──────────────
Deployable.team_id ───────► Team
                            ▲ ▲
                            │ │
                            │ └────── OrgNode.team_id    (leaf nodes)
                            │
                            └──────── WorkOrder.team_id ◄────── TestRun.team_id
                                       │
                                       │
WorkOrder.deployable_id ─► Deployable  │                                          ◄────── TestEnvironment.deployable_id
                                       │                                          ◄────── TestSuite.deployable_id
ChangeRequest.target_deployables ──► Deployable
ChangeRequest.requested_by    ─────────► Person
WorkOrder.change_request_id ◄──── ChangeRequest ◄──────────────────────────────── TestRun.change_request_id
                                          │
                                          └────────── DeploymentPlan.test_environments  ─────────► TestEnvironment
```

Federation uses MeshQL's `@key`-style resolvers: each app exposes a stable id per entity, and consumers pull the foreign payload through their own `getById` fan-out.

## Cross-app reports

- **Union — orphan services:** join Groundwork.Deployable with Union.Team; flag deployables whose `team_id` is null or unresolvable.
- **Union — overcommitted teams:** count active WorkOrders per team; threshold per `Team.kind`.
- **Cityhall — blast-radius gate:** when planning a ChangeRequest, walk Groundwork dependency edges to find affected deployables; require an ApprovalGate per affected team.
- **Cityhall — orphan in plan:** if a transitively-affected deployable has no team, that's a hard blocker on the DeploymentPlan, not a warning.
- **Yard — change-request estimate:** given a Cityhall ChangeRequest, walk its target deployables, find or recommend a TestEnvironment per deployable, sum spin-up time + cost + sync time, and emit infrastructure / data / coordination tasks back into the Cityhall Gantt.
- **Yard — sync recommendation:** given two TestEnvironments and the Groundwork dependency edge between their deployables, pick the right `DataSync.kind` (event-based → push pipeline; API-based → pull on setup; shared DB → shared data lake).
- **Yard — historical estimation:** group TestRuns by deployable + tier and emit average duration / cost / failure-rate so Cityhall plans can assert "last time this took 12 hours".

## Deployment Targets

- **Docker / Kubernetes** — default local + staging (see `docker-compose.yml` for the four-service stack).
- **AWS** — ECS + MongoDB Atlas + EventBridge.
- **Azure** — Container Apps + MerkQL on Azure Files.

## Getting Started

```bash
cargo build --workspace
docker-compose up
# groundwork on :3000, union on :3001, cityhall on :3002, yard on :3003
```

## Repository layout

```
manifold/
├── README.md                 # this file
├── Cargo.toml                # workspace manifest
├── docker-compose.yml        # four-service local stack
├── docs/
│   └── superpowers/plans/    # implementation plans, one per phase
├── groundwork/               # runtime catalogue
├── union/                    # people, teams, work
├── cityhall/                 # governance + change planning
└── yard/                     # test envs, data sync, run history
```
