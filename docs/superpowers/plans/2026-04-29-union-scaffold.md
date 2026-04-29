# Union — scaffold + entity implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Implementation steps tagged **(delegate)** are good Qwen units once `claude-code-delegate` is restored. Architecture/integration steps stay with Claude.

**Goal:** A new Manifold app that owns Person, Team, TeamMember, WorkOrder. Same architectural shape as Groundwork: per-entity SQLite stores via `meshql-rs`, REST + GraphQL surfaces, BDD via cucumber, vanilla-JS UI. Federation hooks (Team key publishable to Groundwork/Cityhall, WorkOrder.deployable_id pointing into Groundwork) are landed in this scaffold but actual cross-app resolvers ship in the federation phase.

**Tech Stack:** Rust 2021, axum 0.7, sqlx 0.8 (sqlite), `meshql-rs` workspace siblings, cucumber 0.21.

---

## File map

```
union/
├── Cargo.toml
├── README.md
├── src/
│   └── main.rs
├── config/
│   ├── json/
│   │   ├── person.schema.json
│   │   ├── team.schema.json
│   │   ├── team_member.schema.json
│   │   └── work_order.schema.json
│   └── graph/
│       ├── person.graphql
│       ├── team.graphql
│       ├── team_member.graphql
│       └── work_order.graphql
├── static/
│   ├── index.html
│   └── app.js
└── tests/
    ├── union_cert.rs
    └── features/
        ├── person.feature
        ├── team.feature
        ├── team_member.feature
        ├── work_order.feature
        └── web_ui.feature
```

Workspace manifest (`manifold/Cargo.toml`) gains `"union"` in `members`.

---

## Entity definitions

### Person

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["name"],
  "properties": {
    "name":    { "type": "string" },
    "contact": { "type": "string" },
    "role":    { "type": "string" }
  },
  "additionalProperties": true
}
```

```graphql
type Person {
  id: ID
  name: String!
  contact: String
  role: String
}
type Query {
  getById(id: ID, at: Float): Person
  getAll(at: Float): [Person]
  getByName(name: String, at: Float): [Person]
}
```

### Team

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["name", "kind"],
  "properties": {
    "name":        { "type": "string" },
    "kind":        { "type": "string", "enum": ["product", "platform", "security", "domain", "enterprise", "infrastructure", "support"] },
    "description": { "type": "string" }
  },
  "additionalProperties": true
}
```

```graphql
type Team {
  id: ID
  name: String!
  kind: String!
  description: String
}
type Query {
  getById(id: ID, at: Float): Team
  getAll(at: Float): [Team]
  getByName(name: String, at: Float): [Team]
  getByKind(kind: String, at: Float): [Team]
}
```

### TeamMember

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["person_id", "team_id"],
  "properties": {
    "person_id": { "type": "string" },
    "team_id":   { "type": "string" },
    "role":      { "type": "string" }
  },
  "additionalProperties": true
}
```

```graphql
type TeamMember {
  id: ID
  person_id: String!
  team_id: String!
  role: String
}
type Query {
  getById(id: ID, at: Float): TeamMember
  getAll(at: Float): [TeamMember]
  getByPersonId(person_id: String, at: Float): [TeamMember]
  getByTeamId(team_id: String, at: Float): [TeamMember]
}
```

### WorkOrder

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["team_id", "summary"],
  "properties": {
    "team_id":            { "type": "string" },
    "summary":            { "type": "string" },
    "deployable_id":      { "type": "string" },
    "change_request_id":  { "type": "string" },
    "status":             { "type": "string", "enum": ["proposed", "in_progress", "blocked", "done", "cancelled"] },
    "priority":           { "type": "string", "enum": ["low", "medium", "high", "urgent"] }
  },
  "additionalProperties": true
}
```

```graphql
type WorkOrder {
  id: ID
  team_id: String!
  summary: String!
  deployable_id: String
  change_request_id: String
  status: String
  priority: String
}
type Query {
  getById(id: ID, at: Float): WorkOrder
  getAll(at: Float): [WorkOrder]
  getByTeamId(team_id: String, at: Float): [WorkOrder]
  getByDeployableId(deployable_id: String, at: Float): [WorkOrder]
  getByChangeRequestId(change_request_id: String, at: Float): [WorkOrder]
  getByStatus(status: String, at: Float): [WorkOrder]
}
```

---

## BDD scenarios

Following the pattern from `groundwork/tests/features/`, one feature file per entity plus `web_ui.feature`.

### `person.feature`

```gherkin
Feature: Person CRUD

  Background:
    Given a Union server is running

  Scenario: Register a person with just a name
    When I POST to "/person/api" with body {"name": "Ada Lovelace"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot register a person without a name
    When I POST to "/person/api" with body {}
    Then the response status should be 400

  Scenario: Update contact and role
    Given I have registered person "Grace Hopper"
    When I PUT "/person/api/<ids.Grace Hopper>" with body {"name": "Grace Hopper", "contact": "ghopper@navy.mil", "role": "Rear Admiral"}
    Then the response status should be 200
    And the response body should contain "Rear Admiral"

  Scenario: Find by name via GraphQL
    Given I have registered person "Margaret Hamilton"
    When I query the "person" graph with: { getByName(name: "Margaret Hamilton") { id name } }
    Then there should be no GraphQL errors
    And the response data should contain "Margaret Hamilton"
```

### `team.feature`

```gherkin
Feature: Team CRUD

  Background:
    Given a Union server is running

  Scenario: Register a product team
    When I POST to "/team/api" with body {"name": "checkout-team", "kind": "product"}
    Then the response status should be 201
    And the response body should contain "checkout-team"
    And the response body should contain "product"

  Scenario: Cannot register a team without a kind
    When I POST to "/team/api" with body {"name": "orphan"}
    Then the response status should be 400

  Scenario: Cannot register with an invalid kind
    When I POST to "/team/api" with body {"name": "weird", "kind": "wizards"}
    Then the response status should be 400

  Scenario: Find teams by kind
    Given I have registered team "checkout-team" with kind "product"
    And I have registered team "platform-eng" with kind "platform"
    And I have registered team "appsec" with kind "security"
    When I query the "team" graph with: { getByKind(kind: "product") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "checkout-team"
    And the response data should not contain "platform-eng"
```

The `not contain` step is new to Union but trivially mirrors `body_contains` with negation.

### `team_member.feature`

```gherkin
Feature: Team membership

  Background:
    Given a Union server is running
    And I have registered person "Ada Lovelace"
    And I have registered team "checkout-team" with kind "product"

  Scenario: Assign a person to a team
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.checkout-team>", "role": "lead"}
    Then the response status should be 201

  Scenario: Cannot assign without person_id
    When I POST to "/team_member/api" with body {"team_id": "<ids.checkout-team>"}
    Then the response status should be 400

  Scenario: List members of a team
    Given I have registered person "Grace Hopper"
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.checkout-team>"}
    And I POST to "/team_member/api" with body {"person_id": "<ids.Grace Hopper>", "team_id": "<ids.checkout-team>"}
    And I query the "team_member" graph with: { getByTeamId(team_id: "<ids.checkout-team>") { person_id role } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.Ada Lovelace>"
    And the response data should contain "<ids.Grace Hopper>"

  Scenario: A person can be on multiple teams (matrix)
    Given I have registered team "appsec" with kind "security"
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.checkout-team>", "role": "lead"}
    And I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.appsec>", "role": "champion"}
    And I query the "team_member" graph with: { getByPersonId(person_id: "<ids.Ada Lovelace>") { team_id role } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.checkout-team>"
    And the response data should contain "<ids.appsec>"
```

### `work_order.feature`

```gherkin
Feature: Work orders

  Background:
    Given a Union server is running
    And I have registered team "checkout-team" with kind "product"

  Scenario: Open a work order against a team
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "rotate db credentials"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot open without a summary
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>"}
    Then the response status should be 400

  Scenario: Status transitions through update
    Given I have opened work order "rotate-creds" against "checkout-team"
    When I PUT "/work_order/api/<ids.rotate-creds>" with body {"team_id": "<ids.checkout-team>", "summary": "rotate db credentials", "status": "in_progress"}
    Then the response status should be 200
    And the response body should contain "in_progress"

  Scenario: Filter open work by team
    Given I have opened work order "task-a" against "checkout-team"
    And I have opened work order "task-b" against "checkout-team"
    When I query the "work_order" graph with: { getByTeamId(team_id: "<ids.checkout-team>") { summary status } }
    Then there should be no GraphQL errors
    And the response data should contain "task-a"
    And the response data should contain "task-b"

  Scenario: A work order can reference a deployable (federation hook)
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "tune SLO", "deployable_id": "external-deployable-uuid"}
    Then the response status should be 201
    And the response body should contain "external-deployable-uuid"
```

### `web_ui.feature`

Mirror Groundwork's: serves index, app.js with `loadEntity` + `createRecord`, `/health`, 404. Sidebar shows the four entities.

---

## Slices

### Slice 0 — workspace bootstrap

- [ ] Add `"union"` to `manifold/Cargo.toml` `[workspace] members`.
- [ ] Create `union/Cargo.toml` mirroring `groundwork/Cargo.toml`'s dependency block. Crate name: `union`. Package description: "People, teams, and work orders for the Manifold suite".
- [ ] Create `union/README.md` with the entity table (copy from manifold/README.md Union section).
- [ ] `cargo build --workspace` clean.
- [ ] Commit.

### Slice 1 — entity wiring (mirror Groundwork main.rs)

For each entity:

- [ ] Write `config/json/<entity>.schema.json` (per the schemas above).
- [ ] Write `config/graph/<entity>.graphql` (per the schemas above).
- [ ] In `src/main.rs`, declare const `<ENTITY>_GRAPHQL`, build an `<entity>_gql_config`, add to `ServerConfig.graphlettes`, build a restlette and merge into `extra`.

Default port: `PORT=3001`. Default `DATA_DIR=./data`. Same `make_required_validator` helper as Groundwork (or extract it shared in a future refactor; not in this slice).

Skipped from Groundwork's pattern: this slice does **not** include the JSON-Schema enum validation for `Team.kind`, `WorkOrder.status`, `WorkOrder.priority`. The current `make_required_validator` only checks required-fields. Add an enum validator helper:

```rust
fn make_enum_validator(schema: &serde_json::Value) -> ValidatorFn {
    let required = /* same as before */;
    let enums: HashMap<String, Vec<String>> = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| props.iter().filter_map(|(k, v)| {
            v.get("enum").and_then(|e| e.as_array()).map(|arr| {
                (k.clone(), arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            })
        }).collect())
        .unwrap_or_default();
    Arc::new(move |payload, _| {
        // required-field check (as before), then:
        for (field, allowed) in &enums {
            if let Some(v) = payload.get(field.as_str()).and_then(|x| x.as_str()) {
                if !allowed.iter().any(|a| a == v) {
                    return Err(format!("Field '{field}' must be one of {allowed:?}, got {v:?}"));
                }
            }
        }
        Ok(())
    })
}
```

Use this validator for Team and WorkOrder so the "wizards" / bad-status scenarios actually 400.

### Slice 2 — BDD harness and step defs

- [ ] Copy `groundwork/tests/groundwork_cert.rs` to `union/tests/union_cert.rs`. Rename `GroundworkWorld` → `UnionWorld`. Update `make_entity` calls + entity wiring to mirror Slice 1.
- [ ] Add step defs:
  - `I have registered person "..."` — POST `/person/api` with `{name}`, store id.
  - `I have registered team "X" with kind "Y"` — POST `/team/api` with `{name: X, kind: Y}`, store id.
  - `I have opened work order "X" against "team_name"` — resolves team_id, POST `/work_order/api` with `{team_id, summary: X}`.
  - `the response body should not contain "..."` — negate `body_contains`.
- [ ] Add a `[[test]]` entry in `union/Cargo.toml` mirroring Groundwork.
- [ ] Run scenarios; iterate until green.
- [ ] Commit.

### Slice 3 — Web UI

- [ ] Copy `groundwork/static/index.html` and `app.js`, change branding to "union", change sidebar nav items to (people, teams, members, work-orders), and rewrite the `ENTITIES` config.
- [ ] Add `Person`, `Team`, `TeamMember`, `WorkOrder` entries with `dynamic-select` for FK fields:
  - `team_member.person_id` selects from `data.people`
  - `team_member.team_id` selects from `data.teams`
  - `work_order.team_id` selects from `data.teams`
  - `work_order.deployable_id` is a free-text input — federation lookup is for later.
- [ ] Manual UI smoke; commit.

### Slice 4 — Reports (stubs)

Three report endpoints, returning stub JSON until federation lands:

- `GET /reports/orphan-services` — returns `[]` for now; comment says "joins Groundwork.Deployable.team_id with Union.Team in federation phase".
- `GET /reports/overcommitted-teams` — counts active WorkOrders per team, threshold 5 per team for now.
- `GET /reports/team-coverage` — counts members per team, returns teams with zero members.

The non-stub overcommitted-teams report is implementable today (no federation), so do it. The other two are stubs.

- [ ] BDD scenarios for all three (overcommitted gets concrete asserts; the others assert shape only).
- [ ] Implement.
- [ ] Commit.

### Definition of done

- [ ] `cargo test --workspace` passes.
- [ ] `cargo run -p union` starts, serves on `PORT=3001`, all four REST endpoints work end-to-end via curl smoke.
- [ ] UI shows four entity tabs, supports CRUD on each.
- [ ] No reference to `groundwork` or `cityhall` in `union/src` (loose coupling).
