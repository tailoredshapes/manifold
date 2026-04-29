# Phase 1 — Model Refactor: Application → Deployable, Exposes, drop tech_stack

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the v0.1 model (Application + Service + Dependency + Contract + Sla) into the v0.2 model (Deployable + Service + Exposes + Dependency + Contract + Sla), drop `tech_stack`, and rename `application_id` → `deployable_id` on Dependency. No other behaviour changes.

**Architecture:** Pure rename + one new entity. Apply the existing per-entity SQLite/Graphlette/Restlette pattern to a new `exposes` entity. No data migration required (pre-1.0, dev data only).

**Tech Stack:** Rust 2021, axum 0.7, sqlx 0.8 (sqlite), `meshql-rs`, cucumber 0.21.

---

## File map

| File | Operation | Notes |
|---|---|---|
| `groundwork/config/json/application.schema.json` | **delete** | Replaced by `deployable.schema.json`. |
| `groundwork/config/json/deployable.schema.json` | **create** | Same shape as application.schema.json **minus** `tech_stack`. |
| `groundwork/config/json/dependency.schema.json` | **modify** | `application_id` → `deployable_id`. |
| `groundwork/config/json/exposes.schema.json` | **create** | Required: `deployable_id`, `service_id`. Optional: `port`, `protocol`. |
| `groundwork/config/graph/application.graphql` | **delete** | Replaced. |
| `groundwork/config/graph/deployable.graphql` | **create** | Mirrors v0.1 application.graphql, minus `tech_stack`. |
| `groundwork/config/graph/dependency.graphql` | **modify** | Field + query rename. |
| `groundwork/config/graph/exposes.graphql` | **create** | New entity schema. |
| `groundwork/src/main.rs` | **modify** | Rename entity name, paths, schema includes; add exposes wiring. |
| `groundwork/static/app.js` | **modify** | Rename `applications` → `deployables`, drop tech_stack field, add exposes config, update dependency dynamic-select. |
| `groundwork/static/index.html` | **modify** | Rename sidebar label + nav badge ID. |
| `groundwork/tests/features/application.feature` | **rename → deployable.feature, modify** | Same scenarios with renamed entity. |
| `groundwork/tests/features/exposes.feature` | **create** | New CRUD + relationship scenarios. |
| `groundwork/tests/features/dependency.feature` | **create** | (Did not exist in v0.1.) Cover the rename. |
| `groundwork/tests/features/web_ui.feature` | **modify** | Rename `applications` → `deployables` in expected sidebar text if any. |
| `groundwork/tests/groundwork_cert.rs` | **modify** | Update step defs that reference `application` paths/fields; load `deployable` schema; add exposes registration helper. |
| `groundwork/data/application.db` | **delete** | Stale. |
| `groundwork/data/exposes.db` | (created at runtime) | Auto-created by SqliteRepository. |

---

## Background notes for the implementer

- Every entity in v0.1 follows this pattern in `main.rs`:
  1. `make_entity(&data_dir, "<name>").await` builds the SQLite-backed repo+searcher.
  2. A JSON Schema is loaded for required-field validation.
  3. A `RootConfig` for GraphQL is built (singleton `getById`, vector `getAll`, plus per-entity vector queries).
  4. A `GraphletteConfig` is added to `ServerConfig.graphlettes` at path `/<entity>/graph`.
  5. A `meshql_server::build_restlette_router_ext` is built at path `/<entity>/api` and merged into the extra Router.
- Each entity's GraphQL schema lives in `config/graph/<entity>.graphql`.
- Each entity's JSON Schema lives in `config/json/<entity>.schema.json`.
- The UI is a vanilla-JS ES module (`static/app.js`) driven by the `ENTITIES` config object. Each entry has `api`, `label`, `newFields`, `detailFields`, `primaryField`, `getRowLabel`, `getRowBadge`, optional `readonlyInDetail`.
- Cucumber scenarios live in `tests/features/*.feature` and step defs in `tests/groundwork_cert.rs`.
- The cucumber harness (`#[tokio::main] async fn main`) runs on `tests/features` directory.

---

## Slice 0 — preflight

- [ ] **Step 0.1: Verify the current test suite passes**

```bash
cd /tank/repos/tailoredshapes/manifold/groundwork
cargo test --test groundwork_cert 2>&1 | tail -20
```

Expected: 12 scenarios passing. If anything fails, **stop and investigate** before refactoring.

- [ ] **Step 0.2: Stage current uncommitted UI changes**

The repo has uncommitted modifications to `groundwork/src/main.rs`, `groundwork/static/app.js`, and `groundwork/static/index.html`, plus untracked schema/graphql files for service/dependency/contract/sla. These are the **multi-entity rollout in progress** that we are about to refactor on top of. Confirm by reading them; if they look complete (multi-entity catalog working end-to-end), commit them as their own commit before starting the refactor:

```bash
git add groundwork/config/graph/contract.graphql \
        groundwork/config/graph/dependency.graphql \
        groundwork/config/graph/service.graphql \
        groundwork/config/graph/sla.graphql \
        groundwork/config/json/contract.schema.json \
        groundwork/config/json/dependency.schema.json \
        groundwork/config/json/service.schema.json \
        groundwork/config/json/sla.schema.json \
        groundwork/src/main.rs \
        groundwork/static/app.js \
        groundwork/static/index.html
git commit -m "feat: multi-entity catalog (Service, Dependency, Contract, Sla)"
```

Leave the `groundwork/data/*.db` files untracked — they are dev-local.

---

## Slice 1 — drop `tech_stack`

We do this first because it is the smallest possible change and lets us verify the full test loop with no rename pressure.

### Task 1A: BDD scenario for tech_stack removal

**Files:** Modify `groundwork/tests/features/application.feature`.

- [ ] **Step 1A.1: Add a failing scenario asserting `tech_stack` is rejected by the schema**

Append to `application.feature`:

```gherkin
  Scenario: tech_stack is no longer a known field on the schema
    When I POST to "/application/api" with body {"name": "old-style", "tech_stack": "rust"}
    Then the response status should be 201
    # We don't reject unknown fields (additionalProperties: true), but the schema must
    # not list tech_stack. We assert by querying the GraphQL schema introspection.
    When I query the "application" graph with: { __type(name: "Application") { fields { name } } }
    Then there should be no GraphQL errors
    And the response body should not contain "tech_stack"
```

Add a new step def `the response body should not contain "..."` mirroring the existing `body_contains` helper, with `assert!(!body.contains(...))`.

- [ ] **Step 1A.2: Run and confirm it fails**

```bash
cargo test --test groundwork_cert 2>&1 | tail -10
```

Expected: the scenario fails because the GraphQL schema currently exposes `tech_stack`.

### Task 1B: Remove tech_stack from JSON schema, GraphQL schema, UI

**Files:**
- Modify: `groundwork/config/json/application.schema.json`
- Modify: `groundwork/config/graph/application.graphql`
- Modify: `groundwork/static/app.js` (`applications.newFields` and `applications.detailFields`)

- [ ] **Step 1B.1: Remove `tech_stack` from JSON schema**

Edit `groundwork/config/json/application.schema.json` to remove the `tech_stack` property line.

- [ ] **Step 1B.2: Remove `tech_stack` from GraphQL schema**

Edit `groundwork/config/graph/application.graphql` to remove the `tech_stack: String` line.

- [ ] **Step 1B.3: Remove `tech_stack` from UI config**

In `groundwork/static/app.js`, find the `applications` block; remove the `tech_stack` entry from both `newFields` and `detailFields`.

- [ ] **Step 1B.4: Run tests, confirm pass**

```bash
cargo test --test groundwork_cert 2>&1 | tail -20
```

Expected: all scenarios pass, including the new tech_stack one.

- [ ] **Step 1B.5: Commit**

```bash
git add groundwork/config/json/application.schema.json \
        groundwork/config/graph/application.graphql \
        groundwork/static/app.js \
        groundwork/tests/features/application.feature \
        groundwork/tests/groundwork_cert.rs
git commit -m "refactor: drop tech_stack from Application (was noise, no consumers)"
```

---

## Slice 2 — rename Application → Deployable

This is the core rename. We do everything in one go because partial states (some files using `application`, others `deployable`) won't compile.

### Task 2A: Update BDD feature file (rename in scenarios)

**Files:**
- Rename: `groundwork/tests/features/application.feature` → `groundwork/tests/features/deployable.feature`

- [ ] **Step 2A.1: `git mv` the feature file**

```bash
git mv groundwork/tests/features/application.feature groundwork/tests/features/deployable.feature
```

- [ ] **Step 2A.2: Replace `application` → `deployable` in the feature file**

In `groundwork/tests/features/deployable.feature`, replace every occurrence of:
- `Application CRUD` → `Deployable CRUD`
- `application` → `deployable` (lowercase, including paths like `/application/api`, GraphQL `application` graph)
- `applications` → `deployables`
- `Application` → `Deployable` (in GraphQL type references)

Edit by hand or `sed -i 's/application/deployable/g; s/Application/Deployable/g' groundwork/tests/features/deployable.feature`. **Inspect the diff before continuing** — `application.feature` may legitimately mention "applicable" etc. (It doesn't, but check anyway.)

### Task 2B: Update step defs in `groundwork_cert.rs`

**Files:**
- Modify: `groundwork/tests/groundwork_cert.rs`

- [ ] **Step 2B.1: Replace include strings + paths**

In `groundwork_cert.rs`, replace:
- `include_str!("../config/graph/application.graphql")` → `include_str!("../config/graph/deployable.graphql")`
- `include_str!("../config/json/application.schema.json")` → `include_str!("../config/json/deployable.schema.json")`
- `path: "/application/graph".into()` → `path: "/deployable/graph".into()`
- `"/application/api"` → `"/deployable/api"` (in `register_app_raw` and step defs)
- `register_app_raw` → `register_deployable_raw`
- `register_one` → `register_one_deployable`, `register_many` → `register_many_deployables`
- `update_app_given` → `update_deployable_given`
- step def regex literals: `application "(.+)"` → `deployable "(.+)"`, `applications:` → `deployables:`

### Task 2C: Rename schema/graphql files

**Files:**
- Rename: `groundwork/config/json/application.schema.json` → `groundwork/config/json/deployable.schema.json`
- Rename: `groundwork/config/graph/application.graphql` → `groundwork/config/graph/deployable.graphql`

- [ ] **Step 2C.1: `git mv` both files**

```bash
git mv groundwork/config/json/application.schema.json groundwork/config/json/deployable.schema.json
git mv groundwork/config/graph/application.graphql groundwork/config/graph/deployable.graphql
```

- [ ] **Step 2C.2: Update GraphQL type name inside the schema**

In `groundwork/config/graph/deployable.graphql`, replace `type Application` with `type Deployable` and update return types accordingly:

```graphql
type Deployable {
    id: ID
    name: String!
    description: String
    repo_url: String
    team: String
}
type Query {
    getById(id: ID, at: Float): Deployable
    getAll(at: Float): [Deployable]
    getByName(name: String, at: Float): [Deployable]
}
```

### Task 2D: Update `main.rs`

**Files:**
- Modify: `groundwork/src/main.rs`

- [ ] **Step 2D.1: Rename const, identifiers, paths**

In `groundwork/src/main.rs`:
- `APPLICATION_GRAPHQL` → `DEPLOYABLE_GRAPHQL`
- `include_str!("../config/graph/application.graphql")` → `include_str!("../config/graph/deployable.graphql")`
- `application_schema_json` → `deployable_schema_json`
- `include_str!("../config/json/application.schema.json")` → `include_str!("../config/json/deployable.schema.json")`
- `make_entity(&data_dir, "application")` → `make_entity(&data_dir, "deployable")`
- `let application = ...` → `let deployable = ...`
- `application_gql_config` → `deployable_gql_config`
- `application_restlette` → `deployable_restlette`
- `path: "/application/graph"` → `path: "/deployable/graph"`
- `"/application/api"` → `"/deployable/api"`

Compile-check at this point — `cargo check` should be clean before moving on.

### Task 2E: Update UI

**Files:**
- Modify: `groundwork/static/app.js` and `groundwork/static/index.html`

- [ ] **Step 2E.1: In `app.js`, rename top-level key `applications` → `deployables`**

Find the `applications:` block inside `ENTITIES`. Rename the key to `deployables`. Update:
- `api: '/application/api'` → `api: '/deployable/api'`
- `label: 'application'` → `label: 'deployable'`

Also in the `dependencies` block, update `optionsFrom: (data) => data.applications.map(...)` to `data.deployables.map(...)` and the `getRowLabel` resolver: `data.applications.find(...)` → `data.deployables.find(...)`. Same in `readonlyInDetail`.

In `state.data` initial object, rename `applications: []` → `deployables: []`.

In `state.activeEntity` default, change from `'applications'` to `'deployables'`.

- [ ] **Step 2E.2: In `index.html`, rename the sidebar nav item**

```html
<div class="nav-item active" data-entity="deployables">
  <span>deployables</span>
  <span class="nav-badge" id="badge-deployables">0</span>
</div>
```

### Task 2F: Drop the stale dev DB

- [ ] **Step 2F.1: Delete `groundwork/data/application.db`**

```bash
rm -f groundwork/data/application.db
```

It's untracked, but if it was ever staged this prevents confusion.

### Task 2G: Verify and commit the rename

- [ ] **Step 2G.1: Run the full test suite**

```bash
cargo test --test groundwork_cert 2>&1 | tail -30
```

Expected: all scenarios still pass (renamed). If the dependency tests fail, that's expected — Slice 3 fixes them.

> **Likely-failure check:** the `dependency.feature` (if present) refers to `application_id`. The compile is fine, but new dependency rows still expect `application_id`. Slice 3 fixes that. If a *deployable* scenario fails, stop and debug before proceeding.

- [ ] **Step 2G.2: Commit**

```bash
git add -A
git commit -m "refactor: rename Application → Deployable (entity, schema, GraphQL, REST, UI, tests)"
```

---

## Slice 3 — rename Dependency.application_id → deployable_id

### Task 3A: Update Dependency JSON schema and GraphQL schema

**Files:**
- Modify: `groundwork/config/json/dependency.schema.json`
- Modify: `groundwork/config/graph/dependency.graphql`

- [ ] **Step 3A.1: In `dependency.schema.json`, rename `application_id` → `deployable_id`**

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["deployable_id", "service_id"],
  "properties": {
    "deployable_id": { "type": "string" },
    "service_id": { "type": "string" },
    "protocol": { "type": "string" },
    "auth_method": { "type": "string" },
    "criticality": { "type": "string" }
  },
  "additionalProperties": true
}
```

- [ ] **Step 3A.2: In `dependency.graphql`, rename field + query**

```graphql
type Dependency {
    id: ID
    deployable_id: String!
    service_id: String!
    protocol: String
    auth_method: String
    criticality: String
}
type Query {
    getById(id: ID, at: Float): Dependency
    getAll(at: Float): [Dependency]
    getByDeployableId(deployable_id: String, at: Float): [Dependency]
    getByServiceId(service_id: String, at: Float): [Dependency]
}
```

### Task 3B: Update `main.rs` query templates

**Files:**
- Modify: `groundwork/src/main.rs`

- [ ] **Step 3B.1: In `dependency_gql_config`, rename query**

```rust
let dependency_gql_config = RootConfig::builder()
    .singleton("getById", r#"{"id": "{{id}}"}"#)
    .vector("getAll", "{}")
    .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
    .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
    .build();
```

### Task 3C: Update UI (Dependency entity in `app.js`)

**Files:**
- Modify: `groundwork/static/app.js`

- [ ] **Step 3C.1: Rename `application_id` → `deployable_id` in the dependencies entity config**

In the `dependencies:` block of `ENTITIES`:
- `newFields[0].name`: `'application_id'` → `'deployable_id'`
- `newFields[0].label`: `'application'` → `'deployable'`
- `primaryField`: `'application_id'` → `'deployable_id'`
- `getRowLabel`: rename `payload.application_id` → `payload.deployable_id`
- `readonlyInDetail[0]`: rename name + resolve to use `deployable_id`

### Task 3D: BDD scenarios for the rename

**Files:**
- Create: `groundwork/tests/features/dependency.feature`

- [ ] **Step 3D.1: Write the dependency feature**

```gherkin
Feature: Dependency relationship (Deployable → Service)

  Background:
    Given a Groundwork server is running
    And I have registered deployable "checkout"
    And I have registered service "payments-api"

  Scenario: Register a dependency from a deployable to a service
    When I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.payments-api>"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot register a dependency without deployable_id
    When I POST to "/dependency/api" with body {"service_id": "<ids.payments-api>"}
    Then the response status should be 400

  Scenario: Cannot register a dependency without service_id
    When I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>"}
    Then the response status should be 400

  Scenario: Find dependencies by deployable
    Given I have registered service "search-api"
    And I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.payments-api>"}
    And I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.search-api>"}
    When I query the "dependency" graph with: { getByDeployableId(deployable_id: "<ids.checkout>") { id service_id } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.payments-api>"
    And the response data should contain "<ids.search-api>"
```

Note: this requires the test harness to also register **services** and **dependencies** entities (currently it only wires the deployable). Either:
1. **Easy path:** extend `build_test_server()` in `groundwork_cert.rs` to wire all five entities (mirroring `main.rs`), OR
2. **Stricter path:** extract a shared helper module so the test server and `main.rs` share an entity-registration function.

Recommend **option 1** for Phase 1 (simpler, less ambitious surgery on `meshql-rs` boundaries). Phase 5 may revisit.

- [ ] **Step 3D.2: Extend `build_test_server` to wire all entities**

In `groundwork_cert.rs::build_test_server`, mirror `main.rs::main` for all five entities (deployable, service, dependency, contract, sla). Replace the in-memory pool boilerplate with one helper:

```rust
async fn make_test_entity(name: &str) -> (Arc<dyn meshql_core::Repository>, Arc<dyn meshql_core::Searcher>) {
    let pool = make_pool().await;
    let repo = Arc::new(SqliteRepository::new_with_pool(pool.clone()).await.unwrap());
    let searcher: Arc<dyn meshql_core::Searcher> =
        Arc::new(SqliteSearcher::new_with_pool(pool).await.unwrap());
    (repo, searcher)
}
```

Then build a `GraphletteConfig` + `build_restlette_router_ext` for each entity, exactly mirroring `main.rs`.

Add step defs for `I have registered service "..."`, mirroring the existing deployable step.

- [ ] **Step 3D.3: Run, confirm pass**

```bash
cargo test --test groundwork_cert 2>&1 | tail -30
```

Expected: dependency scenarios pass.

- [ ] **Step 3D.4: Commit**

```bash
git add -A
git commit -m "refactor: rename Dependency.application_id → deployable_id (schema, GraphQL, UI, BDD)"
```

---

## Slice 4 — add Exposes entity

### Task 4A: BDD scenarios first

**Files:**
- Create: `groundwork/tests/features/exposes.feature`

- [ ] **Step 4A.1: Write the exposes feature**

```gherkin
Feature: Exposes relationship (Deployable exposes Service)

  Background:
    Given a Groundwork server is running
    And I have registered deployable "checkout"
    And I have registered service "checkout-api"

  Scenario: Record that a deployable exposes a service
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot record exposes without deployable_id
    When I POST to "/exposes/api" with body {"service_id": "<ids.checkout-api>"}
    Then the response status should be 400

  Scenario: Cannot record exposes without service_id
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>"}
    Then the response status should be 400

  Scenario: Record exposes with optional port and protocol
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>", "port": "8080", "protocol": "http"}
    Then the response status should be 201
    And the response body should contain "8080"

  Scenario: A service can be exposed by multiple deployables
    Given I have registered deployable "checkout-canary"
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>"}
    And I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout-canary>", "service_id": "<ids.checkout-api>"}
    Then the response status should be 201
    When I query the "exposes" graph with: { getByServiceId(service_id: "<ids.checkout-api>") { deployable_id } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.checkout>"
    And the response data should contain "<ids.checkout-canary>"

  Scenario: A service may exist with no deployables exposing it (managed/external)
    Given I have registered service "stripe"
    When I query the "exposes" graph with: { getByServiceId(service_id: "<ids.stripe>") { id } }
    Then there should be no GraphQL errors
    And the response array should have 0 items
```

Note: the last scenario asserts a JSON array. The current `array_has_items` step asserts on a top-level array but a GraphQL response wraps results in `{"data": {"getByServiceId": [...]}}`. Add a new step `the response data array should have N items` that drills into `data.<query>`.

- [ ] **Step 4A.2: Run and confirm scenarios fail**

```bash
cargo test --test groundwork_cert 2>&1 | tail -20
```

Expected: scenarios fail because `/exposes/api` does not exist.

### Task 4B: JSON schema + GraphQL schema

**Files:**
- Create: `groundwork/config/json/exposes.schema.json`
- Create: `groundwork/config/graph/exposes.graphql`

- [ ] **Step 4B.1: Write the JSON schema**

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["deployable_id", "service_id"],
  "properties": {
    "deployable_id": { "type": "string" },
    "service_id": { "type": "string" },
    "port": { "type": "string" },
    "protocol": { "type": "string" }
  },
  "additionalProperties": true
}
```

- [ ] **Step 4B.2: Write the GraphQL schema**

```graphql
type Exposes {
    id: ID
    deployable_id: String!
    service_id: String!
    port: String
    protocol: String
}
type Query {
    getById(id: ID, at: Float): Exposes
    getAll(at: Float): [Exposes]
    getByDeployableId(deployable_id: String, at: Float): [Exposes]
    getByServiceId(service_id: String, at: Float): [Exposes]
}
```

### Task 4C: Wire exposes into `main.rs`

**Files:**
- Modify: `groundwork/src/main.rs`

- [ ] **Step 4C.1: Add the include_str + entity build + Graphlette + Restlette**

Mirror the dependency wiring exactly. New const at top:

```rust
const EXPOSES_GRAPHQL: &str = include_str!("../config/graph/exposes.graphql");
```

In `main()`, after `dependency`:

```rust
let exposes = make_entity(&data_dir, "exposes").await;
```

Load the schema:

```rust
let exposes_schema_json: serde_json::Value =
    serde_json::from_str(include_str!("../config/json/exposes.schema.json"))
        .expect("invalid exposes schema JSON");
```

Build `exposes_gql_config`:

```rust
let exposes_gql_config = RootConfig::builder()
    .singleton("getById", r#"{"id": "{{id}}"}"#)
    .vector("getAll", "{}")
    .vector("getByDeployableId", r#"{"payload.deployable_id": "{{deployable_id}}"}"#)
    .vector("getByServiceId", r#"{"payload.service_id": "{{service_id}}"}"#)
    .build();
```

Add to `ServerConfig.graphlettes`:

```rust
GraphletteConfig {
    path: "/exposes/graph".into(),
    schema_text: EXPOSES_GRAPHQL.into(),
    root_config: exposes_gql_config,
    searcher: exposes.searcher,
},
```

Build the restlette and merge:

```rust
let exposes_restlette = meshql_server::build_restlette_router_ext(
    "/exposes/api",
    exposes.repo,
    auth.clone(),
    None,
    Some(make_required_validator(&exposes_schema_json)),
    None,
    None,
);
// ...
.merge(exposes_restlette)
```

### Task 4D: Wire exposes into the test server

**Files:**
- Modify: `groundwork/tests/groundwork_cert.rs`

- [ ] **Step 4D.1: Mirror `main.rs` for exposes in `build_test_server`**

By this point Slice 3 has already extracted the multi-entity wiring helper. Add `exposes` to that list using the new schema files.

### Task 4E: Wire exposes into the UI

**Files:**
- Modify: `groundwork/static/app.js`
- Modify: `groundwork/static/index.html`

- [ ] **Step 4E.1: Add the `exposes` entry to `ENTITIES` in `app.js`**

```js
exposes: {
  api: '/exposes/api',
  label: 'exposes',
  newFields: [
    { name: 'deployable_id', label: 'deployable', type: 'dynamic-select', required: true,
      optionsFrom: (data) => data.deployables.map(d => ({ value: d.id, label: d.payload?.name || d.id })) },
    { name: 'service_id', label: 'service', type: 'dynamic-select', required: true,
      optionsFrom: (data) => data.services.map(s => ({ value: s.id, label: s.payload?.name || s.id })) },
    { name: 'port', label: 'port', type: 'text', required: false },
    { name: 'protocol', label: 'protocol', type: 'select', required: false,
      options: ['', 'http', 'https', 'grpc', 'tcp', 'udp', 'other'] },
  ],
  detailFields: [
    { name: 'port', label: 'port', type: 'text' },
    { name: 'protocol', label: 'protocol', type: 'select',
      options: ['', 'http', 'https', 'grpc', 'tcp', 'udp', 'other'] },
  ],
  primaryField: 'deployable_id',
  getRowLabel: (payload, data) => {
    const dep = data.deployables.find(d => d.id === payload.deployable_id)?.payload?.name
      || payload.deployable_id || '?';
    const svc = data.services.find(s => s.id === payload.service_id)?.payload?.name
      || payload.service_id || '?';
    return `${dep} ⇒ ${svc}`;
  },
  getRowBadge: (payload) => payload.protocol || null,
  readonlyInDetail: [
    { name: 'deployable_id', label: 'deployable',
      resolve: (payload, data) => data.deployables.find(d => d.id === payload.deployable_id)?.payload?.name || payload.deployable_id || '—' },
    { name: 'service_id', label: 'service',
      resolve: (payload, data) => data.services.find(s => s.id === payload.service_id)?.payload?.name || payload.service_id || '—' },
  ],
},
```

In `state.data` initial object, add `exposes: []`.

- [ ] **Step 4E.2: Add the `exposes` sidebar item in `index.html`**

Insert after the `deployables` nav-item, before `services`:

```html
<div class="nav-item" data-entity="exposes">
  <span>exposes</span>
  <span class="nav-badge" id="badge-exposes">0</span>
</div>
```

(Order in the sidebar is reader-friendly: deployables, exposes, services, dependencies, contracts, slas — runtime → interfaces → consumption.)

### Task 4F: Verify and commit

- [ ] **Step 4F.1: Run the full suite**

```bash
cargo test --test groundwork_cert 2>&1 | tail -30
```

Expected: all scenarios pass (deployable, dependency, exposes, web_ui, plus the original tech_stack one).

- [ ] **Step 4F.2: Manual UI smoke test**

```bash
cargo run &
sleep 2
xdg-open http://localhost:3000 || open http://localhost:3000 || echo "open http://localhost:3000 in a browser"
```

Confirm in the browser:
1. Sidebar shows: deployables, exposes, services, dependencies, contracts, slas (with counts).
2. Pressing `n` on the `exposes` tab opens a new-record form with deployable + service selectors.
3. After creating a record, the row label reads `<deployable-name> ⇒ <service-name>`.

Stop the server (`fg` then Ctrl-C, or `kill %1`).

- [ ] **Step 4F.3: Commit**

```bash
git add -A
git commit -m "feat: Exposes relationship (Deployable exposes Service)"
```

---

## Definition of done — Phase 1

- [ ] All cucumber scenarios pass: `cargo test --test groundwork_cert` shows 0 failures and ≥18 scenarios (12 from v0.1, plus tech_stack-removal, plus dependency.feature, plus 6 exposes scenarios).
- [ ] `cargo build --release` succeeds with no warnings introduced by this phase.
- [ ] Manual UI smoke shows all six entities working, including Exposes.
- [ ] No reference to `application` (lowercase) or `Application` (uppercase, type) remains in `groundwork/src`, `groundwork/config`, `groundwork/static`, or `groundwork/tests` (other than incidental words like "applicable"). Verify with:

  ```bash
  rg -n '\b[Aa]pplication\b' groundwork/src groundwork/config groundwork/static groundwork/tests
  ```

  Expected: empty.

- [ ] No reference to `tech_stack` remains. Verify with:

  ```bash
  rg -n 'tech_stack' groundwork/
  ```

  Expected: empty.

- [ ] Master plan updated to reflect any architecture decisions made during execution (e.g. if we decide to rename the dev DB files, document that).

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| The `deployable` GraphQL `__type` introspection may not be supported by `meshql-graphlette`. | If introspection fails, replace the tech_stack test with a less-ambitious check: `POST /deployable/api` with `tech_stack` set, then `GET` and confirm `tech_stack` is round-tripped (because additionalProperties:true) but the GraphQL `getById` query for `tech_stack` returns a parse error. The point is to lock in the schema shape, not the introspection. |
| Renaming breaks `meshql-graphlette`'s schema text validation if the old schema text is cached. | Cargo will rebuild on file change; no runtime cache exists. |
| Test server divergence from main: easy to forget to mirror an entity. | Slice 3 extracts a helper that takes `(name, schema_path, gql_path, root_config)` and is reused by both `main.rs` and `groundwork_cert.rs`. **Optional refinement** — left as Phase 1 stretch goal; if not done, add a comment at top of `build_test_server` listing the entities that must match `main.rs`. |
| The UI's `dependencies` block still references `applications` after slice 2 in some places. | `rg -n 'application' groundwork/static/app.js` after slice 2; expect zero matches before committing slice 3. |
