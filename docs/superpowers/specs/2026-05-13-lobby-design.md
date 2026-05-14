# Lobby — advisory tier for Manifold

> Captured 2026-05-13. Built from the design document provided by the user; this file is the engineering breakdown that actually shipped.

## What Lobby is

A fifth peer app in the Manifold suite. Pure read-and-derive over the federated graph: subscribes to the other four meshlettes (today via polling each `/graph` endpoint; tomorrow via merkql-notify, Mongo+Debezium+Kafka, or whatever event adapter is wired in at deploy time) and surfaces persistent, derived concerns as **advisories**.

Lobby owns its own meshlette with 6 entities — advisories, programs, program memberships, lifecycle entries, saved views, comments — but no underlying facts. Authoring of facts continues to happen in Groundwork, Union, Cityhall, Yard.

## Entity model

| Entity | Purpose |
|---|---|
| `Advisory` | The headline. id, kind, subject, severity, state, rule, explain, caused_by, raised_at, acknowledged_at, dismissed_at, resolved_at, dismiss_reason, escalated_to, re_raise_count, assignee. |
| `LifecycleEntry` | Append-only audit row per state transition. (advisory_id, at, actor_type, actor_id, action, reason, note). |
| `Program` | Cross-cutting tag. (name, description, leadership, color). |
| `ProgramMembership` | Many-to-many: programs ↔ subjects (deployable / cr / env / team). |
| `SavedView` | Named filter over the advisory inbox. (name, filter_json, owner, digest_schedule). |
| `Comment` | SME discussion thread on an advisory. (advisory_id, author, body, at). |

## Derivation rules

Six v1 rules ship. Each is a pure function over a `GraphSnapshot` returning the advisories it wants raised. The engine reconciles against existing state.

| Rule | Subject | Source data | Fires when |
|---|---|---|---|
| `blocked-upstream@v1` | deployable | CR.status=blocked, WorkOrder.status=blocked, Dependency, Exposes | A blocked deployable has downstream dependers; one advisory per dependent |
| `circular-dependency@v1` | deployable (first by name) | Dependency, Exposes | Tarjan SCC finds a strongly-connected component of size ≥ 2 |
| `undocumented-interface@v1` | service | Service, Dependency, Contract | A service has dependents and no contract published |
| `watershed-mismatch@v1` | test_environment | TestEnvironment, DataSync | An env declares watershed=prod-like but no DataSync targets it |
| `missing-environment@v1` | change_request | ChangeRequest, TestEnvironment | A CR's target deployable has no registered test env |
| `schedule-contention@v1` | change_request (smaller id) | DeploymentPlan, plan-step windows | Two CRs claim overlapping windows on the same deployable |

Rules are versioned (`@v1`) — bumping the version means changing the logic without retroactively invalidating dismissals. Stretch rules (`stale-ownership@v1`, `orphaned-environment@v1`) sit in the same shape; not yet shipped.

## Lifecycle

- **Raise** — system-only (humans don't raise advisories; they only act on them). Engine writes one on first observation.
- **Acknowledge** — user marks "I see it." No reason required.
- **Dismiss** — user marks "we're not going to act on this," with a controlled-vocabulary reason: `accepted-risk` / `false-positive` / `deferred` / `compensating-control` / `other`. Optional freetext note. Dismissed advisories disappear from default views but re-raise (with `re_raise_count++`) if the rule fires again on the same subject.
- **Resolve** — system-only. Triggered when a rule that previously fired stops firing for the configured **quiet window** (default 1 hour, per-rule override via `LOBBY_QUIET_WINDOW_MINUTES_<RULE_UPPER>` env). Conservative to avoid resolve/reraise flapping under noisy ingest.
- **Escalate** — meta action: writes a lifecycle entry and sets `escalated_to`. Doesn't change state.
- **Assign** — sets `assignee`.
- **Comment** — appended to the comment thread for the advisory.

Lifecycle is append-only. Corrections are new entries, not edits.

## Engine

A tokio task polls each source meshlette at `LOBBY_POLL_INTERVAL_SECONDS` (default 30s). Each pass:

1. Fetch a fresh `GraphSnapshot` via the source clients (each meshlette's `/graph` GraphQL endpoint plus REST list for deployment plans, which store steps as a JSON-encoded string).
2. Run every rule against the snapshot.
3. Index rule output by `(rule, subject_id)`.
4. Reconcile against existing advisories:
   - **In both**: refresh `explain`. State unchanged.
   - **New**: create a fresh advisory in `raised` state. Write `raise` lifecycle entry.
   - **Gone (and not in `dismissed`/`resolved` already)**: check quiet window; if elapsed, transition to `resolved`.
   - **Dismissed/resolved that the rule wants again**: re-raise; bump `re_raise_count`; write `re-raise` lifecycle entry.

Polling is the v1 event source. Swapping in merkql-notify or Mongo CDC → Kafka is a `sources.rs` change; the rule code and engine reconciliation stay identical.

## Cross-app integration

- All four primary apps publish `lobby_public_url` via `/config.json`.
- Each advisory subject deep-links to its source app via the suite's hash-routing convention (`#deployables/{id}`, `#changes/{id}`, etc.).
- Pills like "Open advisories: 3 →" on each source app's detail pages are wired up via the published `lobby_public_url` — frontend changes per app to render the pill against Lobby's `/advisory/graph::getBySubjectId` are pending.

## Auth

Standard Manifold auth: Caddy at edge → trusted headers → `manifold-edge` middleware → `CasbinAuth<StashKeyAuth>` per the suite-wide pattern. Roles:

- `admin` — full read/write
- `viewer` — read-only
- `automation:lobby-derive` — the derivation engine's own role (writes advisories + lifecycle entries on behalf of the system)

Dev synthetic identities: `alice@example.dev → admin`, `lobby-system → automation:lobby-derive`.

## MCP surface

`manifold-lobby-mcp` exposes auto-derived read/write tools for all 6 entities — `list_advisorys`, `get_advisory_by_id`, `create_advisory` (note: humans rarely create directly; the derivation engine raises), `update_advisory`, `delete_advisory`, etc. Plus the same surface for programs, lifecycle entries, saved views, comments.

Writes require `MANIFOLD_USER_ID` in the MCP server's env (same trusted-header pass-through as the other meshlettes).

## Frontend

Vanilla ES modules at `manifold-lobby/static/`. Same design tokens as the other four (editorial paper, sans-serif body, serif headings).

Three screens shipped: Inbox, Programs, Audit. Saved-views sidebar with four defaults (CTO summary, EA structural, Open warnings, All advisories).

Inbox has a detail drawer with all five action buttons (Acknowledge / Dismiss / Escalate / Assign / Comment) plus lifecycle history and comment thread.

Calendar and Map screens are deferred — Calendar depends on plan-window times being meaningfully populated by Cityhall's planner (v1 assigns synthetic sequential windows; richer time semantics are follow-up work); Map is Cytoscape-heavy frontend work that's independent of the backend.

## Schema additions in source apps

- **Yard**: `TestEnvironment.watershed` (new optional field, enum: `prod` / `prod-like` / `path-to-prod`). New `MaintenanceWindow` entity scaffolded but not yet wired into yard's `main.rs` — defers until ScheduleContention needs the maintenance source.
- **Cityhall**: `PlanStep` gains `window_start`, `window_end`, `test_environment_id` (all optional). Planner now assigns sequential windows from `computed_at + 30min` based on `estimated_minutes`. Real scheduling is a follow-up.

## What's not in scope this round

- Calendar screen (depends on richer plan-window semantics)
- Map screen (Cytoscape program-clustered dependency graph)
- "Open advisories →" pills in the other four frontends (data path wired, frontend rendering pending)
- Demo program seed (Meridian Freight programs would make the Programs screen non-empty out of the box)
- Stretch rules: `stale-ownership@v1`, `orphaned-environment@v1`
- Saved-view email digest subscriptions
- Yard `MaintenanceWindow` entity wiring (scaffolded; needs main.rs integration)
