# REST → /graph Read-Path Migration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate all four manifold frontends (groundwork, union, cityhall, yard) from REST `/<entity>/api` reads to GraphQL `/<entity>/graph` reads, bringing the suite into line with the documented MeshQL CQRS pattern: writes go to REST, reads come from the federated graph.

**Architecture:** Each app already exposes per-entity Graphlette endpoints (`/deployable/graph`, `/team/graph`, `/org_node/graph`, etc.) with `getAll`, `getById`, `getByName` queries, and federation resolvers are wired (e.g. `Deployable.team` resolves to `Union.Team` via Union's `/team/graph`). The migration is purely frontend: replace each `apiList()` REST call with a small GraphQL POST that selects exactly the fields the UI renders, including federated foreign-key fields where applicable.

**Tech Stack:** Vanilla JS (no framework, no build step), fetch API, MeshQL-RS server (already wired), cucumber BDD tests in `<app>/tests/features/` for end-to-end verification.

**Workflow:** Trunk-Based Development. Every task commits directly to `main`. Each entity migration is independently shippable; partial migration is safe (frontend can hold a mix of REST and GraphQL reads transitionally per app, but each entity should be migrated atomically).

**Out of scope (named explicitly so they don't drift in):**
- Auth integration — next slice after this lands.
- Selection-level deeplinks (`#/<entity>/<id>`) and `pushState` migration — separate concerns.
- Any new features (Person dropdown, cross-app linking, etc.) — defer until on top of the migrated read path.
- Backend changes — graphlettes and federation are already wired. If this plan discovers a missing resolver, that's a sub-task on the affected entity, not a precondition.

---

## File Structure

### Per-app frontend (modified)

| App | File | Entities to migrate (REST → graph) |
|-----|------|-------|
| groundwork | `groundwork/static/app.js` | deployables, services, dependencies, exposes, contracts, slas |
| union | `union/static/app.js` | persons, teams, team_members, work_orders |
| cityhall | `cityhall/static/app.js` | org_nodes, bylaws, change_requests, deployment_plans, gantt_outputs |
| yard | `yard/static/app.js` | test_environments, test_infrastructures, mock_sources, data_sources, data_syncs, test_runs, test_suites |

Each file gets a small `gqlQuery(path, query, variables)` helper (~12 lines, copy-paste per app — these four files do not currently share JS), and each entity's `apiList` call is replaced with a `gqlQuery` to that entity's `/graph`.

### Per-app integration tests (no changes expected, only verification)

- `groundwork/tests/features/`, `union/tests/features/`, etc. — existing cucumber BDD scenarios. They should continue to pass unchanged because the REST endpoints are untouched.

---

## Pilot: groundwork (Tasks 1–10)

groundwork is the pilot because: smallest entity count among the four with non-trivial federation, has the canonical `Deployable.team → Union.Team` federation example, no inter-entity dependencies in the load order, and its frontend pattern (centralized `ENTITIES` config) is the simplest to refactor.

### Task 1: Verify `/graph` works for every groundwork entity

**Files:** none modified — verification only.

- [ ] **Step 1: Curl each graphlette and confirm a getAll response**

```bash
for entity in deployable service dependency exposes contract sla; do
  echo "=== $entity ==="
  curl -sS -X POST "http://localhost:3050/$entity/graph" \
    -H 'Content-Type: application/json' \
    -d '{"query":"{ getAll { id } }"}' | jq -r '.errors // .data.getAll | length'
done
```

Expected: each entity returns a number (count of records), no `errors` array. If any entity returns a GraphQL error, that's a backend prerequisite — fix the schema/resolver in `groundwork/config/graph/<entity>.graphql` and `groundwork/src/main.rs` *before* migrating that entity's frontend call.

- [ ] **Step 2: Curl the federated field and confirm Union round-trip**

```bash
curl -sS -X POST http://localhost:3050/deployable/graph \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ getAll { id name team { id name kind } } }"}' | jq '.data.getAll[0]'
```

Expected: the first deployable's `team` field is hydrated (`{ id, name, kind }`) — proves federation to Union works through the running stack.

- [ ] **Step 3: Note any baseline issues**

If steps 1–2 surface anything, document each issue in this section as a sub-task and resolve before continuing. Otherwise: proceed.

### Task 2: Add `gqlQuery` helper to `groundwork/static/app.js`

**Files:**
- Modify: `groundwork/static/app.js` — add helper near the existing `api()` function (search for `async function api(`)

- [ ] **Step 1: Read the existing `api()` helper for style/error-handling conventions**

```bash
grep -n -A 15 'async function api(' groundwork/static/app.js
```

Match its error-handling shape (throw on non-2xx, parse JSON, etc.).

- [ ] **Step 2: Add `gqlQuery()` helper**

Append to the same `// ── Network ──` (or equivalent) section:

```javascript
async function gqlQuery(path, query, variables = {}) {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, variables }),
  });
  if (!res.ok) throw new Error(`graph ${path} ${res.status}`);
  const body = await res.json();
  if (body.errors && body.errors.length) {
    throw new Error(body.errors.map(e => e.message).join('; '));
  }
  return body.data;
}
```

- [ ] **Step 3: Verify syntax**

```bash
node --check groundwork/static/app.js
```

Expected: no output (success).

- [ ] **Step 4: Verify in browser**

Rebuild and reload `https://groundwork.tildarc.com`. Open devtools console. Run:

```javascript
await gqlQuery('/deployable/graph', '{ getAll { id name } }')
```

Expected: returns `{ getAll: [...] }` with deployable rows.

- [ ] **Step 5: Commit**

```bash
git add groundwork/static/app.js
git commit -m "feat(groundwork): add gqlQuery helper for /graph reads

Tiny vanilla helper that POSTs a GraphQL query and unwraps data/errors
the same way as the existing api() helper. First step of the REST → /graph
read-path migration; not yet wired into any entity loader."
git push origin main
```

### Tasks 3–8: Migrate each entity's read

For each of the six groundwork entities, in order: deployables, services, dependencies, exposes, contracts, slas. Use the same task shape, parameterized by entity:

#### Task 3: Migrate `deployables` from REST to GraphQL

**Files:**
- Modify: `groundwork/static/app.js` — `loadEntity()` function and the `ENTITIES.deployables` config

- [ ] **Step 1: Identify the fields the UI actually renders for `deployables`**

Read the `ENTITIES.deployables` block in `groundwork/static/app.js` (search for `deployables: {`). Note the `primaryField`, `getRowLabel`, `detailFields`, and any cross-entity references. For deployables, the UI uses: `id`, `name`, `description`, `repo_url`, `team_id`, plus the federated `team` (for displaying the team name in lists).

- [ ] **Step 2: Define the query string**

Add to the `ENTITIES.deployables` config, alongside the existing `api: '/deployable/api'` line:

```javascript
graph: {
  path: '/deployable/graph',
  list: '{ getAll { id name description repo_url team_id team { id name } } }',
},
```

- [ ] **Step 3: Update `loadEntity` to prefer `graph.list` if present, else fall back to REST**

Find `async function loadEntity(entityKey)`. Replace its body with:

```javascript
async function loadEntity(entityKey) {
  const cfg = ENTITIES[entityKey];
  let items;
  if (cfg.graph?.list) {
    const data = await gqlQuery(cfg.graph.path, cfg.graph.list);
    items = data.getAll;
  } else {
    items = await apiList(cfg.api);
  }
  state.data[entityKey] = Array.isArray(items) ? items : [];
  updateBadge(entityKey);
}
```

(The fallback branch lets the migration proceed entity-by-entity on the same branch without breaking unmigrated entities.)

- [ ] **Step 4: Verify syntax + smoke test**

```bash
node --check groundwork/static/app.js
cargo build -p groundwork
```

Restart groundwork (or `docker compose up -d --build groundwork`), reload `https://groundwork.tildarc.com`, click into the deployables list. Verify: rows render with names and the team column shows team names (proves federation worked).

- [ ] **Step 5: Commit**

```bash
git add groundwork/static/app.js
git commit -m "feat(groundwork): migrate deployables read to /graph

Replaces /deployable/api list call with a /deployable/graph getAll query
that selects only the fields the UI renders, including the federated
team { id name } field. The team name now arrives in the same round-trip
rather than via a separate REST fetch+join in the browser.

Other entities still use the REST fallback in loadEntity; will migrate
each in its own commit."
git push origin main
```

#### Task 4: Migrate `services`

Repeat Task 3's pattern. The services entity has no federated fields. Query:

```javascript
graph: {
  path: '/service/graph',
  list: '{ getAll { id name type description endpoint } }',
},
```

Commit message: `feat(groundwork): migrate services read to /graph`. Same verification: reload, click services, confirm rows render.

#### Task 5: Migrate `dependencies`

```javascript
graph: {
  path: '/dependency/graph',
  list: '{ getAll { id deployable_id service_id protocol auth_method criticality } }',
},
```

The dependencies UI cross-references deployable/service names in `getRowLabel` from already-loaded `state.data`, so no inline federation is required here. Verify rows render with the existing cross-referenced names.

#### Task 6: Migrate `exposes`

```javascript
graph: {
  path: '/exposes/graph',
  list: '{ getAll { id deployable_id service_id port protocol } }',
},
```

#### Task 7: Migrate `contracts`

```javascript
graph: {
  path: '/contract/graph',
  list: '{ getAll { id service_id spec_url version format } }',
},
```

#### Task 8: Migrate `slas`

```javascript
graph: {
  path: '/sla/graph',
  list: '{ getAll { id contract_id metric target window } }',
},
```

### Task 9: Drop the REST fallback path from `loadEntity`

**Files:**
- Modify: `groundwork/static/app.js` — `loadEntity()` and `ENTITIES.*.api` lines

- [ ] **Step 1: Remove the REST fallback branch**

`loadEntity()` becomes:

```javascript
async function loadEntity(entityKey) {
  const cfg = ENTITIES[entityKey];
  const data = await gqlQuery(cfg.graph.path, cfg.graph.list);
  state.data[entityKey] = Array.isArray(data.getAll) ? data.getAll : [];
  updateBadge(entityKey);
}
```

- [ ] **Step 2: Keep the `api: '/<entity>/api'` line on each ENTITIES config**

Writes still go to REST per the CQRS rule. The `api` field continues to be used by `createRecord`, `updateRecord`, `deleteRecord`. Do not remove these.

- [ ] **Step 3: Verify all six lists still render**

Reload the app, click through each sidebar item, confirm each list populates.

- [ ] **Step 4: Commit**

```bash
git add groundwork/static/app.js
git commit -m "refactor(groundwork): drop REST fallback now that all entities read via /graph

Reads are uniform on /graph; writes remain on /api per the documented
MeshQL CQRS rule."
git push origin main
```

### Task 10: groundwork cucumber tests still pass

- [ ] **Step 1: Run the cert suite**

```bash
cargo test -p groundwork --test groundwork_cert
```

Expected: all scenarios pass. If a scenario fails because it previously hit `/<entity>/api` and now expects `/graph`-shaped state, that's a bug in the test that pre-existed; resolve in a follow-up if non-trivial.

- [ ] **Step 2: Optional — add a thin cucumber feature for the graph read**

If `groundwork/tests/features/` does not contain a scenario that exercises the graph endpoint, add one to anchor the migration:

`groundwork/tests/features/graph_read.feature`:

```gherkin
Feature: GraphQL reads expose entities through /graph

  Scenario: getAll deployables via the graph endpoint
    Given the groundwork app is running
    When I POST to /deployable/graph with query "{ getAll { id name } }"
    Then the response has at least one deployable
    And each deployable has an id and a name

  Scenario: federated team field hydrates from Union
    Given a deployable exists with a team_id
    When I POST to /deployable/graph with query "{ getAll { id team { id name } } }"
    Then the deployable's team field is non-null
    And it has both id and name
```

The step bindings live in `groundwork/tests/common/`. If equivalent helpers don't exist for graph queries, add a small `post_graphql(path, query)` helper there.

- [ ] **Step 3: Commit (if step 2 was needed)**

```bash
git add groundwork/tests/features/graph_read.feature groundwork/tests/common/
git commit -m "test(groundwork): cucumber scenarios for /graph reads + federation"
git push origin main
```

---

## Roll-out: union (Tasks 11–14)

Same pattern as groundwork. Tasks abbreviated; follow the same TDD shape (verify graph endpoint → write query → swap loader → verify in browser → commit).

### Task 11: Verify union's `/graph` endpoints

- [ ] Curl `/person/graph`, `/team/graph`, `/team_member/graph`, `/work_order/graph` with `{ getAll { id } }`. Confirm each returns data, no errors.
- [ ] Curl federated WorkOrder.deployable: `curl -X POST http://localhost:3051/work_order/graph -d '{"query":"{ getAll { id summary deployable { id name } } }"}'` — confirm `deployable` hydrates.

### Task 12: Add `gqlQuery` helper to `union/static/app.js`

Identical helper to groundwork's. Copy-paste; commit with the same shape of message.

### Task 13: Migrate each union entity, one commit per entity

For union, each entity's `loadAll` shape:

| Entity | Query |
|--------|-------|
| persons | `{ getAll { id name contact role } }` |
| teams | `{ getAll { id name kind description } }` |
| team_members | `{ getAll { id person_id team_id role } }` |
| work_orders | `{ getAll { id team_id summary deployable_id deployable { id name } change_request_id status priority } }` |

Note: `work_orders` already gains federation here (`deployable` and, if wired, `change_request`). Verify the federated field populates after each migration.

### Task 14: Drop REST fallback in union; cucumber green

Same as groundwork Task 9–10.

---

## Roll-out: cityhall (Tasks 15–19)

### Task 15: Verify cityhall's `/graph` endpoints

- [ ] Curl `/org_node/graph`, `/bylaw/graph`, `/change_request/graph`, `/deployment_plan/graph`, `/gantt_output/graph` with `{ getAll { id } }`.
- [ ] Curl federated `OrgNode.team` and `ChangeRequest.requested_by` if wired. If `requested_by` is *not* federated to Union.Person yet (the README documents the intent but the code may not yet do it), this becomes a sub-task: add the resolver to `cityhall/src/main.rs` and the projection to `cityhall/config/graph/change_request.graphql`. Commit that backend wiring before the frontend migration of `change_requests`.

### Task 16: Add `gqlQuery` helper

### Task 17: Migrate each cityhall entity

| Entity | Query |
|--------|-------|
| org_nodes | `{ getAll { id name kind parent_id team_id team { id name kind } } }` |
| bylaws | `{ getAll { id org_node_id gate_type priority conditions window quiesce_for } }` |
| change_requests | `{ getAll { id summary target_deployables target_versions requested_by requestedBy { id name } } }` (or whatever the federated field is named — confirm in the schema) |
| deployment_plans | `{ getAll { id change_request_id steps gates windows blockers } }` |
| gantt_outputs | `{ getAll { id deployment_plan_id mermaid tier } }` |

### Task 18: Update Person dropdown / requested_by display

The previous-session's stashed work added a Person dropdown in cityhall sourced from a BFF proxy. The graph-based version of this UI element is now possible: query `change_requests` with the federated `requested_by { id name }` and render the name directly. If the user wants the dropdown of *all* persons (not just hydrated ones), Cityhall's GraphQL needs a query against Union's person list — wire it as a top-level federated query (e.g., `availablePersons: [Person]`) in `cityhall/config/graph/change_request.graphql` and add the resolver. Commit the resolver wiring before the dropdown frontend.

This task may grow; if it does, split it into its own follow-up plan rather than expanding this one.

### Task 19: Drop REST fallback; cucumber green

---

## Roll-out: yard (Tasks 20–23)

Same pattern. Yard has the most entities (7) so it's the longest of the four, but the simplest in terms of federation (most entities only reference Groundwork via `deployable_id`/`service_id` and Union via `team_id` / `change_request_id`).

### Task 20: Verify yard's `/graph` endpoints

### Task 21: Add `gqlQuery` helper

### Task 22: Migrate each yard entity

| Entity | Query |
|--------|-------|
| test_environments | `{ getAll { id name kind deployable_id deployable { id name } service_id mock_source_id ... } }` |
| test_infrastructures | `{ getAll { id name provider region instance_type cost_per_hour notes } }` |
| mock_sources | `{ getAll { id name repo_url path language notes } }` |
| data_sources | `{ getAll { id name kind location refresh_policy notes } }` |
| data_syncs | `{ getAll { id kind target_env_id source_env_id source_data_id refresh_policy estimated_minutes notes } }` |
| test_runs | `{ getAll { id test_environment_id change_request_id team_id team { id name } status duration_minutes cost_actual } }` |
| test_suites | `{ getAll { id name deployable_id deployable { id name } runner command description } }` |

### Task 23: Drop REST fallback in yard; cucumber green

---

## Final verification (Task 24)

- [ ] Each app's frontend reads exclusively from `/graph` (no `apiList`-style REST GET calls remain in `static/app.js`).
- [ ] Each app's writes (create/update/delete) still go to `/<entity>/api` (REST). Spot-check by creating a record in each app's UI and verifying the network tab.
- [ ] All four apps' cucumber suites pass: `cargo test --workspace`.
- [ ] Federation is exercised in the UI: Deployable shows team name; ChangeRequest shows requested-by name; WorkOrder shows deployable name; TestRun shows team name.

- [ ] **Commit any final tidying**

---

## Migration debt notes

- Selection-level deeplinks and `pushState` migration are deferred. After the read-path migration lands, the `gqlQuery({ getById })` shape makes per-entity deeplinks straightforward — the deeplink scheme should be planned after this migration, not before.
- The `Cache-Control: no-cache` setting on `/static/app.js` (commit `a7304f3`) keeps app.js fresh during this rolling migration. Long-cache + asset versioning is a separate concern.
- The dirty-state stash (`stash@{0}` on main, message starts "WIP from previous Claude session 2026-05-08") contains the previous session's deeplink + BFF work. After this migration lands and proves out the graph read pattern, drop the stash — its features (deeplinks, person dropdown) can be redesigned cleanly on top of `/graph`.
