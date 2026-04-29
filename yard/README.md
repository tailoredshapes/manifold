# Yard

Test infrastructure, data sync, and run-history estimation for the [Manifold](../) suite. Owns TestEnvironment, TestInfrastructure, MockSource, DataSource, DataSync, TestRun, TestSuite. Federates Deployable / Service out of [Groundwork](../groundwork/), ChangeRequest out of [Cityhall](../cityhall/), and Team out of [Union](../union/).

## Philosophy

- **Infra is a first-class entity** — every test environment carries the time and money it costs, the contractual limits it lives under, and the policy that tears it down.
- **Data is a federated edge** — DataSync points from one TestEnvironment (or DataSource) to another, mirroring Groundwork's dependency edges.
- **Estimation is empirical** — TestRun history feeds back into estimates. "Last time this took 12 hours" beats "I think this is a 3-day job."
- **Loose coupling** — Yard never owns a Deployable, ChangeRequest, or Team. It pulls them over HTTP at query time.

## Entities

| Entity | Required | Optional |
|--------|----------|----------|
| TestEnvironment | `name`, `kind` | `deployable_id`, `service_id`, `infrastructure_id`, `mock_source_id`, `cost_per_hour`, `spinup_minutes`, `teardown_policy`, `max_duration_minutes`, `concurrency_limit`, `rate_limit`, `contractual_limit`, `notes` |
| TestInfrastructure | `name`, `provider` | `region`, `instance_type`, `cost_per_hour`, `notes` |
| MockSource | `name` | `repo_url`, `path`, `language`, `notes` |
| DataSource | `name`, `kind` | `location`, `refresh_policy`, `notes` |
| DataSync | `kind`, `target_env_id` | `source_env_id`, `source_data_id`, `refresh_policy`, `estimated_minutes`, `notes` |
| TestRun | `test_environment_id` | `change_request_id`, `test_suite_id`, `team_id`, `started_at`, `finished_at`, `status`, `duration_minutes`, `cost_actual` |
| TestSuite | `name` | `deployable_id`, `runner`, `command`, `description` |

`TestEnvironment.kind` ∈ {mock, stub, sandbox, isolated, multi-tenant, external}.
`TestEnvironment.teardown_policy` ∈ {on_finish, on_idle, manual, never}.
`DataSource.kind` ∈ {prod_snapshot, synthetic, fixtures, external_mock}.
`DataSync.kind` ∈ {push, pull, shared}.
`DataSync.refresh_policy` ∈ {on_demand, periodic, per_test_run, versioned}.
`TestRun.status` ∈ {pending, running, passed, failed, cancelled, errored}.

## Custom routes

- `POST /change_request/:id/estimate` — given a Cityhall ChangeRequest, walk its target deployables, find or recommend a TestEnvironment per deployable, sum spin-up cost and time, and return infrastructure / data / coordination tasks suitable for the Cityhall Gantt.
- `POST /data_sync/recommend` — given source and target Groundwork dependency type, recommend the right `DataSync.kind` (event-based → push; API-based → pull; shared DB → shared).
- `GET /test_environment/:id/history` — average duration, cost, and pass-rate across this env's TestRuns.
- `GET /test_environment/:id/availability` — whether the env is within its concurrency / rate / contractual limits right now (used by Cityhall to schedule test windows).

## Run

```bash
cargo run -p yard                                           # local, port 3003
PORT=3003 GROUNDWORK_URL=http://localhost:3000 \
CITYHALL_URL=http://localhost:3002 UNION_URL=http://localhost:3001 \
cargo run -p yard
```
