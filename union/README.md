# Union

People, teams, and work orders for the [Manifold](../) suite. Owns Person, Team, TeamMember, WorkOrder. Federates Team out to Groundwork (deployable ownership) and Cityhall (org chart leaves).

## Philosophy

- **People-first** — every other catalogue refers back to a person or a team. Union is the source of truth for both.
- **Audit not headcount** — record who works on what, not how much they work.
- **Loose coupling** — Union doesn't know about deployables or change requests; Groundwork and Cityhall point in.
- **Temporal history** — every change is versioned via meshql-rs's `at:` timestamp queries.

## Entities

| Entity | Required | Optional |
|--------|----------|----------|
| Person | `name` | `contact`, `role` |
| Team | `name`, `kind` | `description` |
| TeamMember | `person_id`, `team_id` | `role` |
| WorkOrder | `team_id`, `summary` | `deployable_id`, `change_request_id`, `status`, `priority` |

`Team.kind` ∈ {product, platform, security, domain, enterprise, infrastructure, support}.
`WorkOrder.status` ∈ {proposed, in_progress, blocked, done, cancelled}.
`WorkOrder.priority` ∈ {low, medium, high, urgent}.

## Reports

- `GET /reports/orphan-services` — services without team ownership (federation-resolved against Groundwork).
- `GET /reports/overcommitted-teams` — teams with more than N active work orders.
- `GET /reports/team-coverage` — teams with zero members.

## Run

```bash
cargo run -p union                 # local, port 3001
PORT=3001 DATA_DIR=./data cargo run -p union
```
